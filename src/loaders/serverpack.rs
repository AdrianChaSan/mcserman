use crate::core::config::{InstanceConfig, LoaderType};
use crate::core::paths::SystemPaths;
use crate::core::progress::ProgressReporter;
use crate::core::properties::ServerProperties;
use crate::modrinth::modpack::{sanitize_instance_name, PackType, ResolvedModpack};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::Path;

const CURSEFORGE_API_URL: &str = "https://api.curseforge.com/v1/mods/files";
const DEFAULT_CURSEFORGE_KEY: &str = "$2a$10$bL4bIL5pUWqfcO7KQtnMReakwtfHbNKh6v1uTpKlzhwoueEJQnPnm";

pub const SERVER_STARTER_JAR_URL: &str =
    "https://github.com/neoforged/ServerStarterJar/releases/latest/download/server.jar";

#[derive(Debug, Deserialize)]
struct CfFilesResponse {
    data: Vec<CfFileInfo>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct CfFileInfo {
    id: u64,
    display_name: String,
    file_name: String,
    download_url: Option<String>,
    file_length: Option<u64>,
    game_versions: Option<Vec<String>>,
    is_server_pack: Option<bool>,
    server_pack_file_id: Option<u64>,
}

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

pub fn is_curseforge_or_zip(input: &str) -> bool {
    is_curseforge_url(input) || is_zip_url(input)
}

pub fn is_curseforge_url(input: &str) -> bool {
    let lower = input.to_lowercase();
    lower.contains("curseforge.com")
}

pub fn is_zip_url(input: &str) -> bool {
    let lower = input.to_lowercase();
    if let Some(clean) = lower.split('?').next() {
        clean.ends_with(".zip")
    } else {
        lower.ends_with(".zip")
    }
}

pub fn parse_curseforge_url(input: &str) -> (Option<String>, Option<u64>) {
    let trimmed = input.trim();
    let without_proto = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);

    let clean = without_proto.trim_end_matches('/');
    let parts: Vec<&str> = clean.split('/').collect();

    let mut file_id = None;
    let mut slug = None;

    // Buscar "files" o "download" seguido de un número
    for (i, part) in parts.iter().enumerate() {
        if (*part == "files" || *part == "download") && i + 1 < parts.len() {
            if let Ok(id) = parts[i + 1].parse::<u64>() {
                file_id = Some(id);
                // Si antes de "files" hay un segmento que no sea "modpacks" o "mc-mods"
                if i > 0 {
                    let candidate = parts[i - 1];
                    if candidate != "modpacks" && candidate != "mc-mods" && candidate != "mods" {
                        slug = Some(candidate.to_string());
                    } else if i >= 2 {
                        slug = Some(parts[i - 1].to_string());
                    }
                }
                break;
            }
        }
    }

    (slug, file_id)
}

pub async fn resolve_serverpack(query: &str) -> Result<ResolvedModpack> {
    let trimmed = query.trim();
    if is_curseforge_url(trimmed) {
        resolve_curseforge_url(trimmed).await
    } else if is_zip_url(trimmed) {
        resolve_direct_zip_url(trimmed).await
    } else {
        bail!("No se reconoce como una URL válida de CurseForge o archivo .zip");
    }
}

async fn fetch_cf_file_info(
    client: &reqwest::Client,
    api_key: &str,
    file_id: u64,
) -> Result<CfFileInfo> {
    let payload = serde_json::json!({
        "fileIds": [file_id]
    });

    let resp = client
        .post(CURSEFORGE_API_URL)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("x-api-key", api_key)
        .json(&payload)
        .send()
        .await
        .with_context(|| {
            format!(
                "Error conectando a CurseForge API para el archivo {}",
                file_id
            )
        })?;

    if !resp.status().is_success() {
        bail!(
            "CurseForge API retornó error HTTP {}: {}",
            resp.status(),
            file_id
        );
    }

    let parsed: CfFilesResponse = resp.json().await.with_context(|| {
        format!(
            "Error al parsear metadatos de CurseForge API para el archivo {}",
            file_id
        )
    })?;

    parsed
        .data
        .into_iter()
        .find(|f| f.id == file_id)
        .with_context(|| {
            format!(
                "CurseForge no devolvió información para el archivo {}",
                file_id
            )
        })
}

