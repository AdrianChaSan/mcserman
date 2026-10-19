use std::path::Path;
use anyhow::{bail, Context, Result};
use colored::Colorize;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;

#[derive(Deserialize)]
struct FabricLoaderVersion {
    version: String,
    stable: bool,
}

#[derive(Deserialize)]
struct FabricInstallerVersion {
    version: String,
    stable: bool,
}

pub async fn download_fabric(mc_version: &str, target_path: &Path) -> Result<String> {
    println!("{}", format!("📦 Resolviendo Fabric Loader para Minecraft {}...", mc_version).cyan());

    let client = reqwest::Client::new();

    // 1. Obtener última versión estable del Loader
    let loader_url = "https://meta.fabricmc.net/v2/versions/loader";
    let loaders: Vec<FabricLoaderVersion> = client.get(loader_url).send().await?
        .json().await
        .context("Error al consultar versiones de Fabric Loader")?;

    let latest_loader = loaders.iter().find(|l| l.stable).or_else(|| loaders.first())
        .with_context(|| "No se encontraron versiones de Fabric Loader")?;

    // 2. Obtener última versión estable del Installer
    let installer_url = "https://meta.fabricmc.net/v2/versions/installer";
    let installers: Vec<FabricInstallerVersion> = client.get(installer_url).send().await?
        .json().await
        .context("Error al consultar versiones de Fabric Installer")?;

    let latest_installer = installers.iter().find(|i| i.stable).or_else(|| installers.first())
        .with_context(|| "No se encontraron versiones de Fabric Installer")?;

    // 3. Descargar el server jar preempaquetado de Fabric
    let download_url = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{}/{}/{}/server/jar",
        mc_version, latest_loader.version, latest_installer.version
    );

    let response = client.get(&download_url).send().await
        .with_context(|| format!("Error al descargar Fabric desde {}", download_url))?;

    if !response.status().is_success() {
        bail!("Fabric Meta retornó error HTTP {} para MC {}", response.status(), mc_version);
    }

    let total_size = response.content_length().unwrap_or(0);
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")?
            .progress_chars("#>-"),
    );
    pb.set_message(format!("Descargando Fabric Loader ({})", latest_loader.version));

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

    pb.finish_with_message("Descarga de Fabric completada");
    println!("{}", format!("✓ Fabric Loader {} guardado en {}", latest_loader.version, target_path.display()).green());
    Ok(latest_loader.version.clone())
}
