use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Context, Result, bail};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use futures_util::StreamExt;
use crate::core::paths::SystemPaths;

pub fn required_java_version(mc_version: &str) -> u8 {
    let parts: Vec<&str> = mc_version.split('.').collect();
    if parts.len() < 2 {
        return 21; // Default moderno seguro
    }

    let major: u32 = parts[0].parse().unwrap_or(1);
    let minor: u32 = parts[1].parse().unwrap_or(20);
    let patch: u32 = if parts.len() > 2 {
        parts[2].parse().unwrap_or(0)
    } else {
        0
    };

    if major == 1 {
        if minor <= 16 {
            8
        } else if minor == 17 {
            16
        } else if minor < 20 || (minor == 20 && patch <= 4) {
            17
        } else {
            21 // 1.20.5 en adelante
        }
    } else if major >= 26 {
        // Minecraft 26.x+ snapshots requieren Java 25
        25
    } else {
        21
    }
}

pub fn aikar_g1gc_flags() -> Vec<String> {
    vec![
        "-XX:+UseG1GC".to_string(),
        "-XX:+ParallelRefProcEnabled".to_string(),
        "-XX:MaxGCPauseMillis=200".to_string(),
        "-XX:+UnlockExperimentalVMOptions".to_string(),
        "-XX:+DisableExplicitGC".to_string(),
        "-XX:+AlwaysPreTouch".to_string(),
        "-XX:G1NewSizePercent=30".to_string(),
        "-XX:G1MaxNewSizePercent=40".to_string(),
        "-XX:G1ReservePercent=20".to_string(),
        "-XX:G1HeapWastePercent=5".to_string(),
        "-XX:G1MixedGCCountTarget=4".to_string(),
        "-XX:InitiatingHeapOccupancyPercent=15".to_string(),
        "-XX:G1MixedGCLiveThresholdPercent=90".to_string(),
        "-XX:G1RSetUpdatingPauseTimePercent=5".to_string(),
        "-XX:SurvivorRatio=32".to_string(),
        "-XX:+PerfDisableSharedMem".to_string(),
        "-XX:MaxTenuringThreshold=1".to_string(),
    ]
}

pub async fn resolve_java(version: u8, paths: &SystemPaths) -> Result<PathBuf> {
    // 1. Variable de entorno inyectada por NixOS o el usuario
    let env_var_name = format!("MCSERVER_JAVA_{}_PATH", version);
    if let Ok(env_path) = std::env::var(&env_var_name) {
        let p = PathBuf::from(env_path);
        let java_bin = if p.is_dir() { p.join("bin").join("java") } else { p };
        if java_bin.exists() {
            return Ok(java_bin);
        }
    }

    // 2. Verificar si ya tenemos el runtime descargado en runtimes_dir
    let local_runtime_dir = paths.runtimes_dir.join(format!("java-{}", version));
    if let Some(java_bin) = find_java_binary(&local_runtime_dir) {
        return Ok(java_bin);
    }

    // 3. Verificar si el comando `java` en el PATH del sistema coincide con la versión
    let java_candidates = [which::which("java").ok(), Some(PathBuf::from("java"))];
    for candidate in java_candidates.into_iter().flatten() {
        if let Ok(detected_ver) = query_java_version(&candidate) {
            if detected_ver == version {
                return Ok(candidate);
            }
        }
    }

    // 4. Auto-descarga desde Adoptium API (Eclipse Temurin)
    println!("{}", format!("☕ Java {} no encontrado. Descargando automáticamente Eclipse Temurin...", version).cyan().bold());
    let downloaded_bin = download_adoptium_java(version, &local_runtime_dir).await?;
    Ok(downloaded_bin)
}