pub async fn resolve_curseforge_url(url: &str) -> Result<ResolvedModpack> {
    let (slug_opt, file_id_opt) = parse_curseforge_url(url);
    let file_id = file_id_opt.with_context(|| {
        "No se pudo extraer el ID numérico de archivo de la URL de CurseForge (ej. /files/7522918)"
    })?;

    let client = reqwest::Client::builder()
        .user_agent(concat!("mcserman/", env!("CARGO_PKG_VERSION")))
        .build()?;

    let api_key = std::env::var("CURSEFORGE_API_KEY")
        .or_else(|_| std::env::var("CF_API_KEY"))
        .unwrap_or_else(|_| DEFAULT_CURSEFORGE_KEY.to_string());

    let mut file_info = fetch_cf_file_info(&client, &api_key, file_id).await?;
    let mut target_file_id = file_id;

    // Validación estricta y resolución de Server Pack
    if !file_info.is_server_pack.unwrap_or(false) {
        if let Some(sp_id) = file_info.server_pack_file_id {
            let sp_info = fetch_cf_file_info(&client, &api_key, sp_id).await.with_context(|| {
                format!(
                    "Error al obtener metadatos del Server Pack vinculado (ID {})",
                    sp_id
                )
            })?;
            target_file_id = sp_id;
            file_info = sp_info;
        } else {
            bail!("El modpack seleccionado no cuenta con un Server Pack oficial en CurseForge. No se puede crear una instancia de servidor con el archivo de cliente.");
        }
    }

    let download_url = match file_info.download_url {
        Some(ref u) if !u.is_empty() => u.clone(),
        _ => {
            let p1 = target_file_id / 1000;
            let p2 = target_file_id % 1000;
            format!(
                "https://mediafilez.forgecdn.net/files/{}/{}/{}",
                p1, p2, file_info.file_name
            )
        }
    };

    let mut mc_version: Option<String> = None;
    let mut loader = "fabric".to_string();

    if let Some(ref gv) = file_info.game_versions {
        for v in gv {
            let v_lower = v.to_lowercase();
            if v_lower == "fabric"
                || v_lower == "forge"
                || v_lower == "neoforge"
                || v_lower == "quilt"
            {
                loader = v_lower;
            } else if v.starts_with("1.") && v[2..].chars().all(|c| c.is_ascii_digit() || c == '.')
            {
                // Preferir versiones con mayor especificidad (ej. 1.20.1 sobre 1.20)
                match &mc_version {
                    None => mc_version = Some(v.clone()),
                    Some(current) => {
                        let cur_dots = current.split('.').count();
                        let new_dots = v.split('.').count();
                        if new_dots > cur_dots {
                            mc_version = Some(v.clone());
                        }
                    }
                }
            }
        }
    }
    let mc_version = mc_version.unwrap_or_else(|| "1.20.1".to_string());

    let project_slug = slug_opt.unwrap_or_else(|| {
        file_info
            .file_name
            .strip_suffix(".zip")
            .unwrap_or(&file_info.file_name)
            .to_string()
    });

    let suggested_name = sanitize_instance_name(&project_slug);

    Ok(ResolvedModpack {
        pack_type: PackType::ServerPackZip,
        project_id: target_file_id.to_string(),
        project_title: file_info.display_name,
        version_id: target_file_id.to_string(),
        version_name: file_info.file_name.clone(),
        suggested_instance_name: suggested_name,
        mc_version,
        loader,
        loader_version: None,
        mrpack_url: download_url,
        mrpack_filename: file_info.file_name,
        mrpack_sha512: None,
        mrpack_size: file_info.file_length.unwrap_or(0),
    })
}

pub async fn resolve_direct_zip_url(url: &str) -> Result<ResolvedModpack> {
    let clean = url.trim();
    let filename = clean
        .split('?')
        .next()
        .unwrap_or(clean)
        .split('/')
        .next_back()
        .unwrap_or("serverpack.zip")
        .to_string();

    let name_stem = filename.strip_suffix(".zip").unwrap_or(&filename);
    let suggested_name = sanitize_instance_name(name_stem);

    Ok(ResolvedModpack {
        pack_type: PackType::ServerPackZip,
        project_id: suggested_name.clone(),
        project_title: name_stem.to_string(),
        version_id: "custom".to_string(),
        version_name: filename.clone(),
        suggested_instance_name: suggested_name,
        mc_version: "1.20.1".to_string(),
        loader: "fabric".to_string(),
        loader_version: None,
        mrpack_url: clean.to_string(),
        mrpack_filename: filename,
        mrpack_sha512: None,
        mrpack_size: 0,
    })
}

