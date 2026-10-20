use crate::core::progress::ProgressReporter;
use anyhow::{Context, Result};
use futures_util::StreamExt;
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct VersionManifest {
    versions: Vec<VersionEntry>,
}

#[derive(Deserialize)]
struct VersionEntry {
    id: String,
    url: String,
}

#[derive(Deserialize)]
struct VersionPackage {
    downloads: Downloads,
}

#[derive(Deserialize)]
struct Downloads {
    server: Option<DownloadEntry>,
}

#[derive(Deserialize)]
struct DownloadEntry {
    url: String,
    size: u64,
}

pub async fn download_vanilla(
    mc_version: &str,
    target_path: &Path,
    reporter: &dyn ProgressReporter,
) -> Result<()> {
    reporter.on_info(&format!(
        "📦 Resolviendo Vanilla Minecraft {}...",
        mc_version
    ));

    let client = reqwest::Client::new();
    let manifest_url = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
    let manifest: VersionManifest = client
        .get(manifest_url)
        .send()
        .await?
        .json()
        .await
        .context("Error al consultar el manifiesto de versiones de Mojang")?;

    let version_entry = manifest
        .versions
        .iter()
        .find(|v| v.id == mc_version)
        .with_context(|| {
            format!(
                "La versión '{}' de Minecraft no existe en Mojang",
                mc_version
            )
        })?;

    let package: VersionPackage = client
        .get(&version_entry.url)
        .send()
        .await?
        .json()
        .await
        .context("Error al consultar el paquete de la versión de Mojang")?;

    let server_dl = package.downloads.server.with_context(|| {
        format!(
            "Mojang no provee binario de servidor para la versión {}",
            mc_version
        )
    })?;

    let response = client.get(&server_dl.url).send().await?;
    reporter.on_start(
        &format!("Descargando server.jar ({})", mc_version),
        server_dl.size,
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
        reporter.on_progress(downloaded, server_dl.size);
    }

    reporter.on_finish(&format!(
        "Servidor Vanilla {} guardado en {}",
        mc_version,
        target_path.display()
    ));
    Ok(())
}
