use crate::core::paths::SystemPaths;
use crate::core::progress::ProgressReporter;
use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Representa el entorno o binario a través del cual se ejecuta la JVM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JavaRuntime {
    /// Binario directo en el sistema de archivos (ej. /nix/store/.../bin/java o /usr/bin/java)
    Direct(PathBuf),
    /// Envoltorio dinámico mediante nix shell (ej. nix shell nixpkgs#jdk21 -c java ...)
    NixShell { package: String },
}

impl JavaRuntime {
    /// Configura un comando asíncrono `tokio::process::Command` para ejecutar la JVM.
    pub fn build_tokio_command(&self, jvm_args: &[String]) -> tokio::process::Command {
        match self {
            JavaRuntime::Direct(path) => {
                let mut cmd = tokio::process::Command::new(path);
                cmd.args(jvm_args);
                cmd
            }
            JavaRuntime::NixShell { package } => {
                let mut cmd = tokio::process::Command::new("nix");
                cmd.args([
                    "--extra-experimental-features",
                    "nix-command flakes",
                    "shell",
                    package,
                    "-c",
                    "java",
                ]);
                cmd.args(jvm_args);
                cmd
            }
        }
    }

    /// Configura un comando estándar `std::process::Command` para ejecutar la JVM.
    #[allow(dead_code)]
    pub fn build_std_command(&self, jvm_args: &[String]) -> std::process::Command {
        match self {
            JavaRuntime::Direct(path) => {
                let mut cmd = std::process::Command::new(path);
                cmd.args(jvm_args);
                cmd
            }
            JavaRuntime::NixShell { package } => {
                let mut cmd = std::process::Command::new("nix");
                cmd.args([
                    "--extra-experimental-features",
                    "nix-command flakes",
                    "shell",
                    package,
                    "-c",
                    "java",
                ]);
                cmd.args(jvm_args);
                cmd
            }
        }
    }

    /// Retorna una representación legible del comando de ejecución.
    pub fn display_cmd(&self) -> String {
        match self {
            JavaRuntime::Direct(path) => path.display().to_string(),
            JavaRuntime::NixShell { package } => format!("nix shell {} -c java", package),
        }
    }

    /// Retorna la ruta al binario si la ejecución es directa.
    #[allow(dead_code)]
    pub fn direct_path(&self) -> Option<&Path> {
        match self {
            JavaRuntime::Direct(path) => Some(path.as_path()),
            JavaRuntime::NixShell { .. } => None,
        }
    }
}

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

pub fn is_nix_available() -> bool {
    which::which("nix").is_ok() || Path::new("/run/current-system/sw/bin/nix").exists()
}

pub fn nix_package_for_java(version: u8) -> Option<String> {
    match version {
        8 => Some("nixpkgs#jdk8".to_string()),
        17 => Some("nixpkgs#jdk17".to_string()),
        21 => Some("nixpkgs#jdk21".to_string()),
        25 => Some("nixpkgs#jdk25".to_string()),
        _ => None,
    }
}

pub fn find_all_local_javas(paths: &SystemPaths) -> Vec<(u8, PathBuf)> {
    let mut found = Vec::new();
    let mut checked_paths = std::collections::HashSet::new();

    let mut add_candidate = |p: PathBuf| {
        let bin = if p.is_dir() {
            p.join("bin").join("java")
        } else {
            p
        };
        if bin.exists() && checked_paths.insert(bin.clone()) {
            if let Ok(ver) = query_java_version(&bin) {
                found.push((ver, bin));
            }
        }
    };

    for v in [8, 17, 21, 25] {
        if let Ok(val) = std::env::var(format!("MCSERVER_JAVA_{}_PATH", v)) {
            add_candidate(PathBuf::from(val));
        }
    }

    if Path::new("/etc/set-environment").exists() {
        if let Ok(content) = std::fs::read_to_string("/etc/set-environment") {
            for line in content.lines() {
                for v in [8, 17, 21, 25] {
                    let prefix = format!("export MCSERVER_JAVA_{}_PATH=", v);
                    if let Some(rest) = line.strip_prefix(&prefix) {
                        let path_str = rest.trim_matches('"').trim_matches('\'');
                        add_candidate(PathBuf::from(path_str));
                    }
                }
            }
        }
    }

    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        add_candidate(PathBuf::from(java_home));
    }

    if let Ok(p) = which::which("java") {
        add_candidate(p);
    }
    add_candidate(PathBuf::from("/run/current-system/sw/bin/java"));

    for v in [8, 16, 17, 21, 25] {
        let dir = paths.runtimes_dir.join(format!("java-{}", v));
        if let Some(bin) = find_java_binary(&dir) {
            add_candidate(bin);
        }
    }

    found
}