pub async fn download_serverpack_file(
    client: &reqwest::Client,
    url: &str,
    target_path: &Path,
    reporter: &dyn ProgressReporter,
) -> Result<()> {
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut response = client
        .get(url)
        .header(
            reqwest::header::USER_AGENT,
            concat!("mcserman/", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .with_context(|| format!("Error al iniciar descarga desde {}", url))?;

    if !response.status().is_success() {
        bail!(
            "El servidor retornó error HTTP {}: {}",
            response.status(),
            url
        );
    }

    let total_size = response.content_length().unwrap_or(0);
    reporter.on_start("Descargando archivo ZIP del Server Pack...", total_size);

    let mut file = tokio::fs::File::create(target_path).await?;
    let mut downloaded: u64 = 0;

    while let Some(chunk) = response.chunk().await? {
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await?;
        downloaded += chunk.len() as u64;
        reporter.on_progress(downloaded, total_size);
    }
    tokio::io::AsyncWriteExt::flush(&mut file).await?;
    reporter.on_finish("Descarga del Server Pack completada");
    Ok(())
}

pub fn extract_serverpack_zip(
    zip_path: &Path,
    dest_dir: &Path,
    reporter: &dyn ProgressReporter,
) -> Result<()> {
    let file = std::fs::File::open(zip_path)
        .with_context(|| format!("No se pudo abrir el ZIP {}", zip_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("Formato de archivo ZIP inválido en {}", zip_path.display()))?;

    // Detección de carpeta contenedora única (ej. "ServerPack/mods/...")
    let mut common_prefix: Option<String> = None;
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        if let Some(first_slash) = name.find('/') {
            let prefix = name[..=first_slash].to_string();
            match &common_prefix {
                None => common_prefix = Some(prefix),
                Some(p) if *p == prefix => {}
                Some(_) => {
                    common_prefix = None;
                    break;
                }
            }
        } else {
            // Existe al menos un archivo en la raíz del zip
            common_prefix = None;
            break;
        }
    }

    let total = archive.len();
    reporter.on_start(
        &format!("Extrayendo {} archivos del Server Pack...", total),
        total as u64,
    );

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let mut entry_name = entry.name().to_string();

        if let Some(ref prefix) = common_prefix {
            if let Some(stripped) = entry_name.strip_prefix(prefix) {
                entry_name = stripped.to_string();
            }
        }
        if entry_name.is_empty() {
            continue;
        }

        let out_path = dest_dir.join(&entry_name);
        if !out_path.starts_with(dest_dir) {
            continue;
        }

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out_file = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out_file)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = entry.unix_mode().unwrap_or(0);
                let is_sh = out_path.extension().is_some_and(|ext| ext == "sh");
                let is_exec = (mode & 0o111) != 0 || is_sh;
                if mode != 0 {
                    let final_mode = if is_exec { mode | 0o755 } else { mode };
                    let _ = std::fs::set_permissions(
                        &out_path,
                        std::fs::Permissions::from_mode(final_mode),
                    );
                } else if is_exec {
                    let _ = std::fs::set_permissions(
                        &out_path,
                        std::fs::Permissions::from_mode(0o755),
                    );
                }
            }
        }
        reporter.on_progress(i as u64 + 1, total as u64);
    }

    reporter.on_finish("Archivos del Server Pack extraídos correctamente");
    Ok(())
}

#[derive(Debug, Default)]
pub struct DetectedServerInfo {
    pub server_jar: Option<String>,
    pub mc_version: Option<String>,
    pub loader: Option<String>,
    pub loader_version: Option<String>,
}