fn query_java_version(java_cmd: &Path) -> Result<u8> {
    let output = Command::new(java_cmd)
        .arg("-version")
        .output()
        .context("No se pudo ejecutar el comando java para verificar versión")?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}\n{}", stdout, stderr);

    // Parsear ej: "openjdk version \"21.0.6\"" o "java version \"1.8.0_382\""
    for line in combined.lines() {
        if line.contains("version") {
            if let Some(start) = line.find('"') {
                if let Some(end) = line[start + 1..].find('"') {
                    let ver_str = &line[start + 1..start + 1 + end];
                    if ver_str.starts_with("1.8") {
                        return Ok(8);
                    }
                    if let Some(major_str) = ver_str.split('.').next() {
                        if let Ok(major) = major_str.parse::<u8>() {
                            return Ok(major);
                        }
                    }
                }
            }
        }
    }

    bail!("No se pudo determinar la versión de Java ejecutando '{}'", java_cmd.display())
}

fn find_java_binary(dir: &Path) -> Option<PathBuf> {
    if !dir.exists() {
        return None;
    }
    // Revisar dir/bin/java
    let direct = dir.join("bin").join("java");
    if direct.exists() {
        return Some(direct);
    }
    // Revisar subdirectorios de extracción (ej. jdk-21.0.6+7/bin/java)
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path().join("bin").join("java");
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}

async fn download_adoptium_java(version: u8, target_dir: &Path) -> Result<PathBuf> {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        other => bail!("Arquitectura no soportada para descarga automática de Java: {}", other),
    };

    let url = format!(
        "https://api.adoptium.net/v3/binary/latest/{}/ga/linux/{}/jdk/hotspot/normal/eclipse?project=jdk",
        version, arch
    );

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;

    let response = client.get(&url).send().await
        .with_context(|| format!("Error al contactar Adoptium API en {}", url))?;

    if !response.status().is_success() {
        bail!("Adoptium API retornó estado HTTP {} para Java {}", response.status(), version);
    }

    let total_size = response.content_length().unwrap_or(0);
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")?
            .progress_chars("#>-"),
    );
    pb.set_message(format!("Descargando Java {}", version));

    std::fs::create_dir_all(target_dir)?;
    let temp_tar = target_dir.join("runtime.tar.gz");
    let mut file = std::fs::File::create(&temp_tar)?;
    let mut stream = response.bytes_stream();

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res?;
        std::io::copy(&mut chunk.as_ref(), &mut file)?;
        pb.inc(chunk.len() as u64);
    }
    pb.finish_with_message("Descarga de Java completada. Extrayendo...");

    // Descomprimir tar.gz
    let tar_file = std::fs::File::open(&temp_tar)?;
    let decompressor = flate2::read::GzDecoder::new(tar_file);
    let mut archive = tar::Archive::new(decompressor);
    archive.unpack(target_dir)?;

    let _ = std::fs::remove_file(temp_tar);

    if let Some(java_bin) = find_java_binary(target_dir) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&java_bin)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&java_bin, perms)?;
        }
        println!("{}", format!("✓ Java {} instalado exitosamente en {}", version, java_bin.display()).green());
        Ok(java_bin)
    } else {
        bail!("Se extrajo el runtime de Java pero no se encontró el binario en {}", target_dir.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_required_java_versions() {
        assert_eq!(required_java_version("1.12.2"), 8);
        assert_eq!(required_java_version("1.16.5"), 8);
        assert_eq!(required_java_version("1.17"), 16);
        assert_eq!(required_java_version("1.17.1"), 16);
        assert_eq!(required_java_version("1.18.2"), 17);
        assert_eq!(required_java_version("1.19.4"), 17);
        assert_eq!(required_java_version("1.20.1"), 17);
        assert_eq!(required_java_version("1.20.4"), 17);
        assert_eq!(required_java_version("1.20.5"), 21);
        assert_eq!(required_java_version("1.21.1"), 21);
        assert_eq!(required_java_version("1.21.4"), 21);
        assert_eq!(required_java_version("26.1"), 25);
    }

    #[test]
    fn test_aikar_flags_contains_g1gc() {
        let flags = aikar_g1gc_flags();
        assert!(flags.contains(&"-XX:+UseG1GC".to_string()));
        assert!(flags.contains(&"-XX:+AlwaysPreTouch".to_string()));
    }
}
