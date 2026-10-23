use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::timeout;

use crate::core::config::InstanceConfig;
use crate::core::paths::SystemPaths;
use crate::daemon::socket::{SocketRequest, SocketResponse};

/// Lanza el subproceso daemon de mcserman completamente desacoplado de la terminal actual y
/// previene la acumulación de procesos zombie (<defunct>) mediante un reaper asíncrono.
///
/// 1. `Stdio::null()` para no interferir ni corromper el Alternate Screen de la TUI o terminal.
/// 2. `setsid()` en sistemas Unix para que sea líder de sesión independiente (sobrevive
///    si la terminal o TUI se cierra y desvincula la señal SIGHUP de la terminal).
/// 3. Reaper en `tokio::spawn` ejecutando `.wait().await` para recolectar el código de salida
///    cuando el daemon termine mientras la TUI o el CLI sigan en ejecución.
pub fn spawn_daemon_detached(instance_name: &str) -> Result<()> {
    let current_exe =
        std::env::current_exe().context("No se pudo obtener la ruta del ejecutable")?;
    let mut cmd = Command::new(current_exe);
    cmd.arg("daemon")
        .arg(instance_name)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.as_std_mut().pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }

    let mut child = cmd
        .spawn()
        .with_context(|| format!("Error al lanzar el daemon de '{}'", instance_name))?;

    tokio::spawn(async move {
        let _ = child.wait().await;
    });

    Ok(())
}