pub fn detect_server_info(dir: &Path) -> DetectedServerInfo {
    let mut server_jar = None;
    let mut mc_version = None;
    let mut loader = None;
    let mut loader_version = None;

    // 1. Detectar ejecutables comunes en la raíz
    let common_jar_names = [
        "fabric-server-launch.jar",
        "fabric-server.jar",
        "server.jar",
        "run.jar",
    ];
    for name in &common_jar_names {
        if dir.join(name).is_file() {
            server_jar = Some(name.to_string());
            break;
        }
    }
    if server_jar.is_none() {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Some(file_name) = p.file_name().and_then(|n| n.to_str()) {
                        if file_name.ends_with(".jar")
                            && file_name.to_lowercase().contains("server")
                        {
                            server_jar = Some(file_name.to_string());
                            break;
                        }
                    }
                }
            }
        }
    }

    // 2. Parsear variables.txt (generado comúnmente por ServerPackCreator)
    let var_file = dir.join("variables.txt");
    if var_file.is_file() {
        if let Ok(content) = std::fs::read_to_string(&var_file) {
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with('#') || line.is_empty() {
                    continue;
                }
                if let Some((k, v)) = line.split_once('=') {
                    let k = k.trim();
                    let v = v.trim().trim_matches('"').trim_matches('\'');
                    match k {
                        "MINECRAFT_VERSION" if !v.is_empty() => {
                            mc_version = Some(v.to_string());
                        }
                        "MODLOADER" if !v.is_empty() => {
                            loader = Some(v.to_lowercase());
                        }
                        "MODLOADER_VERSION" if !v.is_empty() => {
                            loader_version = Some(v.to_string());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // 3. Inspeccionar mods/ para corroborar loader y versión de MC
    let mods_dir = dir.join("mods");
    if mods_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&mods_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if loader.is_none() {
                    let name_lower = name.to_lowercase();
                    if name_lower.contains("neoforge") {
                        loader = Some("neoforge".to_string());
                    } else if name_lower.contains("forge") {
                        loader = Some("forge".to_string());
                    } else if name_lower.contains("fabric") {
                        loader = Some("fabric".to_string());
                    }
                }
                if mc_version.is_none() {
                    if let Some(pos) = name.find("-1.") {
                        let sub = &name[pos + 1..];
                        let ver_str: String = sub
                            .chars()
                            .take_while(|c| c.is_ascii_digit() || *c == '.')
                            .collect();
                        if ver_str.split('.').count() >= 2 {
                            mc_version = Some(ver_str);
                        }
                    }
                }
            }
        }
    }

    DetectedServerInfo {
        server_jar,
        mc_version,
        loader,
        loader_version,
    }
}

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

    // 1. Descarga del ZIP a carpeta temporal
    let temp_dir = std::env::temp_dir().join(format!(
        "mcserman_sp_{}_{}",
        instance_name,
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir)?;
    let temp_zip_path = temp_dir.join(&resolved.mrpack_filename);

    download_serverpack_file(&client, &resolved.mrpack_url, &temp_zip_path, reporter).await?;

    // 2. Descomprimir en el directorio de la instancia
    extract_serverpack_zip(&temp_zip_path, &instance_dir, reporter)?;

    // 3. Inspeccionar metadatos tras la extracción
    let detected = detect_server_info(&instance_dir);

    let mc_version = detected.mc_version.unwrap_or(resolved.mc_version);
    let loader_str = detected.loader.unwrap_or(resolved.loader);
    let loader_type = match loader_str.to_lowercase().as_str() {
        "fabric" => LoaderType::Fabric,
        "paper" => LoaderType::Paper,
        "neoforge" => LoaderType::NeoForge,
        "forge" => LoaderType::Forge,
        "vanilla" => LoaderType::Vanilla,
        _ => LoaderType::Fabric,
    };
    let loader_version = detected.loader_version.or(resolved.loader_version);

    // 4. Determinar ejecutable o descargar loader jar si falta
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
    };

    // 5. Asegurar eula.txt con eula=true
    let eula_path = instance_dir.join("eula.txt");
    std::fs::write(&eula_path, "eula=true\n")
        .context("No se pudo escribir eula.txt con eula=true")?;

    // 6. Actualizar / crear server.properties con el puerto
    let props_path = instance_dir.join("server.properties");
    let mut props = ServerProperties::load_or_default(&props_path);
    props.set("server-port", &port.to_string());
    props.save(&props_path)?;

    // 7. Generar instance.toml
    let mut config = InstanceConfig::new(instance_name.to_string(), loader_type, mc_version);
    config.loader_version = loader_version;
    config.server_jar = server_jar;
    config.ram_min = "2G".to_string();
    config.ram_max = ram.to_string();
    config.port = port;
    config.eula = true;
    config.source_pack = Some(format!("serverpack:{}", resolved.project_title));
    config.save(&paths.instance_config(instance_name))?;

    let _ = std::fs::remove_dir_all(&temp_dir);

    reporter.on_finish(&format!(
        "Instancia '{}' creada exitosamente desde Server Pack",
        instance_name
    ));
    Ok(instance_name.to_string())
}

