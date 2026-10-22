use crate::core::config::{InstanceConfig, LoaderType};
use crate::core::paths::SystemPaths;
use crate::core::progress::ProgressReporter;
use crate::core::properties::ServerProperties;
use crate::modrinth::api::ProjectVersion;
use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModpackIndex {
    #[serde(rename = "formatVersion")]
    pub format_version: u32,
    pub game: String,
    #[serde(rename = "versionId")]
    pub version_id: String,
    pub name: String,
    pub summary: Option<String>,
    pub files: Vec<ModpackFile>,
    pub dependencies: HashMap<String, String>,
}

impl ModpackIndex {
    pub fn minecraft_version(&self) -> Option<&str> {
        self.dependencies.get("minecraft").map(|s| s.as_str())
    }

    pub fn loader_info(&self) -> (Option<&'static str>, Option<&str>) {
        if let Some(v) = self.dependencies.get("fabric-loader") {
            (Some("fabric"), Some(v.as_str()))
        } else if let Some(v) = self.dependencies.get("quilt-loader") {
            (Some("quilt"), Some(v.as_str()))
        } else if let Some(v) = self.dependencies.get("forge") {
            (Some("forge"), Some(v.as_str()))
        } else if let Some(v) = self.dependencies.get("neoforge") {
            (Some("neoforge"), Some(v.as_str()))
        } else {
            (None, None)
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModpackFile {
    pub path: String,
    pub hashes: ModpackFileHashes,
    pub env: Option<ModpackEnv>,
    pub downloads: Vec<String>,
    #[serde(rename = "fileSize")]
    pub file_size: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModpackFileHashes {
    pub sha512: Option<String>,
    pub sha1: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModpackEnv {
    pub client: Option<String>,
    pub server: Option<String>,
}

impl ModpackEnv {
    pub fn is_server_supported(&self) -> bool {
        match self.server.as_deref() {
            Some("unsupported") => false,
            Some("required") | Some("optional") | None => true,
            _ => true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ProjectInfo {
    pub id: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub project_type: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PackType {
    ModrinthMrpack,
    ServerPackZip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedModpack {
    pub pack_type: PackType,
    pub project_id: String,
    pub project_title: String,
    pub version_id: String,
    pub version_name: String,
    pub suggested_instance_name: String,
    pub mc_version: String,
    pub loader: String,
    pub loader_version: Option<String>,
    pub mrpack_url: String,
    pub mrpack_filename: String,
    pub mrpack_sha512: Option<String>,
    pub mrpack_size: u64,
}

pub fn parse_modpack_input(input: &str) -> (Option<String>, Option<String>) {
    let trimmed = input.trim();
    if let Some(rest) = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
    {
        let clean = rest.trim_end_matches('/');
        let parts: Vec<&str> = clean.split('/').collect();
        // Buscar '/version/{id}'
        if let Some(pos) = parts.iter().position(|&p| p == "version") {
            if pos + 1 < parts.len() {
                let version_id = parts[pos + 1].to_string();
                let project = if pos > 0
                    && parts[pos - 1] != "modpack"
                    && parts[pos - 1] != "project"
                    && !parts[pos - 1].contains('.')
                {
                    Some(parts[pos - 1].to_string())
                } else {
                    None
                };
                return (project, Some(version_id));
            }
        }
        // Si no tiene '/version/', buscar '/modpack/{slug}' o '/project/{slug}'
        if let Some(pos) = parts.iter().position(|&p| p == "modpack" || p == "project") {
            if pos + 1 < parts.len() {
                return (Some(parts[pos + 1].to_string()), None);
            }
        }
    }
    (Some(trimmed.to_string()), None)
}

pub fn sanitize_instance_name(input: &str) -> String {
    let mut out = String::new();
    let mut last_was_sep = false;
    for c in input.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            if c == '_' || c == '-' {
                if !last_was_sep && !out.is_empty() {
                    out.push(c);
                    last_was_sep = true;
                }
            } else {
                out.push(c.to_ascii_lowercase());
                last_was_sep = false;
            }
        } else if (c.is_whitespace()
            || c == '['
            || c == ']'
            || c == '('
            || c == ')'
            || c == '!'
            || c == ':'
            || c == '/'
            || c == '\\')
            && !last_was_sep
            && !out.is_empty()
        {
            out.push('-');
            last_was_sep = true;
        }
    }
    let trimmed = out.trim_matches('-').trim_matches('_');
    if trimmed.is_empty() {
        "modpack".to_string()
    } else {
        trimmed.to_string()
    }
}

pub async fn resolve_modpack(input: &str) -> Result<ResolvedModpack> {
    if crate::loaders::serverpack::is_curseforge_or_zip(input) {
        return crate::loaders::serverpack::resolve_serverpack(input).await;
    }

    let client = reqwest::Client::builder()
        .user_agent(concat!(
            "mcserman/",
            env!("CARGO_PKG_VERSION"),
            " (https://github.com/AdrianChaSan/mcserman)"
        ))
        .build()?;

    let (parsed_project, parsed_version) = parse_modpack_input(input);

    let version: ProjectVersion = if let Some(ref ver_id) = parsed_version {
        let url = format!("https://api.modrinth.com/v2/version/{}", ver_id);
        client
            .get(&url)
            .send()
            .await?
            .json()
            .await
            .with_context(|| format!("Error al obtener versión '{}' de Modrinth", ver_id))?
    } else if let Some(ref proj) = parsed_project {
        // 1. Probar si la entrada es un ID de versión directamente
        let ver_url = format!("https://api.modrinth.com/v2/version/{}", proj);
        if let Ok(resp) = client.get(&ver_url).send().await {
            if resp.status().is_success() {
                if let Ok(ver) = resp.json::<ProjectVersion>().await {
                    return build_resolved_modpack(&client, ver, None).await;
                }
            }
        }
        // 2. Si no, consultar versiones del proyecto
        let url = format!("https://api.modrinth.com/v2/project/{}/version", proj);
        let versions: Vec<ProjectVersion> = client
            .get(&url)
            .send()
            .await?
            .json()
            .await
            .with_context(|| format!("Error al consultar versiones de '{}' en Modrinth", proj))?;

        versions
            .into_iter()
            .next()
            .with_context(|| format!("El modpack '{}' no tiene versiones publicadas", proj))?
    } else {
        bail!("No se pudo interpretar la URL o identificador del modpack");
    };

    build_resolved_modpack(&client, version, parsed_project.as_deref()).await
}

async fn build_resolved_modpack(
    client: &reqwest::Client,
    version: ProjectVersion,
    preferred_slug: Option<&str>,
) -> Result<ResolvedModpack> {
    let proj_url = format!("https://api.modrinth.com/v2/project/{}", version.project_id);
    let project_info: Option<ProjectInfo> = match client.get(&proj_url).send().await {
        Ok(resp) if resp.status().is_success() => resp.json().await.ok(),
        _ => None,
    };

    let title = project_info
        .as_ref()
        .map(|p| p.title.clone())
        .unwrap_or_else(|| version.name.clone());

    let slug = project_info
        .as_ref()
        .map(|p| p.slug.clone())
        .or_else(|| preferred_slug.map(|s| s.to_string()))
        .unwrap_or_else(|| sanitize_instance_name(&title));

    let mrpack_file = version
        .files
        .iter()
        .find(|f| f.filename.ends_with(".mrpack"))
        .or_else(|| version.files.iter().find(|f| f.primary))
        .or_else(|| version.files.first())
        .with_context(|| "No se encontró ningún archivo .mrpack en esta versión del modpack")?;

    let mc_ver = version
        .game_versions
        .first()
        .cloned()
        .unwrap_or_else(|| "1.20.1".to_string());

    let loader = version
        .loaders
        .first()
        .cloned()
        .unwrap_or_else(|| "fabric".to_string());

    Ok(ResolvedModpack {
        pack_type: PackType::ModrinthMrpack,
        project_id: version.project_id,
        project_title: title.clone(),
        version_id: version.id,
        version_name: version.name,
        suggested_instance_name: sanitize_instance_name(&slug),
        mc_version: mc_ver,
        loader,
        loader_version: None,
        mrpack_url: mrpack_file.url.clone(),
        mrpack_filename: mrpack_file.filename.clone(),
        mrpack_sha512: mrpack_file.hashes.sha512.clone(),
        mrpack_size: mrpack_file.size,
    })
}

pub async fn download_mrpack_file(
    client: &reqwest::Client,
    url: &str,
    expected_sha512: Option<&str>,
    target_path: &Path,
    reporter: &dyn ProgressReporter,
) -> Result<()> {
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Error al descargar modpack desde {}", url))?;

    if !response.status().is_success() {
        bail!(
            "HTTP {} al descargar modpack desde {}",
            response.status(),
            url
        );
    }

    let total_size = response.content_length().unwrap_or(0);
    reporter.on_start("Descargando paquete .mrpack...", total_size);

    let mut file = std::fs::File::create(target_path)?;
    let mut stream = response.bytes_stream();
    let mut hasher = Sha512::new();
    let mut downloaded = 0u64;

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res?;
        std::io::copy(&mut chunk.as_ref(), &mut file)?;
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;
        reporter.on_progress(downloaded, total_size);
    }

    if let Some(expected) = expected_sha512 {
        let calculated = hex::encode(hasher.finalize());
        if !calculated.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(target_path);
            bail!(
                "Corrupción de archivo: hash SHA-512 del .mrpack no coincide.\nEsperado: {}\nObtenido: {}",
                expected, calculated
            );
        }
    }

    reporter.on_info("🔒 Archivo .mrpack verificado con éxito");
    Ok(())
}

pub fn extract_mrpack_index(mrpack_path: &Path) -> Result<ModpackIndex> {
    let file = std::fs::File::open(mrpack_path).with_context(|| {
        format!(
            "No se pudo abrir el archivo .mrpack en {}",
            mrpack_path.display()
        )
    })?;
    let mut archive =
        zip::ZipArchive::new(file).with_context(|| "El archivo .mrpack no es un ZIP válido")?;
    let mut index_entry = archive
        .by_name("modrinth.index.json")
        .with_context(|| "El archivo .mrpack no contiene 'modrinth.index.json'")?;
    let mut content = String::new();
    index_entry.read_to_string(&mut content)?;
    let index: ModpackIndex = serde_json::from_str(&content)
        .with_context(|| "Error al deserializar 'modrinth.index.json'")?;
    Ok(index)
}

pub fn extract_mrpack_overrides(mrpack_path: &Path, target_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(mrpack_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    // 1. Extraer overrides/
    for i in 0..archive.len() {
        let mut zip_file = archive.by_index(i)?;
        let name = zip_file.name().to_string();

        if let Some(rel) = name.strip_prefix("overrides/") {
            if rel.is_empty() || rel.ends_with('/') {
                continue;
            }
            let outpath = target_dir.join(rel);
            if !outpath.starts_with(target_dir) {
                continue;
            }
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut outfile = std::fs::File::create(&outpath)?;
            std::io::copy(&mut zip_file, &mut outfile)?;
        }
    }

    // 2. Extraer server-overrides/ (sobrescribe sobre overrides/ si hay conflicto)
    for i in 0..archive.len() {
        let mut zip_file = archive.by_index(i)?;
        let name = zip_file.name().to_string();

        if let Some(rel) = name.strip_prefix("server-overrides/") {
            if rel.is_empty() || rel.ends_with('/') {
                continue;
            }
            let outpath = target_dir.join(rel);
            if !outpath.starts_with(target_dir) {
                continue;
            }
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut outfile = std::fs::File::create(&outpath)?;
            std::io::copy(&mut zip_file, &mut outfile)?;
        }
    }

    Ok(())
}

pub async fn download_serverpack_files(
    client: &reqwest::Client,
    files: &[ModpackFile],
    target_dir: &Path,
    reporter: &dyn ProgressReporter,
) -> Result<()> {
    let server_files: Vec<ModpackFile> = files
        .iter()
        .filter(|f| f.env.as_ref().is_none_or(|e| e.is_server_supported()))
        .cloned()
        .collect();

    let total = server_files.len();
    reporter.on_start(
        &format!("Instalando {} mods de servidor...", total),
        total as u64,
    );

    let completed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let stream = futures_util::stream::iter(server_files.into_iter().map(|file| {
        let client = client.clone();
        let target_dir = target_dir.to_path_buf();
        let completed = std::sync::Arc::clone(&completed);

        async move {
            let outpath = target_dir.join(&file.path);
            if !outpath.starts_with(&target_dir) {
                bail!("Ruta de archivo no permitida: {}", file.path);
            }
            if let Some(parent) = outpath.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }

            // Si ya existe y el hash coincide, omitir descarga
            if outpath.exists() {
                if let Some(ref expected_hash) = file.hashes.sha512 {
                    if let Ok(bytes) = tokio::fs::read(&outpath).await {
                        let mut hasher = Sha512::new();
                        hasher.update(&bytes);
                        if hex::encode(hasher.finalize()).eq_ignore_ascii_case(expected_hash) {
                            let count =
                                completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                            reporter.on_progress(count as u64, total as u64);
                            return Ok(());
                        }
                    }
                }
            }

            let mut last_err = None;
            let mut downloaded_bytes = None;

            for url in &file.downloads {
                match client.get(url).send().await {
                    Ok(resp) if resp.status().is_success() => match resp.bytes().await {
                        Ok(bytes) => {
                            downloaded_bytes = Some(bytes);
                            break;
                        }
                        Err(e) => last_err = Some(e.into()),
                    },
                    Ok(resp) => {
                        last_err = Some(anyhow::anyhow!("HTTP {}", resp.status()));
                    }
                    Err(e) => {
                        last_err = Some(e.into());
                    }
                }
            }

            let bytes = match downloaded_bytes {
                Some(b) => b,
                None => {
                    bail!(
                        "Error al descargar {}: {:?}",
                        file.path,
                        last_err.unwrap_or_else(|| anyhow::anyhow!("No se pudo conectar"))
                    );
                }
            };

            // Validación de integridad SHA-512
            if let Some(ref expected_hash) = file.hashes.sha512 {
                let mut hasher = Sha512::new();
                hasher.update(&bytes);
                let calculated = hex::encode(hasher.finalize());
                if !calculated.eq_ignore_ascii_case(expected_hash) {
                    bail!(
                        "¡Hash SHA-512 corrupto para {}!\nEsperado: {}\nObtenido: {}",
                        file.path,
                        expected_hash,
                        calculated
                    );
                }
            }

            tokio::fs::write(&outpath, bytes).await?;

            let count = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            reporter.on_progress(count as u64, total as u64);
            reporter.on_info(&format!("Instalado [{}/{}]: {}", count, total, file.path));

            Ok::<(), anyhow::Error>(())
        }
    }))
    .buffer_unordered(8);

    let results: Vec<Result<(), anyhow::Error>> = stream.collect().await;
    for res in results {
        res?;
    }

    reporter.on_finish(&format!(
        "✓ {} mods de servidor listos y verificados",
        total
    ));
    Ok(())
}

pub async fn install_modpack(
    name: &str,
    modpack_query: &str,
    resolved_opt: Option<ResolvedModpack>,
    ram: &str,
    port: u16,
    paths: &SystemPaths,
    reporter: &dyn ProgressReporter,
) -> Result<String> {
    if resolved_opt
        .as_ref()
        .is_some_and(|r| r.pack_type == PackType::ServerPackZip)
        || crate::loaders::serverpack::is_curseforge_or_zip(modpack_query)
    {
        return crate::loaders::serverpack::install_serverpack(
            name,
            modpack_query,
            resolved_opt,
            ram,
            port,
            paths,
            reporter,
        )
        .await;
    }

    let client = reqwest::Client::builder()
        .user_agent(concat!(
            "mcserman/",
            env!("CARGO_PKG_VERSION"),
            " (https://github.com/AdrianChaSan/mcserman)"
        ))
        .build()?;

    let resolved = match resolved_opt {
        Some(r) => r,
        None => resolve_modpack(modpack_query).await?,
    };

    let inst_dir = paths.instance_dir(name);
    std::fs::create_dir_all(&inst_dir)?;

    let temp_mrpack = inst_dir.join(".serverpack.mrpack.tmp");

    // 1. Descargar .mrpack
    download_mrpack_file(
        &client,
        &resolved.mrpack_url,
        resolved.mrpack_sha512.as_deref(),
        &temp_mrpack,
        reporter,
    )
    .await?;

    // 2. Extraer modrinth.index.json
    reporter.on_info("Leyendo manifiesto modrinth.index.json...");
    let index = extract_mrpack_index(&temp_mrpack)?;

    let mc_version = index.minecraft_version().unwrap_or(&resolved.mc_version);

    let (loader_name, loader_ver) = index.loader_info();
    let effective_loader = loader_name.unwrap_or(&resolved.loader);
    let effective_loader_ver = loader_ver.or(resolved.loader_version.as_deref());

    if effective_loader != "fabric" && effective_loader != "quilt" {
        let _ = std::fs::remove_file(&temp_mrpack);
        bail!(
            "El modpack requiere el loader '{}', que no está soportado actualmente. Solo modpacks Fabric son soportados.",
            effective_loader
        );
    }

    // 3. Extraer overrides y server-overrides
    reporter.on_info("Extrayendo configuraciones y server-overrides...");
    extract_mrpack_overrides(&temp_mrpack, &inst_dir)?;

    // 4. Descargar mods de servidor
    download_serverpack_files(&client, &index.files, &inst_dir, reporter).await?;

    // 5. Descargar Fabric Server JAR
    let target_jar = inst_dir.join(format!("fabric-server-{}.jar", mc_version));
    reporter.on_info(&format!(
        "Instalando Fabric Server para MC {}...",
        mc_version
    ));
    let installed_loader_ver = crate::loaders::fabric::download_fabric_custom(
        mc_version,
        effective_loader_ver,
        &target_jar,
        reporter,
    )
    .await?;

    // 6. Resolver Java para la versión de Minecraft
    let java_ver = crate::core::java::required_java_version(mc_version);
    crate::core::java::resolve_java(java_ver, paths, reporter).await?;

    // 7. Generar eula.txt, server.properties e instance.toml
    std::fs::write(inst_dir.join("eula.txt"), "eula=true\n")?;

    let props_path = inst_dir.join("server.properties");
    let mut props = ServerProperties::load_or_default(&props_path);
    props.set("server-port", &port.to_string());
    props.set(
        "motd",
        &format!("Servidor {} ({})", name, resolved.project_title),
    );
    props.save(&props_path)?;

    let mut config =
        InstanceConfig::new(name.to_string(), LoaderType::Fabric, mc_version.to_string());
    config.ram_max = ram.to_string();
    config.port = port;
    config.loader_version = Some(installed_loader_ver);
    config.source_pack = Some(format!("modpack:{}", resolved.project_title));
    config.save(&paths.instance_config(name))?;

    // 8. Limpiar archivo temporal
    let _ = std::fs::remove_file(&temp_mrpack);

    reporter.on_finish(&format!(
        "✓ Modpack '{}' instalado y listo para iniciar",
        name
    ));
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_modpack_input() {
        let (proj, ver) = parse_modpack_input(
            "https://modrinth.com/modpack/better-mc-fabric-bmc2/version/M5BnAIQy",
        );
        assert_eq!(proj, Some("better-mc-fabric-bmc2".to_string()));
        assert_eq!(ver, Some("M5BnAIQy".to_string()));

        let (proj2, ver2) =
            parse_modpack_input("https://modrinth.com/modpack/better-mc-fabric-bmc2");
        assert_eq!(proj2, Some("better-mc-fabric-bmc2".to_string()));
        assert_eq!(ver2, None);

        let (proj3, ver3) = parse_modpack_input("better-mc-fabric-bmc2");
        assert_eq!(proj3, Some("better-mc-fabric-bmc2".to_string()));
        assert_eq!(ver3, None);

        let (proj4, ver4) = parse_modpack_input("M5BnAIQy");
        assert_eq!(proj4, Some("M5BnAIQy".to_string()));
        assert_eq!(ver4, None);
    }

    #[test]
    fn test_sanitize_instance_name() {
        assert_eq!(
            sanitize_instance_name("Better MC [FABRIC] - BMC2"),
            "better-mc-fabric-bmc2"
        );
        assert_eq!(sanitize_instance_name("My Server! 123"), "my-server-123");
        assert_eq!(
            sanitize_instance_name("already_good-name"),
            "already_good-name"
        );
    }

    #[test]
    fn test_env_server_supported() {
        let env_unsupported = ModpackEnv {
            client: Some("required".to_string()),
            server: Some("unsupported".to_string()),
        };
        assert!(!env_unsupported.is_server_supported());

        let env_required = ModpackEnv {
            client: Some("required".to_string()),
            server: Some("required".to_string()),
        };
        assert!(env_required.is_server_supported());

        let env_optional = ModpackEnv {
            client: Some("optional".to_string()),
            server: Some("optional".to_string()),
        };
        assert!(env_optional.is_server_supported());
    }

    #[test]
    fn test_extract_in_memory_mrpack() {
        use std::io::Write;
        let temp_dir =
            std::env::temp_dir().join(format!("mcserman_test_mrpack_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let zip_path = temp_dir.join("test.mrpack");

        // Construir zip de prueba en memoria
        {
            let file = std::fs::File::create(&zip_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);

            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);

            zip.start_file("modrinth.index.json", options).unwrap();
            let index_json = r#"{
                "formatVersion": 1,
                "game": "minecraft",
                "versionId": "1.0",
                "name": "Test Modpack",
                "files": [],
                "dependencies": {
                    "minecraft": "1.20.1",
                    "fabric-loader": "0.15.11"
                }
            }"#;
            zip.write_all(index_json.as_bytes()).unwrap();

            zip.start_file("overrides/config/test.toml", options)
                .unwrap();
            zip.write_all(b"key = 'overrides'\n").unwrap();

            zip.start_file("server-overrides/config/test.toml", options)
                .unwrap();
            zip.write_all(b"key = 'server-overrides'\n").unwrap();

            zip.finish().unwrap();
        }

        let index = extract_mrpack_index(&zip_path).unwrap();
        assert_eq!(index.name, "Test Modpack");
        assert_eq!(index.minecraft_version(), Some("1.20.1"));
        assert_eq!(index.loader_info(), (Some("fabric"), Some("0.15.11")));

        let out_dir = temp_dir.join("out");
        extract_mrpack_overrides(&zip_path, &out_dir).unwrap();

        let config_file = out_dir.join("config").join("test.toml");
        assert!(config_file.exists());
        let content = std::fs::read_to_string(&config_file).unwrap();
        assert_eq!(content, "key = 'server-overrides'\n");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    #[ignore = "requires network access (skipped in NixOS build sandbox)"]
    async fn test_modpack_better_mc() {
        let input = "https://modrinth.com/modpack/better-mc-fabric-bmc2/version/M5BnAIQy";
        let resolved = resolve_modpack(input)
            .await
            .expect("Error al resolver modpack Better MC");

        assert_eq!(resolved.version_id, "M5BnAIQy");
        assert_eq!(resolved.mc_version, "1.20.1");
        assert_eq!(resolved.loader, "fabric");
        assert!(resolved.mrpack_url.ends_with(".mrpack"));

        let client = reqwest::Client::new();
        let temp_dir =
            std::env::temp_dir().join(format!("mcserman_bmc2_test_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let mrpack_path = temp_dir.join("test.mrpack");

        let (tx, _rx) = tokio::sync::mpsc::channel(10);
        let reporter = crate::core::progress::ChannelProgressReporter::new(tx);

        // 1. Descargar .mrpack e interpretar
        download_mrpack_file(
            &client,
            &resolved.mrpack_url,
            resolved.mrpack_sha512.as_deref(),
            &mrpack_path,
            &reporter,
        )
        .await
        .expect("Error descargando .mrpack");
        assert!(mrpack_path.exists());

        let index = extract_mrpack_index(&mrpack_path).expect("Error leyendo modrinth.index.json");
        assert_eq!(index.minecraft_version(), Some("1.20.1"));

        // 2. Extraer overrides
        extract_mrpack_overrides(&mrpack_path, &temp_dir).expect("Error extrayendo overrides");

        // 3. Probar descarga de server jar Fabric 1.20.1
        let jar_path = temp_dir.join("fabric-server.jar");
        let (loader_name, loader_ver) = index.loader_info();
        assert_eq!(loader_name, Some("fabric"));

        let downloaded_ver = crate::loaders::fabric::download_fabric_custom(
            "1.20.1", loader_ver, &jar_path, &reporter,
        )
        .await
        .expect("Error descargando Fabric Server JAR");
        assert!(jar_path.exists());
        assert!(!downloaded_ver.is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