pub async fn resolve_java(
    version: u8,
    paths: &SystemPaths,
    reporter: &dyn ProgressReporter,
) -> Result<JavaRuntime> {
    // Variable de entorno inyectada por NixOS o el usuario
    let env_var_name = format!("MCSERVER_JAVA_{}_PATH", version);
    if let Ok(env_path) = std::env::var(&env_var_name) {
        let p = PathBuf::from(env_path);
        let java_bin = if p.is_dir() {
            p.join("bin").join("java")
        } else {
            p
        };
        if java_bin.exists() {
            reporter.on_info(&format!(
                "☕ Java {} encontrado en variable {}: {}",
                version,
                env_var_name,
                java_bin.display()
            ));
            return Ok(JavaRuntime::Direct(java_bin));
        }
    }

    // Consulta de /etc/set-environment para sessionVariables en NixOS
    if Path::new("/etc/set-environment").exists() {
        if let Ok(content) = std::fs::read_to_string("/etc/set-environment") {
            for line in content.lines() {
                if let Some(rest) = line.strip_prefix(&format!("export {}=", env_var_name)) {
                    let path_str = rest.trim_matches('"').trim_matches('\'');
                    let p = PathBuf::from(path_str);
                    let java_bin = if p.is_dir() {
                        p.join("bin").join("java")
                    } else {
                        p
                    };
                    if java_bin.exists() {
                        reporter.on_info(&format!(
                            "☕ Java {} encontrado en /etc/set-environment: {}",
                            version,
                            java_bin.display()
                        ));
                        return Ok(JavaRuntime::Direct(java_bin));
                    }
                }
            }
        }
    }

    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let p = PathBuf::from(java_home);
        let java_bin = if p.is_dir() {
            p.join("bin").join("java")
        } else {
            p
        };
        if java_bin.exists() {
            if let Ok(detected_ver) = query_java_version(&java_bin) {
                if detected_ver == version {
                    reporter.on_info(&format!(
                        "☕ Java {} encontrado en JAVA_HOME: {}",
                        version,
                        java_bin.display()
                    ));
                    return Ok(JavaRuntime::Direct(java_bin));
                }
            }
        }
    }

    let java_candidates = [
        which::which("java").ok(),
        Some(PathBuf::from("/run/current-system/sw/bin/java")),
        Some(PathBuf::from("java")),
    ];
    for candidate in java_candidates.into_iter().flatten() {
        if candidate.exists() {
            if let Ok(detected_ver) = query_java_version(&candidate) {
                if detected_ver == version {
                    reporter.on_info(&format!(
                        "☕ Java {} detectado en PATH del sistema: {}",
                        version,
                        candidate.display()
                    ));
                    return Ok(JavaRuntime::Direct(candidate));
                }
            }
        }
    }

    let local_runtime_dir = paths.runtimes_dir.join(format!("java-{}", version));
    if let Some(java_bin) = find_java_binary(&local_runtime_dir) {
        if query_java_version(&java_bin).is_ok() {
            reporter.on_info(&format!(
                "☕ Java {} encontrado en runtimes locales: {}",
                version,
                java_bin.display()
            ));
            return Ok(JavaRuntime::Direct(java_bin));
        }
    }

    let on_nixos = Path::new("/etc/NIXOS").exists() || is_nix_available();
    if on_nixos {
        if is_nix_available() {
            if let Some(pkg) = nix_package_for_java(version) {
                reporter.on_info(&format!(
                    "☕ Java {} no está en PATH local. Envolviendo dinámicamente con '{}'...",
                    version, pkg
                ));
                return Ok(JavaRuntime::NixShell { package: pkg });
            }
        }

        // Fallback al JDK local más cercano superior disponible en el sistema (ej. Java 17 para versión 16)
        let local_javas = find_all_local_javas(paths);
        let closest_superior = local_javas
            .into_iter()
            .filter(|(v, _)| *v >= version)
            .min_by_key(|(v, _)| *v);

        if let Some((sup_ver, sup_bin)) = closest_superior {
            reporter.on_info(&format!(
                "☕ Java {} no disponible directamente. Usando fallback al JDK {} superior local: {}",
                version, sup_ver, sup_bin.display()
            ));
            return Ok(JavaRuntime::Direct(sup_bin));
        }

        // Si no hay paquete exacto ni JDK local superior, usar el siguiente JDK LTS en nixpkgs
        if is_nix_available() {
            let next_pkg = if version <= 17 {
                "nixpkgs#jdk17".to_string()
            } else if version <= 21 {
                "nixpkgs#jdk21".to_string()
            } else {
                "nixpkgs#jdk25".to_string()
            };
            reporter.on_info(&format!(
                "☕ Java {} no disponible en nixpkgs. Envolviendo dinámicamente con '{}'...",
                version, next_pkg
            ));
            return Ok(JavaRuntime::NixShell { package: next_pkg });
        }

        bail!(
            "Java {} no está disponible en NixOS y no se encontró una versión superior en el sistema ni comando 'nix'.\n\
             Configura 'services.mcserman.javaVersions = [ 8 17 21 25 ];' en tu configuración de NixOS.",
            version
        );
    }

    // Descarga desde Adoptium API para distribuciones estándar
    reporter.on_info(&format!(
        "☕ Java {} no encontrado. Descargando automáticamente Eclipse Temurin...",
        version
    ));
    let downloaded_bin = download_adoptium_java(version, &local_runtime_dir, reporter).await?;
    Ok(JavaRuntime::Direct(downloaded_bin))
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

    bail!(
        "No se pudo determinar la versión de Java ejecutando '{}'",
        java_cmd.display()
    )
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