pub fn is_jar_url(input: &str) -> bool {
    let lower = input.to_lowercase();
    if let Some(clean) = lower.split('?').next() {
        clean.ends_with(".jar")
    } else {
        lower.ends_with(".jar")
    }
}

pub async fn add_addon_from_url_or_slug(
    query: &str,
    mc_version: &str,
    loader: &str,
    target_dir: &Path,
    reporter: &dyn ProgressReporter,
) -> Result<String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        bail!("La URL o identificador no puede estar vacío");
    }

    std::fs::create_dir_all(target_dir)?;

    // 1. Enlace directo HTTP(S) a archivo .jar
    if (trimmed.starts_with("http://") || trimmed.starts_with("https://")) && is_jar_url(trimmed) {
        return download_direct_jar(trimmed, target_dir, reporter).await;
    }

    // 2. URL de CurseForge
    if is_curseforge_url(trimmed) {
        return download_curseforge_mod(trimmed, mc_version, loader, target_dir, reporter).await;
    }

    // 3. Modrinth (URL o Slug)
    download_modrinth_mod(trimmed, mc_version, loader, target_dir, reporter).await
}

async fn download_direct_jar(
    url: &str,
    target_dir: &Path,
    reporter: &dyn ProgressReporter,
) -> Result<String> {
    let filename = url
        .split('?')
        .next()
        .unwrap_or(url)
        .split('/')
        .next_back()
        .unwrap_or("addon.jar")
        .to_string();

    let target_path = target_dir.join(&filename);
    let client = reqwest::Client::builder()
        .user_agent(concat!("mcserman/", env!("CARGO_PKG_VERSION")))
        .build()?;

    reporter.on_info(&format!("Descargando {}...", filename));
    let mut response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Error descargando archivo .jar desde {}", url))?;

    if !response.status().is_success() {
        bail!("Error HTTP {} al descargar {}", response.status(), url);
    }

    let total = response.content_length().unwrap_or(0);
    reporter.on_start(&filename, total);

    let mut file = tokio::fs::File::create(&target_path).await?;
    let mut downloaded = 0u64;

    while let Some(chunk) = response.chunk().await? {
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await?;
        downloaded += chunk.len() as u64;
        reporter.on_progress(downloaded, total);
    }
    tokio::io::AsyncWriteExt::flush(&mut file).await?;
    reporter.on_finish(&format!("{} instalado correctamente", filename));
    Ok(filename)
}

