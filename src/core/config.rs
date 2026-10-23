use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderType {
    Vanilla,
    Paper,
    Fabric,
    NeoForge,
    Forge,
}

impl std::fmt::Display for LoaderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoaderType::Vanilla => write!(f, "vanilla"),
            LoaderType::Paper => write!(f, "paper"),
            LoaderType::Fabric => write!(f, "fabric"),
            LoaderType::NeoForge => write!(f, "neoforge"),
            LoaderType::Forge => write!(f, "forge"),
        }
    }
}

impl LoaderType {
    pub fn addons_subfolder(&self) -> &'static str {
        match self {
            LoaderType::Paper => "plugins",
            LoaderType::Fabric
            | LoaderType::Vanilla
            | LoaderType::NeoForge
            | LoaderType::Forge => "mods",
        }
    }
}

impl std::str::FromStr for LoaderType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "vanilla" => Ok(LoaderType::Vanilla),
            "paper" => Ok(LoaderType::Paper),
            "fabric" => Ok(LoaderType::Fabric),
            "neoforge" => Ok(LoaderType::NeoForge),
            "forge" => Ok(LoaderType::Forge),
            other => anyhow::bail!(
                "Loader no soportado: '{}'. Opciones válidas: vanilla, paper, fabric, neoforge, forge",
                other
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceConfig {
    pub name: String,
    pub loader: LoaderType,
    pub mc_version: String,
    pub loader_version: Option<String>,
    pub server_jar: String,
    pub java_version: u8,
    pub java_path: Option<PathBuf>,
    pub ram_min: String,
    pub ram_max: String,
    pub port: u16,
    pub eula: bool,
    pub aikar_flags: bool,
    #[serde(default)]
    pub extra_jvm_args: Vec<String>,
    #[serde(default)]
    pub source_pack: Option<String>,
}

impl InstanceConfig {
    pub fn new(name: String, loader: LoaderType, mc_version: String) -> Self {
        let java_ver = crate::core::java::required_java_version(&mc_version);
        let jar_name = match loader {
            LoaderType::Vanilla => "server.jar".to_string(),
            LoaderType::Paper => format!("paper-{}.jar", mc_version),
            LoaderType::Fabric => format!("fabric-server-{}.jar", mc_version),
            LoaderType::NeoForge | LoaderType::Forge => "server.jar".to_string(),
        };

        Self {
            name,
            loader,
            mc_version,
            loader_version: None,
            server_jar: jar_name,
            java_version: java_ver,
            java_path: None,
            ram_min: "2G".to_string(),
            ram_max: "4G".to_string(),
            port: 25565,
            eula: true,
            aikar_flags: true,
            extra_jvm_args: Vec::new(),
            source_pack: None,
        }
    }

    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("No se pudo leer la configuración en {}", path.display()))?;
        let config: InstanceConfig = toml::from_str(&content)
            .with_context(|| format!("Error al parsear TOML en {}", path.display()))?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)
            .context("Error al serializar la configuración a formato TOML")?;
        std::fs::write(path, content).with_context(|| {
            format!("No se pudo escribir la configuración en {}", path.display())
        })?;
        Ok(())
    }

    pub fn jvm_args(&self, instance_dir: &Path) -> Vec<String> {
        let mut args = Vec::new();
        args.push(format!("-Xms{}", self.ram_min));
        args.push(format!("-Xmx{}", self.ram_max));

        if self.aikar_flags {
            args.extend(crate::core::java::aikar_g1gc_flags());
        }

        args.extend(self.extra_jvm_args.clone());

        // Detección de unix_args.txt para NeoForge / Forge
        if matches!(self.loader, LoaderType::NeoForge | LoaderType::Forge) {
            if let Some(unix_args) = find_unix_args_file(instance_dir) {
                let arg_str = if let Ok(rel) = unix_args.strip_prefix(instance_dir) {
                    format!("@{}", rel.display())
                } else {
                    format!("@{}", unix_args.display())
                };
                args.push(arg_str);
                args.push("nogui".to_string());
                return args;
            }
        }

        args.push("-jar".to_string());
        args.push(self.server_jar.clone());

        // Si es NeoForge o Forge y aún no existe unix_args.txt (primera ejecución),
        // ServerStarterJar requiere los flags del instalador para descargarlo e inicializarlo
        if matches!(self.loader, LoaderType::NeoForge | LoaderType::Forge) {
            let effective_loader_ver = self
                .loader_version
                .clone()
                .or_else(|| extract_loader_version_from_variables(instance_dir));

            if let Some(ref ver) = effective_loader_ver {
                args.push("--installer-force".to_string());
                args.push("--installer".to_string());
                if self.loader == LoaderType::Forge {
                    let forge_installer_url = format!(
                        "https://files.minecraftforge.net/maven/net/minecraftforge/forge/{}-{}/forge-{}-{}-installer.jar",
                        self.mc_version, ver, self.mc_version, ver
                    );
                    args.push(forge_installer_url);
                } else if self.mc_version == "1.20.1" {
                    let neo_1201_url = format!(
                        "https://maven.neoforged.net/releases/net/neoforged/forge/{}-{}/forge-{}-{}-installer.jar",
                        self.mc_version, ver, self.mc_version, ver
                    );
                    args.push(neo_1201_url);
                } else {
                    args.push(ver.clone());
                }
            }
        }

        args.push("nogui".to_string());
        args
    }
}

