use std::path::{Path, PathBuf};
use directories::ProjectDirs;

#[derive(Debug, Clone)]
pub struct SystemPaths {
    pub base_dir: PathBuf,
    pub instances_dir: PathBuf,
    pub runtimes_dir: PathBuf,
    pub socket_dir: PathBuf,
}

impl SystemPaths {
    pub fn resolve() -> Self {
        // Si MCSERVER_DATA_DIR está definido, tiene máxima prioridad
        if let Ok(custom_dir) = std::env::var("MCSERVER_DATA_DIR") {
            let base = PathBuf::from(custom_dir);
            let sock = std::env::var("MCSERVER_RUNTIME_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| base.join("run"));
            return Self {
                instances_dir: base.join("instances"),
                runtimes_dir: base.join("runtimes"),
                socket_dir: sock,
                base_dir: base,
            };
        }

        // Si corre como root o /var/lib/mcserver existe y es accesible
        let var_lib = Path::new("/var/lib/mcserver");
        if var_lib.exists() && var_lib.is_dir() {
            let sock_dir = PathBuf::from("/run/mcserver");
            return Self {
                base_dir: var_lib.to_path_buf(),
                instances_dir: var_lib.join("instances"),
                runtimes_dir: var_lib.join("runtimes"),
                socket_dir: sock_dir,
            };
        }

        // Fallback a directorios XDG de usuario estándar
        if let Some(proj_dirs) = ProjectDirs::from("org", "mcserman", "mcserman") {
            let base = proj_dirs.data_local_dir().to_path_buf();
            let sock_dir = std::env::var("XDG_RUNTIME_DIR")
                .map(|p| PathBuf::from(p).join("mcserver"))
                .unwrap_or_else(|_| base.join("run"));

            Self {
                instances_dir: base.join("instances"),
                runtimes_dir: base.join("runtimes"),
                socket_dir: sock_dir,
                base_dir: base,
            }
        } else {
            // Fallback extremo
            let fallback = PathBuf::from("./mcserver_data");
            Self {
                instances_dir: fallback.join("instances"),
                runtimes_dir: fallback.join("runtimes"),
                socket_dir: fallback.join("run"),
                base_dir: fallback,
            }
        }
    }

    pub fn ensure_dirs(&self) -> anyhow::Result<()> {
        let check = |p: &Path| -> anyhow::Result<()> {
            std::fs::create_dir_all(p).map_err(|e| {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    anyhow::anyhow!(
                        "Permiso denegado al acceder al directorio '{}'.\n\
                         💡 Si acabas de habilitar el módulo en NixOS, ejecuta 'newgrp mcserver' o abre una nueva terminal para refrescar tus permisos de grupo.",
                        p.display()
                    )
                } else {
                    anyhow::anyhow!("Error al preparar el directorio '{}': {}", p.display(), e)
                }
            })
        };
        check(&self.base_dir)?;
        check(&self.instances_dir)?;
        check(&self.runtimes_dir)?;
        check(&self.socket_dir)?;
        Ok(())
    }

    pub fn instance_dir(&self, name: &str) -> PathBuf {
        self.instances_dir.join(name)
    }

    pub fn instance_config(&self, name: &str) -> PathBuf {
        self.instance_dir(name).join("instance.toml")
    }

    pub fn socket_path(&self, name: &str) -> PathBuf {
        self.socket_dir.join(format!("{}.sock", name))
    }

    pub fn logs_file(&self, name: &str) -> PathBuf {
        self.instance_dir(name).join("logs").join("latest.log")
    }
}
