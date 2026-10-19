use std::collections::HashMap;
use std::path::Path;
use anyhow::{bail, Context, Result};
use colored::Colorize;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;

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

pub async fn download_paper(mc_version: &str, target_path: &Path) -> Result<String> {
    println!("{}", format!("📦 Resolviendo PaperMC para Minecraft {}...", mc_version).cyan());

    let client = reqwest::Client::builder()
        .user_agent("mcserman/0.1.0 (https://github.com/AdrianChaSan/mcserman)")
        .build()?;

    let version_url = format!("https://fill.papermc.io/v3/projects/paper/versions/{}", mc_version);

    let res = client.get(&version_url).send().await
        .with_context(|| format!("Error al conectar con PaperMC API en {}", version_url))?;

    if !res.status().is_success() {
        bail!("La versión '{}' no está soportada o no existe en PaperMC (HTTP {})", mc_version, res.status());
    }

    let info: PaperVersionInfo = res.json().await?;
    let latest_build = info.builds.iter().max()
        .with_context(|| format!("No se encontraron builds disponibles para Paper {}", mc_version))?;

    let build_url = format!(
        "https://fill.papermc.io/v3/projects/paper/versions/{}/builds/{}",
        mc_version, latest_build
    );

    let build_res = client.get(&build_url).send().await
        .with_context(|| format!("Error al consultar build #{} en PaperMC API", latest_build))?;

    if !build_res.status().is_success() {
        bail!("No se pudo obtener información del build #{} de PaperMC (HTTP {})", latest_build, build_res.status());
    }

    let build_info: PaperBuildInfo = build_res.json().await?;
    let download = build_info.downloads.get("server:default")
        .or_else(|| build_info.downloads.values().find(|d| d.name.ends_with(".jar")))
        .with_context(|| format!("No se encontró archivo de descarga para Paper build #{}", latest_build))?;

    let response = client.get(&download.url).send().await
        .with_context(|| format!("Error al descargar jar de Paper desde {}", download.url))?;
    let total_size = response.content_length().unwrap_or(0);

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")?
            .progress_chars("#>-"),
    );
    pb.set_message(format!("Descargando Paper build #{}", latest_build));

    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = std::fs::File::create(target_path)?;
    let mut stream = response.bytes_stream();

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res?;
        std::io::copy(&mut chunk.as_ref(), &mut file)?;
        pb.inc(chunk.len() as u64);
    }

    pb.finish_with_message("Descarga de PaperMC completada");
    println!("{}", format!("✓ Paper {} (build #{}) guardado en {}", mc_version, latest_build, target_path.display()).green());
    Ok(latest_build.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
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