/// Extrae la versión del modloader desde un archivo `variables.txt` si existe en la instancia.
pub fn extract_loader_version_from_variables(instance_dir: &Path) -> Option<String> {
    let var_file = instance_dir.join("variables.txt");
    if let Ok(content) = std::fs::read_to_string(&var_file) {
        for line in content.lines() {
            let line = line.trim();
            if let Some((k, v)) = line.split_once('=') {
                if k.trim() == "MODLOADER_VERSION" {
                    let clean = v.trim().trim_matches('"').trim_matches('\'').to_string();
                    if !clean.is_empty() {
                        return Some(clean);
                    }
                }
            }
        }
    }
    None
}

/// Localiza el archivo `unix_args.txt` generado comúnmente por instaladores de NeoForge y Forge.
pub fn find_unix_args_file(instance_dir: &Path) -> Option<PathBuf> {
    // 1. Directo en la raíz de la instancia
    let root_args = instance_dir.join("unix_args.txt");
    if root_args.is_file() {
        return Some(root_args);
    }

    // 2. En libraries/net/neoforged/neoforge/
    let neoforge_dir = instance_dir
        .join("libraries")
        .join("net")
        .join("neoforged")
        .join("neoforge");
    if neoforge_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&neoforge_dir) {
            for entry in entries.flatten() {
                let candidate = entry.path().join("unix_args.txt");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    // 3. En libraries/net/minecraftforge/forge/
    let forge_dir = instance_dir
        .join("libraries")
        .join("net")
        .join("minecraftforge")
        .join("forge");
    if forge_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&forge_dir) {
            for entry in entries.flatten() {
                let candidate = entry.path().join("unix_args.txt");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    // 4. Búsqueda recursiva poco profunda en libraries/
    let lib_dir = instance_dir.join("libraries");
    if lib_dir.is_dir() {
        return search_file_recursive(&lib_dir, "unix_args.txt", 4);
    }

    None
}

fn search_file_recursive(dir: &Path, target: &str, depth: usize) -> Option<PathBuf> {
    if depth == 0 {
        return None;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.file_name().and_then(|n| n.to_str()) == Some(target) {
                return Some(path);
            } else if path.is_dir() {
                if let Some(found) = search_file_recursive(&path, target, depth - 1) {
                    return Some(found);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loader_type_parsing() {
        assert_eq!(
            "vanilla".parse::<LoaderType>().unwrap(),
            LoaderType::Vanilla
        );
        assert_eq!("paper".parse::<LoaderType>().unwrap(), LoaderType::Paper);
        assert_eq!("fabric".parse::<LoaderType>().unwrap(), LoaderType::Fabric);
        assert_eq!(
            "neoforge".parse::<LoaderType>().unwrap(),
            LoaderType::NeoForge
        );
        assert_eq!("forge".parse::<LoaderType>().unwrap(), LoaderType::Forge);
        assert!("unknown_loader".parse::<LoaderType>().is_err());

        assert_eq!(LoaderType::Paper.addons_subfolder(), "plugins");
        assert_eq!(LoaderType::Fabric.addons_subfolder(), "mods");
        assert_eq!(LoaderType::Vanilla.addons_subfolder(), "mods");
        assert_eq!(LoaderType::NeoForge.addons_subfolder(), "mods");
        assert_eq!(LoaderType::Forge.addons_subfolder(), "mods");
    }

    #[test]
    fn test_instance_config_defaults() {
        let cfg = InstanceConfig::new(
            "survival".to_string(),
            LoaderType::Paper,
            "1.21.4".to_string(),
        );
        assert_eq!(cfg.name, "survival");
        assert_eq!(cfg.loader, LoaderType::Paper);
        assert_eq!(cfg.server_jar, "paper-1.21.4.jar");
        assert_eq!(cfg.java_version, 21);
        let args = cfg.jvm_args(Path::new("."));
        assert!(args.contains(&"-jar".to_string()));
        assert!(args.contains(&"paper-1.21.4.jar".to_string()));
    }

    #[test]
    fn test_neoforge_unix_args_detection() {
        let temp_dir =
            std::env::temp_dir().join(format!("mcserman_cfg_test_{}", std::process::id()));
        let neoforge_target = temp_dir
            .join("libraries")
            .join("net")
            .join("neoforged")
            .join("neoforge")
            .join("21.1.250");
        std::fs::create_dir_all(&neoforge_target).unwrap();
        let args_file = neoforge_target.join("unix_args.txt");
        std::fs::write(&args_file, "--launchTarget neoforge_server\n").unwrap();

        let cfg = InstanceConfig::new(
            "neoforge_srv".to_string(),
            LoaderType::NeoForge,
            "1.21.1".to_string(),
        );

        let detected = find_unix_args_file(&temp_dir);
        assert_eq!(detected, Some(args_file));

        let jvm_args = cfg.jvm_args(&temp_dir);
        assert!(!jvm_args.contains(&"-jar".to_string()));
        assert!(jvm_args.iter().any(|a| a
            .starts_with("@libraries/net/neoforged/neoforge/21.1.250/unix_args.txt")));
        assert!(jvm_args.contains(&"nogui".to_string()));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_neoforge_installer_args_when_unix_args_absent() {
        let temp_dir = std::env::temp_dir()
            .join(format!("mcserman_cfg_installer_test_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let mut cfg = InstanceConfig::new(
            "neoforge_srv".to_string(),
            LoaderType::NeoForge,
            "1.21.1".to_string(),
        );
        cfg.loader_version = Some("21.1.250".to_string());

        let jvm_args = cfg.jvm_args(&temp_dir);
        assert!(jvm_args.contains(&"-jar".to_string()));
        assert!(jvm_args.contains(&"server.jar".to_string()));
        assert!(jvm_args.contains(&"--installer-force".to_string()));
        assert!(jvm_args.contains(&"--installer".to_string()));
        assert!(jvm_args.contains(&"21.1.250".to_string()));
        assert!(jvm_args.contains(&"nogui".to_string()));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
