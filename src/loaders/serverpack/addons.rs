use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::Path;

use super::archive::download_serverpack_file;
use super::curseforge::{
    is_curseforge_url, parse_curseforge_url, CfFilesResponse, CURSEFORGE_API_URL,
    DEFAULT_CURSEFORGE_KEY,
};
use crate::core::progress::ProgressReporter;

/// Comprueba si la cadena representa una URL directa hacia un binario .jar.
pub fn is_jar_url(input: &str) -> bool {
    let lower = input.to_lowercase();
    if let Some(clean) = lower.split('?').next() {
        clean.ends_with(".jar")
    } else {
        lower.ends_with(".jar")
    }
}

/// Descarga e instala un mod o plugin resolviendo URLs directas, CurseForge o slugs de Modrinth.
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

    if (trimmed.starts_with("http://") || trimmed.starts_with("https://")) && is_jar_url(trimmed) {
        return download_direct_jar(trimmed, target_dir, reporter).await;
    }

    if is_curseforge_url(trimmed) {
        return download_curseforge_mod(trimmed, mc_version, loader, target_dir, reporter).await;
    }

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
        #[allow(dead_code)]
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
