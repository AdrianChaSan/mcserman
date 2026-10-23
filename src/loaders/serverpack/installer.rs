use anyhow::{bail, Context, Result};
use std::path::Path;

use super::archive::{download_serverpack_file, extract_serverpack_zip};
use super::curseforge::resolve_serverpack;
use super::detector::detect_server_info;
use crate::core::config::{is_legacy_minecraft_version, InstanceConfig, LoaderType};
use crate::core::paths::SystemPaths;
use crate::core::progress::ProgressReporter;
use crate::core::properties::ServerProperties;
use crate::modrinth::modpack::ResolvedModpack;

pub const SERVER_STARTER_JAR_URL: &str =
    "https://github.com/neoforged/ServerStarterJar/releases/latest/download/server.jar";

/// Descarga el ejecutable ServerStarterJar oficial utilizado por instaladores modernos de NeoForge y Forge.
pub async fn download_server_starter_jar(
    client: &reqwest::Client,
    target_path: &Path,
    reporter: &dyn ProgressReporter,
) -> Result<()> {
    reporter.on_start("Descargando ServerStarterJar oficial de NeoForge...", 1);
    download_serverpack_file(client, SERVER_STARTER_JAR_URL, target_path, reporter).await?;
    reporter.on_finish("ServerStarterJar descargado correctamente");
    Ok(())
}

/// Consulta la versión recomendada o última de Forge para una versión dada de Minecraft.
pub async fn fetch_forge_version(client: &reqwest::Client, mc_version: &str) -> Result<String> {
    let url = "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
    let resp = client
        .get(url)
        .header(
            reqwest::header::USER_AGENT,
            concat!("mcserman/", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .context("Error conectando al repositorio de promociones de Minecraft Forge")?;

    if !resp.status().is_success() {
        bail!("Error HTTP {} al obtener versiones de Forge", resp.status());
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .context("Error al parsear JSON de promociones de Forge")?;
    if let Some(promos) = data.get("promos").and_then(|p| p.as_object()) {
        let rec_key = format!("{}-recommended", mc_version);
        if let Some(v) = promos.get(&rec_key).and_then(|v| v.as_str()) {
            return Ok(v.to_string());
        }
        let lat_key = format!("{}-latest", mc_version);
        if let Some(v) = promos.get(&lat_key).and_then(|v| v.as_str()) {
            return Ok(v.to_string());
        }
    }
    bail!(
        "No se encontró una versión de Forge compatible con Minecraft {}",
        mc_version
    );
}

/// Descarga e instala Forge para versiones legacy (<= 1.16.5) utilizando el instalador oficial
/// con la opción `--installServer` y la JVM de Java 8.
pub async fn install_legacy_forge_server(
    client: &reqwest::Client,
    mc_version: &str,
    forge_version: &str,
    instance_dir: &Path,
    target_jar: &Path,
    reporter: &dyn ProgressReporter,
) -> Result<()> {
    let installer_name = "forge-installer.jar";
    let installer_path = instance_dir.join(installer_name);
    let forge_installer_url = format!(
        "https://files.minecraftforge.net/maven/net/minecraftforge/forge/{}-{}/forge-{}-{}-installer.jar",
        mc_version, forge_version, mc_version, forge_version
    );

    reporter.on_start(
        &format!("Descargando instalador de Forge {}...", forge_version),
        1,
    );
    download_serverpack_file(client, &forge_installer_url, &installer_path, reporter).await?;
    reporter.on_finish("Instalador de Forge descargado");

    let java_ver = crate::core::java::required_java_version(mc_version);
    let paths = SystemPaths::resolve();
    let java_runtime = crate::core::java::resolve_java(java_ver, &paths, reporter)
        .await
        .with_context(|| {
            format!(
                "Se requiere Java {} para ejecutar el instalador de Forge legacy",
                java_ver
            )
        })?;

    reporter.on_start("Instalando servidor Forge (--installServer)...", 1);
    let mut cmd = java_runtime.build_tokio_command(&[
        "-jar".to_string(),
        installer_name.to_string(),
        "--installServer".to_string(),
    ]);
    cmd.current_dir(instance_dir);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .context("Error al iniciar el instalador de Forge")?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let drain_stdout = tokio::spawn(async move {
        if let Some(mut out) = stdout {
            let mut buf = Vec::new();
            let _ = tokio::io::AsyncReadExt::read_to_end(&mut out, &mut buf).await;
        }
    });
    let drain_stderr = tokio::spawn(async move {
        let mut err_msg = String::new();
        if let Some(mut err) = stderr {
            let _ = tokio::io::AsyncReadExt::read_to_string(&mut err, &mut err_msg).await;
        }
        err_msg
    });

    let status = child
        .wait()
        .await
        .context("Error durante la ejecución del instalador de Forge")?;
    let _ = drain_stdout.await;
    let err_msg = drain_stderr.await.unwrap_or_default();

    if !status.success() {
        bail!(
            "El instalador de Forge falló con código {:?}. {}",
            status.code(),
            err_msg.trim()
        );
    }
    reporter.on_finish("Instalación de Forge finalizada");

    // Localizar el jar de Forge generado (forge-*.jar que no sea installer)
    let mut found_forge_jar = None;
    if let Ok(entries) = std::fs::read_dir(instance_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("forge-")
                        && name.ends_with(".jar")
                        && !name.contains("installer")
                    {
                        found_forge_jar = Some(p);
                        break;
                    }
                }
            }
        }
    }

    if let Some(src_jar) = found_forge_jar {
        if src_jar != target_jar {
            let _ = std::fs::rename(&src_jar, target_jar).or_else(|_| {
                std::fs::copy(&src_jar, target_jar)?;
                let _ = std::fs::remove_file(&src_jar);
                Ok::<(), std::io::Error>(())
            });
        }
    } else if !target_jar.exists() {
        bail!("El instalador de Forge terminó pero no se generó el archivo jar del servidor.");
    }

    let _ = std::fs::remove_file(&installer_path);
    let _ = std::fs::remove_file(instance_dir.join("forge-installer.jar.log"));

    Ok(())
}

