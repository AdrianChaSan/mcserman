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

pub async fn download_paper(mc_version: &str, target_path: &Path) -> Result<String> {
    println!("{}", format!("📦 Resolviendo PaperMC para Minecraft {}...", mc_version).cyan());

    let client = reqwest::Client::new();
    let api_url = format!("https://api.papermc.io/v2/projects/paper/versions/{}", mc_version);

    let res = client.get(&api_url).send().await
        .with_context(|| format!("Error al conectar con PaperMC API en {}", api_url))?;

    if !res.status().is_success() {
        bail!("La versión '{}' no está soportada o no existe en PaperMC (HTTP {})", mc_version, res.status());
    }

    let info: PaperVersionInfo = res.json().await?;
    let latest_build = info.builds.iter().max()
        .with_context(|| format!("No se encontraron builds disponibles para Paper {}", mc_version))?;

    let jar_name = format!("paper-{}-{}.jar", mc_version, latest_build);
    let download_url = format!(
        "https://api.papermc.io/v2/projects/paper/versions/{}/builds/{}/downloads/{}",
        mc_version, latest_build, jar_name
    );

    let response = client.get(&download_url).send().await?;
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