async fn download_curseforge_mod(
    url: &str,
    mc_version: &str,
    loader: &str,
    target_dir: &Path,
    reporter: &dyn ProgressReporter,
) -> Result<String> {
    let (slug_opt, file_id_opt) = parse_curseforge_url(url);

    // Si tiene file_id explícito (ej. /files/4593548)
    if let Some(file_id) = file_id_opt {
        let client = reqwest::Client::builder()
            .user_agent(concat!("mcserman/", env!("CARGO_PKG_VERSION")))
            .build()?;

        let api_key = std::env::var("CURSEFORGE_API_KEY")
            .or_else(|_| std::env::var("CF_API_KEY"))
            .unwrap_or_else(|_| DEFAULT_CURSEFORGE_KEY.to_string());

        let payload = serde_json::json!({
            "fileIds": [file_id]
        });

        let resp = client
            .post(CURSEFORGE_API_URL)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .header("x-api-key", &api_key)
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            bail!(
                "CurseForge API retornó error HTTP {}: {}",
                resp.status(),
                file_id
            );
        }

        let parsed: CfFilesResponse = resp.json().await?;
        let file_info = parsed
            .data
            .into_iter()
            .find(|f| f.id == file_id)
            .with_context(|| {
                format!(
                    "No se encontró información para el archivo CurseForge {}",
                    file_id
                )
            })?;

        let download_url = match file_info.download_url {
            Some(ref u) if !u.is_empty() => u.clone(),
            _ => {
                let p1 = file_id / 1000;
                let p2 = file_id % 1000;
                format!(
                    "https://mediafilez.forgecdn.net/files/{}/{}/{}",
                    p1, p2, file_info.file_name
                )
            }
        };

        let target_path = target_dir.join(&file_info.file_name);
        download_serverpack_file(&client, &download_url, &target_path, reporter).await?;
        return Ok(file_info.file_name);
    }

    // Si no tiene file_id, buscar por slug en CurseForge
    let slug =
        slug_opt.with_context(|| "No se pudo extraer el slug o file ID de la URL de CurseForge")?;
    let client = reqwest::Client::builder()
        .user_agent(concat!("mcserman/", env!("CARGO_PKG_VERSION")))
        .build()?;

    let api_key = std::env::var("CURSEFORGE_API_KEY")
        .or_else(|_| std::env::var("CF_API_KEY"))
        .unwrap_or_else(|_| DEFAULT_CURSEFORGE_KEY.to_string());

    let search_url = format!(
        "https://api.curseforge.com/v1/mods/search?gameId=432&searchFilter={}",
        urlencoding::encode(&slug)
    );
    let resp = client
        .get(&search_url)
        .header("Accept", "application/json")
        .header("x-api-key", &api_key)
        .send()
        .await?;

    if !resp.status().is_success() {
        bail!(
            "Error buscando mod '{}' en CurseForge: HTTP {}",
            slug,
            resp.status()
        );
    }

    #[derive(Deserialize)]
    struct ModSearchResp {
        data: Vec<ModSearchItem>,
    }
    #[derive(Deserialize)]
    struct ModSearchItem {
        id: u64,
        slug: String,
    }

    let search_res: ModSearchResp = resp.json().await?;
    let found_mod = search_res
        .data
        .iter()
        .find(|m| m.slug.eq_ignore_ascii_case(&slug))
        .or_else(|| search_res.data.first())
        .with_context(|| {
            format!(
                "No se encontró ningún mod coincidente con '{}' en CurseForge",
                slug
            )
        })?;

    // Consultar archivos del mod
    let files_url = format!("https://api.curseforge.com/v1/mods/{}/files", found_mod.id);
    let files_resp = client
        .get(&files_url)
        .header("Accept", "application/json")
        .header("x-api-key", &api_key)
        .send()
        .await?;

    let parsed_files: CfFilesResponse = files_resp.json().await?;
    let compatible_file = parsed_files
        .data
        .into_iter()
        .find(|f| {
            if let Some(ref gv) = f.game_versions {
                let has_mc = gv
                    .iter()
                    .any(|v| v == mc_version || v.starts_with(mc_version));
                let has_loader = gv.iter().any(|v| v.eq_ignore_ascii_case(loader));
                has_mc && has_loader
            } else {
                false
            }
        })
        .with_context(|| {
            format!(
                "No se encontró ningún archivo compatible con MC {} y {} en CurseForge para '{}'",
                mc_version, loader, slug
            )
        })?;

    let download_url = match compatible_file.download_url {
        Some(ref u) if !u.is_empty() => u.clone(),
        _ => {
            let p1 = compatible_file.id / 1000;
            let p2 = compatible_file.id % 1000;
            format!(
                "https://mediafilez.forgecdn.net/files/{}/{}/{}",
                p1, p2, compatible_file.file_name
            )
        }
    };

    let target_path = target_dir.join(&compatible_file.file_name);
    download_serverpack_file(&client, &download_url, &target_path, reporter).await?;
    Ok(compatible_file.file_name)
}