/// Orquesta la descarga, descompresión, detección e inicialización completa de una instancia desde un Server Pack.
pub async fn install_serverpack(
    instance_name: &str,
    query: &str,
    resolved: Option<ResolvedModpack>,
    ram: &str,
    port: u16,
    paths: &SystemPaths,
    reporter: &dyn ProgressReporter,
) -> Result<String> {
    let resolved = match resolved {
        Some(r) => r,
        None => resolve_serverpack(query).await?,
    };

    let instance_dir = paths.instance_dir(instance_name);
    std::fs::create_dir_all(&instance_dir).with_context(|| {
        format!(
            "No se pudo crear el directorio de la instancia en {}",
            instance_dir.display()
        )
    })?;

    let client = reqwest::Client::builder()
        .user_agent(concat!("mcserman/", env!("CARGO_PKG_VERSION")))
        .build()?;

    let temp_dir = std::env::temp_dir().join(format!(
        "mcserman_sp_{}_{}",
        instance_name,
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir)?;
    let temp_zip_path = temp_dir.join(&resolved.mrpack_filename);

    download_serverpack_file(&client, &resolved.mrpack_url, &temp_zip_path, reporter).await?;
    extract_serverpack_zip(&temp_zip_path, &instance_dir, reporter)?;

    let detected = detect_server_info(&instance_dir);

    // Si la versión proviene de CurseForge o Modrinth, es la fuente de verdad definitiva
    let mc_version = if resolved.version_id != "custom" && !resolved.mc_version.is_empty() {
        resolved.mc_version
    } else {
        detected.mc_version.unwrap_or(resolved.mc_version)
    };
    let loader_str = if resolved.version_id != "custom" && !resolved.loader.is_empty() {
        resolved.loader
    } else {
        detected.loader.unwrap_or(resolved.loader)
    };
    let loader_type = match loader_str.to_lowercase().as_str() {
        "fabric" => LoaderType::Fabric,
        "paper" => LoaderType::Paper,
        "neoforge" => LoaderType::NeoForge,
        "forge" => LoaderType::Forge,
        "vanilla" => LoaderType::Vanilla,
        _ => LoaderType::Fabric,
    };
    let loader_version = detected.loader_version.or(resolved.loader_version);

    let server_jar = if let Some(jar) = detected.server_jar {
        jar
    } else {
        match loader_type {
            LoaderType::Fabric => {
                let jar_name = format!("fabric-server-{}.jar", mc_version);
                let target_path = instance_dir.join(&jar_name);
                reporter.on_start(&format!("Descargando Fabric Server {}...", mc_version), 1);
                crate::loaders::fabric::download_fabric_custom(
                    &mc_version,
                    loader_version.as_deref(),
                    &target_path,
                    reporter,
                )
                .await
                .context("Error descargando Fabric Server JAR para el Server Pack")?;
                reporter.on_finish("Fabric Server instalado correctamente");
                jar_name
            }
            LoaderType::Paper => {
                let jar_name = format!("paper-{}.jar", mc_version);
                let target_path = instance_dir.join(&jar_name);
                reporter.on_start(&format!("Descargando Paper Server {}...", mc_version), 1);
                crate::loaders::paper::download_paper(&mc_version, &target_path, reporter).await?;
                reporter.on_finish("Paper Server instalado correctamente");
                jar_name
            }
            LoaderType::Vanilla => {
                let jar_name = "server.jar".to_string();
                let target_path = instance_dir.join(&jar_name);
                reporter.on_start(&format!("Descargando Vanilla Server {}...", mc_version), 1);
                crate::loaders::vanilla::download_vanilla(&mc_version, &target_path, reporter)
                    .await?;
                reporter.on_finish("Vanilla Server instalado correctamente");
                jar_name
            }
            LoaderType::NeoForge | LoaderType::Forge => {
                if is_legacy_minecraft_version(&mc_version) {
                    let target_jar_name = "forge.jar".to_string();
                    let target_path = instance_dir.join(&target_jar_name);
                    if !target_path.is_file() {
                        let forge_ver = if let Some(ref ver) = loader_version {
                            ver.clone()
                        } else {
                            fetch_forge_version(&client, &mc_version).await?
                        };
                        install_legacy_forge_server(
                            &client,
                            &mc_version,
                            &forge_ver,
                            &instance_dir,
                            &target_path,
                            reporter,
                        )
                        .await?;
                    }
                    target_jar_name
                } else {
                    let jar_name = "server.jar".to_string();
                    let target_path = instance_dir.join(&jar_name);
                    if !target_path.is_file() {
                        reporter.on_start(
                            &format!("Descargando ServerStarterJar para {}...", loader_type),
                            1,
                        );
                        download_server_starter_jar(&client, &target_path, reporter).await?;
                        reporter.on_finish("ServerStarterJar instalado correctamente");
                    }
                    jar_name
                }
            }
        }
    };

    let eula_path = instance_dir.join("eula.txt");
    std::fs::write(&eula_path, "eula=true\n")
        .context("No se pudo escribir eula.txt con eula=true")?;

    let props_path = instance_dir.join("server.properties");
    let mut props = ServerProperties::load_or_default(&props_path);
    props.set("server-port", &port.to_string());
    props.save(&props_path)?;

    let mut config = InstanceConfig::new(instance_name.to_string(), loader_type, mc_version);
    config.loader_version = loader_version;
    config.server_jar = server_jar;
    config.ram_min = "2G".to_string();
    config.ram_max = ram.to_string();
    config.port = port;
    config.eula = true;
    config.source_pack = Some(format!("serverpack:{}", resolved.project_title));
    config.modpack_info = resolved.modpack_info;
    config.save(&paths.instance_config(instance_name))?;

    let _ = std::fs::remove_dir_all(&temp_dir);

    reporter.on_finish(&format!(
        "Instancia '{}' creada exitosamente desde Server Pack",
        instance_name
    ));
    Ok(instance_name.to_string())
}
