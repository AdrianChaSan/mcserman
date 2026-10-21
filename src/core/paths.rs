use directories::ProjectDirs;
use std::path::{Path, PathBuf};

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
            let run_mcserver = Path::new("/run/mcserver");
            let sock_dir = if run_mcserver.exists() || std::fs::create_dir_all(run_mcserver).is_ok()
            {
                run_mcserver.to_path_buf()
            } else if let Ok(xdg_runtime) = std::env::var("XDG_RUNTIME_DIR") {
                PathBuf::from(xdg_runtime).join("mcserver")
            } else {
                var_lib.join("run")
            };

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

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.base_dir)?;
        std::fs::create_dir_all(&self.instances_dir)?;
        std::fs::create_dir_all(&self.runtimes_dir)?;
        std::fs::create_dir_all(&self.socket_dir)?;
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

    pub fn pid_file(&self, name: &str) -> PathBuf {
        self.socket_dir.join(format!("{}.pid", name))
    }

    pub fn logs_file(&self, name: &str) -> PathBuf {
        self.instance_dir(name).join("logs").join("latest.log")
    }
}