async fn download_modrinth_mod(
    input: &str,
    mc_version: &str,
    loader: &str,
    target_dir: &Path,
    reporter: &dyn ProgressReporter,
) -> Result<String> {
    let slug = if let Some(rest) = input
        .strip_prefix("https://modrinth.com/")
        .or_else(|| input.strip_prefix("http://modrinth.com/"))
    {
        let clean = rest.trim_matches('/');
        let parts: Vec<&str> = clean.split('/').collect();
        if parts.len() >= 2 && (parts[0] == "mod" || parts[0] == "plugin" || parts[0] == "project")
        {
            parts[1].to_string()
        } else if !parts.is_empty() {
            parts[0].to_string()
        } else {
            input.to_string()
        }
    } else {
        input.trim().to_string()
    };

    let client = crate::modrinth::api::ModrinthClient::new();
    let version = client
        .get_compatible_version(&slug, loader, mc_version)
        .await
        .with_context(|| {
            format!(
                "No se encontró versión en Modrinth de '{}' para {} en MC {}",
                slug, loader, mc_version
            )
        })?;

    let file_info = version
        .files
        .iter()
        .find(|f| f.primary)
        .or_else(|| version.files.first())
        .with_context(|| {
            format!(
                "La versión {} de '{}' no contiene archivos descargables",
                version.name, slug
            )
        })?;

    let dest = client
        .download_file(file_info, target_dir, reporter)
        .await?;
    let filename = dest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or(file_info.filename.clone());

    Ok(filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_curseforge_url() {
        let url1 =
            "https://www.curseforge.com/minecraft/modpacks/better-mc-fabric-bmc2/files/7522918";
        let (slug1, file_id1) = parse_curseforge_url(url1);
        assert_eq!(slug1, Some("better-mc-fabric-bmc2".to_string()));
        assert_eq!(file_id1, Some(7522918));

        let url2 = "https://curseforge.com/minecraft/modpacks/all-the-mods-9/files/5123456";
        let (slug2, file_id2) = parse_curseforge_url(url2);
        assert_eq!(slug2, Some("all-the-mods-9".to_string()));
        assert_eq!(file_id2, Some(5123456));

        let url_zip = "https://mediafilez.forgecdn.net/files/7522/918/BMC2_Server_Pack_v40.zip";
        assert!(is_zip_url(url_zip));
        assert!(!is_curseforge_url(url_zip));
    }

    #[test]
    fn test_detect_server_info_from_variables() {
        let temp_dir =
            std::env::temp_dir().join(format!("mcserman_test_detect_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let var_content = "MINECRAFT_VERSION=1.20.1\nMODLOADER=Fabric\nMODLOADER_VERSION=0.18.4\n";
        std::fs::write(temp_dir.join("variables.txt"), var_content).unwrap();

        let info = detect_server_info(&temp_dir);
        assert_eq!(info.mc_version, Some("1.20.1".to_string()));
        assert_eq!(info.loader, Some("fabric".to_string()));
        assert_eq!(info.loader_version, Some("0.18.4".to_string()));
        assert_eq!(info.server_jar, None);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_extract_serverpack_zip_single_root_unwrapping() {
        use std::io::Write;
        let temp_dir =
            std::env::temp_dir().join(format!("mcserman_test_sp_zip_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let zip_path = temp_dir.join("pack.zip");

        {
            let file = std::fs::File::create(&zip_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();

            zip.start_file("MyServerPack/server.properties", options)
                .unwrap();
            zip.write_all(b"motd=Hello Serverpack\n").unwrap();

            zip.start_file("MyServerPack/mods/test-mod-1.20.1.jar", options)
                .unwrap();
            zip.write_all(b"dummy jar content").unwrap();

            zip.start_file("MyServerPack/run.sh", options)
                .unwrap();
            zip.write_all(b"#!/bin/sh\necho running\n").unwrap();

            zip.finish().unwrap();
        }

        let out_dir = temp_dir.join("out");
        let (tx, _rx) = tokio::sync::mpsc::channel(10);
        let reporter = crate::core::progress::ChannelProgressReporter::new(tx);

        extract_serverpack_zip(&zip_path, &out_dir, &reporter).unwrap();

        // Debe haber desenvuelto "MyServerPack" directamente en out_dir
        assert!(out_dir.join("server.properties").exists());
        assert!(out_dir.join("mods").join("test-mod-1.20.1.jar").exists());
        let run_sh = out_dir.join("run.sh");
        assert!(run_sh.exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::metadata(&run_sh).unwrap().permissions();
            assert_eq!(perms.mode() & 0o111, 0o111, "run.sh debe tener permisos de ejecución");
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_detect_server_info_neoforge() {
        let temp_dir =
            std::env::temp_dir().join(format!("mcserman_test_detect_neo_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let var_content = "MINECRAFT_VERSION=1.21.1\nMODLOADER=NeoForge\nMODLOADER_VERSION=21.1.250\nUSE_SSJ=true\n";
        std::fs::write(temp_dir.join("variables.txt"), var_content).unwrap();

        let info = detect_server_info(&temp_dir);
        assert_eq!(info.mc_version, Some("1.21.1".to_string()));
        assert_eq!(info.loader, Some("neoforge".to_string()));
        assert_eq!(info.loader_version, Some("21.1.250".to_string()));
        assert_eq!(info.server_jar, None);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    #[ignore = "requires network access (skipped in NixOS build sandbox)"]
    async fn test_curseforge_client_auto_resolves_to_serverpack() {
        let client_url =
            "https://www.curseforge.com/minecraft/modpacks/better-mc-neoforge-bmc5/files/8835241";
        let resolved = resolve_serverpack(client_url)
            .await
            .expect("Debe resolver automáticamente el Server Pack vinculado");

        assert_eq!(resolved.version_id, "8835262");
        assert_eq!(resolved.mc_version, "1.21.1");
        assert_eq!(resolved.loader, "neoforge");
        assert_eq!(resolved.mrpack_filename, "BMC5Neo_ServerPack_v53.zip");
        assert!(resolved.mrpack_url.contains("BMC5Neo_ServerPack_v53.zip"));
    }

    #[tokio::test]
    #[ignore = "requires network access (skipped in NixOS build sandbox)"]
    async fn test_curseforge_bmc2_serverpack() {
        let url =
            "https://www.curseforge.com/minecraft/modpacks/better-mc-fabric-bmc2/files/7522918";
        let resolved = resolve_serverpack(url)
            .await
            .expect("Error al resolver URL de CurseForge");

        assert_eq!(resolved.version_id, "7522918");
        assert_eq!(resolved.mc_version, "1.20.1");
        assert_eq!(resolved.loader, "fabric");
        assert!(resolved.mrpack_url.ends_with(".zip"));

        let temp_dir =
            std::env::temp_dir().join(format!("mcserman_bmc2_sp_test_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let (tx, _rx) = tokio::sync::mpsc::channel(10);
        let reporter = crate::core::progress::ChannelProgressReporter::new(tx);

        let paths = SystemPaths::resolve();
        let inst_name = format!("bmc2_test_{}", std::process::id());
        let inst_dir = paths.instance_dir(&inst_name);
        std::fs::create_dir_all(&inst_dir).unwrap();

        let cached_zip = std::path::Path::new("/tmp/bmc2_server_pack.zip");
        let zip_to_use = if cached_zip.is_file() {
            cached_zip.to_path_buf()
        } else {
            let client = reqwest::Client::new();
            let dl_path = temp_dir.join("downloaded.zip");
            download_serverpack_file(&client, &resolved.mrpack_url, &dl_path, &reporter)
                .await
                .unwrap();
            dl_path
        };

        // Extraer Server Pack
        extract_serverpack_zip(&zip_to_use, &inst_dir, &reporter).unwrap();

        // Comprobar que en mods/ queden instalados Balm y FallingTree
        let mods_dir = inst_dir.join("mods");
        assert!(mods_dir.is_dir());

        let mut has_balm = false;
        let mut has_fallingtree = false;
        if let Ok(entries) = std::fs::read_dir(&mods_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.to_lowercase().contains("balm") {
                    has_balm = true;
                }
                if name.to_lowercase().contains("fallingtree") {
                    has_fallingtree = true;
                }
            }
        }
        assert!(has_balm, "El mod Balm debe estar instalado en mods/");
        assert!(
            has_fallingtree,
            "El mod FallingTree debe estar instalado en mods/"
        );

        // Detección de server info
        let detected = detect_server_info(&inst_dir);
        assert_eq!(detected.mc_version.as_deref(), Some("1.20.1"));
        assert_eq!(detected.loader.as_deref(), Some("fabric"));

        // Descargar Fabric Server jar
        let jar_name = format!("fabric-server-{}.jar", "1.20.1");
        let target_jar = inst_dir.join(&jar_name);
        crate::loaders::fabric::download_fabric_custom(
            "1.20.1",
            detected.loader_version.as_deref(),
            &target_jar,
            &reporter,
        )
        .await
        .expect("Error al descargar Fabric Server");
        assert!(target_jar.is_file());

        // Asegurar eula=true
        let eula_path = inst_dir.join("eula.txt");
        std::fs::write(&eula_path, "eula=true\n").unwrap();
        assert!(std::fs::read_to_string(&eula_path)
            .unwrap()
            .contains("eula=true"));

        // Limpieza
        let _ = std::fs::remove_dir_all(&inst_dir);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
