use crate::core::progress::ProgressReporter;
use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Deserialize)]
struct PaperVersionInfo {
    builds: Vec<u32>,
}

#[derive(Deserialize)]
struct PaperBuildDownload {
    name: String,
    url: String,
    #[allow(dead_code)]
    size: Option<u64>,
}

#[derive(Deserialize)]
struct PaperBuildInfo {
    downloads: HashMap<String, PaperBuildDownload>,
}

pub async fn download_paper(
    mc_version: &str,
    target_path: &Path,
    reporter: &dyn ProgressReporter,
) -> Result<String> {
    reporter.on_info(&format!(
        "📦 Resolviendo PaperMC para Minecraft {}...",
        mc_version
    ));

    let client = reqwest::Client::builder()
        .user_agent("mcserman/0.1.0 (https://github.com/AdrianChaSan/mcserman)")
        .build()?;

    let version_url = format!(
        "https://fill.papermc.io/v3/projects/paper/versions/{}",
        mc_version
    );

    let res = client
        .get(&version_url)
        .send()
        .await
        .with_context(|| format!("Error al conectar con PaperMC API en {}", version_url))?;

    if !res.status().is_success() {
        bail!(
            "La versión '{}' no está soportada o no existe en PaperMC (HTTP {})",
            mc_version,
            res.status()
        );
    }

    let info: PaperVersionInfo = res.json().await?;
    let latest_build = info.builds.iter().max().with_context(|| {
        format!(
            "No se encontraron builds disponibles para Paper {}",
            mc_version
        )
    })?;

    let build_url = format!(
        "https://fill.papermc.io/v3/projects/paper/versions/{}/builds/{}",
        mc_version, latest_build
    );

    let build_res =
        client.get(&build_url).send().await.with_context(|| {
            format!("Error al consultar build #{} en PaperMC API", latest_build)
        })?;

    if !build_res.status().is_success() {
        bail!(
            "No se pudo obtener información del build #{} de PaperMC (HTTP {})",
            latest_build,
            build_res.status()
        );
    }

    let build_info: PaperBuildInfo = build_res.json().await?;
    let download = build_info
        .downloads
        .get("server:default")
        .or_else(|| {
            build_info
                .downloads
                .values()
                .find(|d| d.name.ends_with(".jar"))
        })
        .with_context(|| {
            format!(
                "No se encontró archivo de descarga para Paper build #{}",
                latest_build
            )
        })?;

    let response = client
        .get(&download.url)
        .send()
        .await
        .with_context(|| format!("Error al descargar jar de Paper desde {}", download.url))?;
    let total_size = response.content_length().unwrap_or(0);

    reporter.on_start(
        &format!("Descargando Paper build #{}", latest_build),
        total_size,
    );

    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = std::fs::File::create(target_path)?;
    let mut stream = response.bytes_stream();
    let mut downloaded = 0u64;

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res?;
        std::io::copy(&mut chunk.as_ref(), &mut file)?;
        downloaded += chunk.len() as u64;
        reporter.on_progress(downloaded, total_size);
    }

    reporter.on_finish(&format!(
        "Paper {} (build #{}) guardado en {}",
        mc_version,
        latest_build,
        target_path.display()
    ));
    Ok(latest_build.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires network access (skipped in NixOS build sandbox)"]
    async fn test_papermc_v3_api_resolves() {
        let client = reqwest::Client::builder()
            .user_agent("mcserman/0.1.0 (https://github.com/AdrianChaSan/mcserman)")
            .build()
            .unwrap();
        let version_url = "https://fill.papermc.io/v3/projects/paper/versions/1.21.4";
        let res = client.get(version_url).send().await.unwrap();
        assert!(res.status().is_success());
        let info: PaperVersionInfo = res.json().await.unwrap();
        assert!(!info.builds.is_empty());
    }
}
