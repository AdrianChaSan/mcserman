use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderType {
    Vanilla,
    Paper,
    Fabric,
}

impl std::fmt::Display for LoaderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoaderType::Vanilla => write!(f, "vanilla"),
            LoaderType::Paper => write!(f, "paper"),
            LoaderType::Fabric => write!(f, "fabric"),
        }
    }
}

impl LoaderType {
    pub fn addons_subfolder(&self) -> &'static str {
        match self {
            LoaderType::Paper => "plugins",
            LoaderType::Fabric | LoaderType::Vanilla => "mods",
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
            other => anyhow::bail!(
                "Loader no soportado: '{}'. Opciones válidas: vanilla, paper, fabric",
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

    pub fn jvm_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        args.push(format!("-Xms{}", self.ram_min));
        args.push(format!("-Xmx{}", self.ram_max));

        if self.aikar_flags {
            args.extend(crate::core::java::aikar_g1gc_flags());
        }

        args.extend(self.extra_jvm_args.clone());
        args.push("-jar".to_string());
        args.push(self.server_jar.clone());
        args.push("nogui".to_string());
        args
    }
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
        assert!("forge".parse::<LoaderType>().is_err());

        assert_eq!(LoaderType::Paper.addons_subfolder(), "plugins");
        assert_eq!(LoaderType::Fabric.addons_subfolder(), "mods");
        assert_eq!(LoaderType::Vanilla.addons_subfolder(), "mods");
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
        assert!(cfg.jvm_args().contains(&"-jar".to_string()));
        assert!(cfg.jvm_args().contains(&"paper-1.21.4.jar".to_string()));
    }
}
