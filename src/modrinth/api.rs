use std::path::Path;
use anyhow::{bail, Context, Result};
use colored::Colorize;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;
use sha2::{Digest, Sha512};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct SearchHit {
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub slug: String,
    pub client_side: String,
    pub server_side: String,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    hits: Vec<SearchHit>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ProjectVersion {
    pub id: String,
    pub name: String,
    pub version_number: String,
    pub files: Vec<VersionFile>,
}

#[derive(Debug, Deserialize)]
pub struct VersionFile {
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub size: u64,
    pub hashes: FileHashes,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct FileHashes {
    pub sha512: Option<String>,
    pub sha1: Option<String>,
}

pub struct ModrinthClient {
    client: reqwest::Client,
}

impl ModrinthClient {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("mcserman/0.1.0 (https://github.com/AdrianChaSan/mcserman)")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client }
    }

    pub async fn search(&self, query: &str, loader: &str, mc_version: &str) -> Result<Vec<SearchHit>> {
        let facets = format!(
            r#"[["categories:{}"],["versions:{}"]]"#,
            loader, mc_version
        );
        let url = format!(
            "https://api.modrinth.com/v2/search?query={}&facets={}&limit=10",
            urlencoding::encode(query),
            urlencoding::encode(&facets)
        );

        let res: SearchResponse = self.client.get(&url).send().await?
            .json().await
            .context("Error al consultar la API de Modrinth")?;

        Ok(res.hits)
    }

    pub async fn get_compatible_version(&self, project_id: &str, loader: &str, mc_version: &str) -> Result<ProjectVersion> {
        let loaders_json = format!(r#"["{}"]"#, loader);
        let versions_json = format!(r#"["{}"]"#, mc_version);
        let url = format!(
            "https://api.modrinth.com/v2/project/{}/version?loaders={}&game_versions={}",
            project_id,
            urlencoding::encode(&loaders_json),
            urlencoding::encode(&versions_json)
        );

        let versions: Vec<ProjectVersion> = self.client.get(&url).send().await?
            .json().await
            .context("Error al obtener versiones del proyecto en Modrinth")?;

        versions.into_iter().next()
            .with_context(|| format!("No se encontró ninguna versión compatible con {} en MC {}", loader, mc_version))
    }

    pub async fn download_file(&self, file_info: &VersionFile, target_dir: &Path) -> Result<std::path::PathBuf> {
        std::fs::create_dir_all(target_dir)?;
        let target_path = target_dir.join(&file_info.filename);

        println!("{}", format!("📥 Descargando {}...", file_info.filename).cyan());

        let response = self.client.get(&file_info.url).send().await
            .with_context(|| format!("Fallo al descargar archivo desde {}", file_info.url))?;

        let pb = ProgressBar::new(file_info.size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")?
                .progress_chars("#>-"),
        );
        pb.set_message(format!("{}", file_info.filename));

        let mut file = std::fs::File::create(&target_path)?;
        let mut stream = response.bytes_stream();
        let mut hasher = Sha512::new();

        while let Some(chunk_res) = stream.next().await {
            let chunk = chunk_res?;
            std::io::copy(&mut chunk.as_ref(), &mut file)?;
            hasher.update(&chunk);
            pb.inc(chunk.len() as u64);
        }

        pb.finish_with_message("Descarga finalizada");

        // Verificación de integridad SHA-512
        if let Some(ref expected_sha512) = file_info.hashes.sha512 {
            let calculated_hash = hex::encode(hasher.finalize());
            if !calculated_hash.eq_ignore_ascii_case(expected_sha512) {
                let _ = std::fs::remove_file(&target_path);
                bail!(
                    "¡CORRUPCIÓN DE ARCHIVO! El hash SHA-512 de {} no coincide.\nEsperado: {}\nObtenido: {}",
                    file_info.filename, expected_sha512, calculated_hash
                );
            }
            println!("{}", format!("🔒 Hash SHA-512 verificado exitosamente").green());
        }

        println!("{}", format!("✓ Guardado en {}", target_path.display()).green());
        Ok(target_path)
    }
}