pub async fn run_daemon(instance_name: &str, paths: &SystemPaths) -> Result<()> {
    let instance_dir = paths.instance_dir(instance_name);
    let config_path = paths.instance_config(instance_name);

    if !config_path.exists() {
        bail!(
            "La instancia '{}' no existe en {}",
            instance_name,
            instance_dir.display()
        );
    }

    let config = InstanceConfig::load(&config_path)?;
    println!(
        "{}",
        format!(
            "🚀 Iniciando daemon para instancia '{}' (Minecraft {} via {})...",
            config.name, config.mc_version, config.loader
        )
        .green()
        .bold()
    );

    // 1. Asegurar EULA
    let eula_path = instance_dir.join("eula.txt");
    if !eula_path.exists()
        || !std::fs::read_to_string(&eula_path)
            .unwrap_or_default()
            .contains("eula=true")
    {
        std::fs::write(&eula_path, "eula=true\n")
            .context("No se pudo escribir eula.txt con eula=true")?;
    }

    // 2. Preparar socket y PID antes de cualquier tarea bloqueante
    paths.ensure_dirs()?;
    let pid_path = paths.pid_file(instance_name);
    let _ = std::fs::write(&pid_path, format!("{}\n", std::process::id()));

    let socket_path = paths.socket_path(instance_name);
    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

    let listener = UnixListener::bind(&socket_path).with_context(|| {
        format!(
            "No se pudo enlazar el Unix Socket en {}",
            socket_path.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&socket_path)?.permissions();
        perms.set_mode(0o660); // Lectura/escritura para dueño y grupo
        let _ = std::fs::set_permissions(&socket_path, perms);
    }

    // El socket responde inmediatamente a ping y status (Starting)
    let child_stdin: Arc<Mutex<Option<tokio::process::ChildStdin>>> = Arc::new(Mutex::new(None));
    let child_pid_atomic = Arc::new(std::sync::atomic::AtomicU32::new(0));

    let stdin_for_socket = Arc::clone(&child_stdin);
    let pid_for_socket = Arc::clone(&child_pid_atomic);
    let socket_task = tokio::spawn(async move {
        while let Ok((mut stream, _addr)) = listener.accept().await {
            let stdin_ref = Arc::clone(&stdin_for_socket);
            let pid_ref = Arc::clone(&pid_for_socket);
            tokio::spawn(async move {
                let mut reader = BufReader::new(&mut stream);
                let mut line = String::new();
                if reader.read_line(&mut line).await.is_ok() {
                    let resp = match serde_json::from_str::<SocketRequest>(&line) {
                        Ok(SocketRequest::Command(cmd)) => {
                            let mut lock = stdin_ref.lock().await;
                            if let Some(ref mut stdin) = *lock {
                                let full_cmd = format!("{}\n", cmd);
                                if stdin.write_all(full_cmd.as_bytes()).await.is_ok()
                                    && stdin.flush().await.is_ok()
                                {
                                    SocketResponse::Ok(format!("Comando '{}' enviado", cmd))
                                } else {
                                    SocketResponse::Error(
                                        "Fallo al escribir en stdin de Java".to_string(),
                                    )
                                }
                            } else {
                                SocketResponse::Error(
                                    "El servidor aún está arrancando...".to_string(),
                                )
                            }
                        }
                        Ok(SocketRequest::Stop) => {
                            let mut lock = stdin_ref.lock().await;
                            if let Some(ref mut stdin) = *lock {
                                let _ = stdin.write_all(b"stop\n").await;
                                let _ = stdin.flush().await;
                                SocketResponse::Ok(
                                    "Orden de apagado limpio 'stop' enviada".to_string(),
                                )
                            } else {
                                SocketResponse::Ok(
                                    "El servidor se estaba iniciando. Deteniendo.".to_string(),
                                )
                            }
                        }
                        Ok(SocketRequest::Ping) => SocketResponse::Ok("pong".to_string()),
                        Ok(SocketRequest::Status) => {
                            let pid = pid_ref.load(std::sync::atomic::Ordering::Relaxed);
                            if pid > 0 {
                                SocketResponse::Ok(format!("Running (PID {})", pid))
                            } else {
                                SocketResponse::Ok("Starting".to_string())
                            }
                        }
                        Err(e) => SocketResponse::Error(format!("Petición inválida: {}", e)),
                    };

                    let resp_json = serde_json::to_string(&resp).unwrap_or_default() + "\n";
                    let _ = stream.write_all(resp_json.as_bytes()).await;
                }
            });
        }
    });

    // 3. Resolver Java
    let java_runtime = if let Some(ref custom_java) = config.java_path {
        crate::core::java::JavaRuntime::Direct(custom_java.clone())
    } else {
        crate::core::java::resolve_java(
            config.java_version,
            paths,
            &crate::core::progress::SilentProgressReporter,
        )
        .await?
    };

    // 4. Preparar archivo de log
    let logs_dir = instance_dir.join("logs");
    std::fs::create_dir_all(&logs_dir)?;
    let latest_log_path = logs_dir.join("latest.log");
    let mut init_file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&latest_log_path)
        .await?;

    let start_msg = format!(
        "[mcserman] Iniciando instancia '{}' (Minecraft {} via {})...\n",
        config.name, config.mc_version, config.loader
    );
    let _ = init_file.write_all(start_msg.as_bytes()).await;
    let _ = init_file.flush().await;
    let log_file = Arc::new(Mutex::new(init_file));

    // 5. Lanzar proceso Java
    let jvm_args = config.jvm_args(&instance_dir);
    println!(
        "☕ Ejecutando: {} {}",
        java_runtime.display_cmd(),
        jvm_args.join(" ")
    );

    let mut child = java_runtime
        .build_tokio_command(&jvm_args)
        .current_dir(&instance_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| {
            format!(
                "Fallo al arrancar el proceso JVM con {}",
                java_runtime.display_cmd()
            )
        })?;

    let child_pid = child.id().unwrap_or(0);
    child_pid_atomic.store(child_pid, std::sync::atomic::Ordering::Relaxed);
    let _ = std::fs::write(
        &pid_path,
        format!("{}\n{}\n", std::process::id(), child_pid),
    );
    println!(
        "{}",
        format!("✓ Proceso Java en ejecución con PID {}", child_pid).green()
    );

    let raw_stdin = child
        .stdin
        .take()
        .context("No se pudo capturar stdin de Java")?;
    *child_stdin.lock().await = Some(raw_stdin);

    // Canalizar stdout
    let child_stdout = child
        .stdout
        .take()
        .context("No se pudo capturar stdout de Java")?;
    let log_file_clone1 = Arc::clone(&log_file);
    tokio::spawn(async move {
        let mut reader = BufReader::new(child_stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let clean = crate::tui::app::sanitize_log_line(&line);
            let trimmed = clean.trim_end();
            if trimmed.is_empty() && !line.trim().is_empty() {
                continue;
            }
            let mut out = trimmed.to_string();
            out.push('\n');
            let mut file = log_file_clone1.lock().await;
            let _ = file.write_all(out.as_bytes()).await;
            let _ = file.flush().await;
            // Imprimir al stdout del daemon para systemd journalctl
            println!("{}", trimmed);
        }
    });

    // Canalizar stderr
    let child_stderr = child
        .stderr
        .take()
        .context("No se pudo capturar stderr de Java")?;
    let log_file_clone2 = Arc::clone(&log_file);
    tokio::spawn(async move {
        let mut reader = BufReader::new(child_stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let clean = crate::tui::app::sanitize_log_line(&line);
            let trimmed = clean.trim_end();
            let out = format!("[STDERR] {}\n", trimmed);
            let mut file = log_file_clone2.lock().await;
            let _ = file.write_all(out.as_bytes()).await;
            let _ = file.flush().await;
            eprintln!("{}", trimmed);
        }
    });

    // 6. Señales de apagado
    #[cfg(unix)]
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    #[cfg(unix)]
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;

    tokio::select! {
        _ = child.wait() => {
            println!("{}", "El proceso de Minecraft ha salido por sí mismo.".yellow());
        }
        _ = async {
            #[cfg(unix)]
            tokio::select! {
                _ = sigterm.recv() => "SIGTERM",
                _ = sigint.recv() => "SIGINT",
            }
            #[cfg(not(unix))]
            tokio::signal::ctrl_c().await.map(|_| "CTRL_C").unwrap_or("EXIT")
        } => {
            println!("{}", "Señal de apagado recibida. Enviando 'stop' a Minecraft para guardar mapa...".yellow().bold());
            {
                let mut lock = child_stdin.lock().await;
                if let Some(ref mut stdin) = *lock {
                    let _ = stdin.write_all(b"stop\n").await;
                    let _ = stdin.flush().await;
                }
            }

            // Esperar apagado limpio con timeout de 60 segundos
            match timeout(Duration::from_secs(60), child.wait()).await {
                Ok(Ok(status)) => {
                    println!("{}", format!("✓ Servidor detenido limpiamente con código: {:?}", status.code()).green());
                }
                _ => {
                    eprintln!("{}", "Timeout de 60s excedido esperando 'stop'. Forzando kill...".red().bold());
                    let _ = child.kill().await;
                }
            }
        }
    }

    socket_task.abort();
    let _ = std::fs::remove_file(&socket_path);
    let _ = std::fs::remove_file(&pid_path);
    println!("{}", "Daemon finalizado. Socket y PID cerrados.".cyan());
    Ok(())
}