async fn download_adoptium_java(
    version: u8,
    target_dir: &Path,
    reporter: &dyn ProgressReporter,
) -> Result<PathBuf> {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        other => bail!(
            "Arquitectura no soportada para descarga automática de Java: {}",
            other
        ),
    };

    let url = format!(
        "https://api.adoptium.net/v3/binary/latest/{}/ga/linux/{}/jdk/hotspot/normal/eclipse?project=jdk",
        version, arch
    );

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;

    let response = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("Error al contactar Adoptium API en {}", url))?;

    if !response.status().is_success() {
        bail!(
            "Adoptium API retornó estado HTTP {} para Java {}",
            response.status(),
            version
        );
    }

    let total_size = response.content_length().unwrap_or(0);
    reporter.on_start(&format!("Descargando Java {}", version), total_size);

    std::fs::create_dir_all(target_dir)?;
    let temp_tar = target_dir.join("runtime.tar.gz");
    let mut file = std::fs::File::create(&temp_tar)?;
    let mut stream = response.bytes_stream();
    let mut downloaded = 0u64;

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res?;
        std::io::copy(&mut chunk.as_ref(), &mut file)?;
        downloaded += chunk.len() as u64;
        reporter.on_progress(downloaded, total_size);
    }
    reporter.on_info("Descarga de Java completada. Extrayendo runtime...");

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
        reporter.on_finish(&format!(
            "Java {} instalado exitosamente en {}",
            version,
            java_bin.display()
        ));
        Ok(java_bin)
    } else {
        bail!(
            "Se extrajo el runtime de Java pero no se encontró el binario en {}",
            target_dir.display()
        )
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

    #[test]
    fn test_nix_package_for_java() {
        assert_eq!(nix_package_for_java(8), Some("nixpkgs#jdk8".to_string()));
        assert_eq!(nix_package_for_java(17), Some("nixpkgs#jdk17".to_string()));
        assert_eq!(nix_package_for_java(21), Some("nixpkgs#jdk21".to_string()));
        assert_eq!(nix_package_for_java(25), Some("nixpkgs#jdk25".to_string()));
        assert_eq!(nix_package_for_java(16), None);
    }

    #[test]
    fn test_java_runtime_commands() {
        let rt_direct = JavaRuntime::Direct(PathBuf::from("/usr/bin/java"));
        assert_eq!(rt_direct.display_cmd(), "/usr/bin/java");
        assert_eq!(rt_direct.direct_path(), Some(Path::new("/usr/bin/java")));

        let cmd_direct =
            rt_direct.build_std_command(&["-jar".to_string(), "server.jar".to_string()]);
        assert_eq!(cmd_direct.get_program(), "/usr/bin/java");

        let rt_nix = JavaRuntime::NixShell {
            package: "nixpkgs#jdk25".to_string(),
        };
        assert_eq!(rt_nix.display_cmd(), "nix shell nixpkgs#jdk25 -c java");
        assert_eq!(rt_nix.direct_path(), None);

        let cmd_nix = rt_nix.build_std_command(&["-Xmx4G".to_string(), "-jar".to_string()]);
        assert_eq!(cmd_nix.get_program(), "nix");
        let args: Vec<&std::ffi::OsStr> = cmd_nix.get_args().collect();
        assert!(args.contains(&std::ffi::OsStr::new("nixpkgs#jdk25")));
        assert!(args.contains(&std::ffi::OsStr::new("java")));
        assert!(args.contains(&std::ffi::OsStr::new("-Xmx4G")));
    }

    #[test]
    fn test_find_all_local_javas_safe_execution() {
        let paths = SystemPaths::resolve();
        let _ = find_all_local_javas(&paths);
    }

    #[tokio::test]
    #[ignore = "requires pre-installed Java or network access (skipped in NixOS build sandbox)"]
    async fn test_resolve_java_21() {
        let paths = SystemPaths::resolve();
        let java_runtime =
            resolve_java(21, &paths, &crate::core::progress::SilentProgressReporter).await;
        assert!(
            java_runtime.is_ok(),
            "Failed to resolve Java 21: {:?}",
            java_runtime.err()
        );
        let rt = java_runtime.unwrap();
        match rt {
            JavaRuntime::Direct(bin) => assert!(bin.exists()),
            JavaRuntime::NixShell { package } => assert!(package.contains("21")),
        }
    }
}
