use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::core::progress::ProgressReporter;

/// Descarga un archivo de paquete de servidor reportando el progreso de transferencia.
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

/// Extrae el archivo ZIP preservando permisos ejecutables POSIX y desenvolviendo carpetas raíz únicas.
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
