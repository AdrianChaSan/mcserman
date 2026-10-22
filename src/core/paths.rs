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

    #[allow(dead_code)]
    pub fn list_instances(&self) -> Vec<String> {
        let mut list = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.instances_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() && entry.path().join("instance.toml").exists() {
                    if let Some(name) = entry.file_name().to_str() {
                        list.push(name.to_string());
                    }
                }
            }
        }
        list.sort();
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_instances() {
        let unique_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("mcserman_paths_test_{}", unique_id));
        let paths = SystemPaths {
            instances_dir: temp_dir.join("instances"),
            runtimes_dir: temp_dir.join("runtimes"),
            socket_dir: temp_dir.join("run"),
            base_dir: temp_dir.clone(),
        };
        paths.ensure_dirs().unwrap();

        assert_eq!(paths.list_instances(), Vec::<String>::new());

        let inst1 = paths.instance_dir("server_b");
        let inst2 = paths.instance_dir("server_a");
        std::fs::create_dir_all(&inst1).unwrap();
        std::fs::create_dir_all(&inst2).unwrap();
        std::fs::write(inst1.join("instance.toml"), "").unwrap();
        std::fs::write(inst2.join("instance.toml"), "").unwrap();

        assert_eq!(paths.list_instances(), vec!["server_a", "server_b"]);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
