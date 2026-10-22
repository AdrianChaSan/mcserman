use crate::core::progress::ProgressReporter;
use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use serde::Deserialize;
use std::path::Path;

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

pub async fn download_fabric_custom(
    mc_version: &str,
    loader_version: Option<&str>,
    target_path: &Path,
    reporter: &dyn ProgressReporter,
) -> Result<String> {
    let client = reqwest::Client::new();

    // 1. Obtener última versión estable del Loader (o usar la especificada)
    let loader_url = "https://meta.fabricmc.net/v2/versions/loader";
    let loaders: Vec<FabricLoaderVersion> = client
        .get(loader_url)
        .send()
        .await?
        .json()
        .await
        .context("Error al consultar versiones de Fabric Loader")?;

    let latest_stable_loader = loaders
        .iter()
        .find(|l| l.stable)
        .or_else(|| loaders.first())
        .with_context(|| "No se encontraron versiones de Fabric Loader")?;

    let mut chosen_loader_version = loader_version
        .map(|s| s.to_string())
        .unwrap_or_else(|| latest_stable_loader.version.clone());

    reporter.on_info(&format!(
        "📦 Resolviendo Fabric Loader {} para Minecraft {}...",
        chosen_loader_version, mc_version
    ));

    // 2. Obtener última versión estable del Installer
    let installer_url = "https://meta.fabricmc.net/v2/versions/installer";
    let installers: Vec<FabricInstallerVersion> = client
        .get(installer_url)
        .send()
        .await?
        .json()
        .await
        .context("Error al consultar versiones de Fabric Installer")?;

    let latest_installer = installers
        .iter()
        .find(|i| i.stable)
        .or_else(|| installers.first())
        .with_context(|| "No se encontraron versiones de Fabric Installer")?;

    // 3. Descargar el server jar preempaquetado de Fabric
    let download_url = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{}/{}/{}/server/jar",
        mc_version, chosen_loader_version, latest_installer.version
    );

    let mut response = client
        .get(&download_url)
        .send()
        .await
        .with_context(|| format!("Error al descargar Fabric desde {}", download_url))?;

    // Fallback a latest_stable si la versión requerida no existe
    if !response.status().is_success() && loader_version.is_some() {
        let fallback_url = format!(
            "https://meta.fabricmc.net/v2/versions/loader/{}/{}/{}/server/jar",
            mc_version, latest_stable_loader.version, latest_installer.version
        );
        if let Ok(fb_resp) = client.get(&fallback_url).send().await {
            if fb_resp.status().is_success() {
                response = fb_resp;
                chosen_loader_version = latest_stable_loader.version.clone();
            }
        }
    }

    if !response.status().is_success() {
        bail!(
            "Fabric Meta retornó error HTTP {} para MC {} y Loader {}",
            response.status(),
            mc_version,
            chosen_loader_version
        );
    }

    let total_size = response.content_length().unwrap_or(0);
    reporter.on_start(
        &format!("Descargando Fabric Loader ({})", chosen_loader_version),
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
        "Fabric Loader {} guardado en {}",
        chosen_loader_version,
        target_path.display()
    ));
    Ok(chosen_loader_version)
}

pub async fn download_fabric(
    mc_version: &str,
    target_path: &Path,
    reporter: &dyn ProgressReporter,
) -> Result<String> {
    download_fabric_custom(mc_version, None, target_path, reporter).await
}
