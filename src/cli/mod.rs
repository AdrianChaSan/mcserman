pub mod args;
pub mod prompts;

use std::process::Stdio;
use anyhow::{bail, Context, Result};
use colored::Colorize;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};

use crate::core::config::{InstanceConfig, LoaderType};
use crate::core::paths::SystemPaths;
use crate::core::properties::ServerProperties;
use crate::daemon::socket::{is_daemon_running, send_request, SocketRequest, SocketResponse};
use crate::loaders::download_server_jar;
use crate::modrinth::api::ModrinthClient;

pub fn get_existing_instances(paths: &SystemPaths) -> Vec<String> {
    let mut list = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&paths.instances_dir) {
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

fn resolve_target_instance(name: Option<String>, paths: &SystemPaths, prompt_msg: &str) -> Result<String> {
    let instances = get_existing_instances(paths);
    match name {
        Some(n) => {
            if !instances.contains(&n) {
                bail!("La instancia '{}' no existe en {}", n, paths.instances_dir.display());
            }
            Ok(n)
        }
        None => prompts::select_instance(&instances, prompt_msg),
    }
}

pub async fn handle_create(
    name: Option<String>,
    loader: Option<String>,
    mc_version: Option<String>,
    ram: Option<String>,
    port: Option<u16>,
    paths: &SystemPaths,
) -> Result<()> {
    println!("{}", "🛠️  Creando nueva instancia de Minecraft con mcserman...".cyan().bold());

    // 1. Nombre de la instancia
    let final_name = prompts::prompt_string(name, "¿Nombre del servidor / instancia?", "survival")?;
    let instance_dir = paths.instance_dir(&final_name);
    let config_path = paths.instance_config(&final_name);

    if config_path.exists() {
        bail!("Ya existe una instancia configurada con el nombre '{}' en {}", final_name, instance_dir.display());
    } else if instance_dir.exists() {
        let is_empty = std::fs::read_dir(&instance_dir)
            .map(|mut r| r.next().is_none())
            .unwrap_or(false);

        if !is_empty {
            bail!("El directorio '{}' ya existe y contiene archivos. Elige otro nombre.", instance_dir.display());
        }
    }

    // 2. Modloader y versión
    let final_loader = prompts::prompt_loader(loader)?;
    let final_mc_ver = prompts::prompt_mc_version(mc_version)?;
    let final_ram = prompts::prompt_string(ram, "Memoria RAM asignada (ej. 2G, 4G, 8G):", "4G")?;
    let final_port = port.unwrap_or(25565);

    // 3. Crear estructura de carpetas
    std::fs::create_dir_all(&instance_dir)?;
    let mut config = InstanceConfig::new(final_name.clone(), final_loader, final_mc_ver.clone());
    config.ram_max = final_ram.clone();
    config.port = final_port;

    // 4. Descargar el server jar correspondiente y configurar archivos
    let target_jar = instance_dir.join(&config.server_jar);
    let setup_result = async {
        let loader_ver = download_server_jar(final_loader, &final_mc_ver, &target_jar).await?;
        config.loader_version = loader_ver;

        let props_path = instance_dir.join("server.properties");
        let mut props = ServerProperties::load_or_default(&props_path);
        props.set("server-port", &final_port.to_string());
        props.set("motd", &format!("§aServidor §b{} §7(§e{} §6{}§7)", final_name, final_loader, final_mc_ver));
        props.save(&props_path)?;

        std::fs::write(instance_dir.join("eula.txt"), "eula=true\n")?;
        config.save(&config_path)?;
        Ok::<(), anyhow::Error>(())
    }.await;

    if let Err(e) = setup_result {
        if !config_path.exists() {
            let _ = std::fs::remove_dir_all(&instance_dir);
        }
        return Err(e);
    }

    println!("{}", format!("\n✨ ¡Instancia '{}' creada con éxito!", final_name).green().bold());
    println!("   Directorio: {}", instance_dir.display().to_string().cyan());
    println!("   Loader:     {} (Minecraft {})", final_loader, final_mc_ver);
    println!("   RAM:        {}", final_ram);
    println!("   Puerto:     {}", final_port);
    println!("\nIníciala con: {}", format!("mcserman start {}", final_name).yellow().bold());

    Ok(())
}

pub async fn handle_list(paths: &SystemPaths) -> Result<()> {
    let instances = get_existing_instances(paths);
    if instances.is_empty() {
        println!("{}", "No hay instancias creadas. Usa 'mcserman create' para crear una.".yellow());
        return Ok(());
    }

    println!("{}", "\n📋 Instancias de Minecraft registradas:".cyan().bold());
    println!("{:<20} {:<10} {:<10} {:<10} {:<12}", "NOMBRE", "LOADER", "MC VER", "PUERTO", "ESTADO");
    println!("{}", "-".repeat(65));

    for name in instances {
        let config_path = paths.instance_config(&name);
        let socket_path = paths.socket_path(&name);
        let is_running = is_daemon_running(&socket_path).await;

        if let Ok(cfg) = InstanceConfig::load(&config_path) {
            let status_str = if is_running {
                "● ONLINE".green().bold()
            } else {
                "○ OFFLINE".dimmed()
            };
            println!("{:<20} {:<10} {:<10} {:<10} {:<12}",
                cfg.name.bold(),
                cfg.loader.to_string(),
                cfg.mc_version,
                cfg.port,
                status_str
            );
        }
    }
    println!();
    Ok(())
}

pub async fn handle_start(name: Option<String>, paths: &SystemPaths) -> Result<()> {
    let selected = resolve_target_instance(name, paths, "Selecciona la instancia a iniciar:")?;
    let socket_path = paths.socket_path(&selected);

    if is_daemon_running(&socket_path).await {
        println!("{}", format!("⚠️  La instancia '{}' ya está en ejecución.", selected).yellow());
        return Ok(());
    }

    println!("{}", format!("🚀 Iniciando servidor '{}' en segundo plano...", selected).cyan());

    let current_exe = std::env::current_exe().context("No se pudo obtener la ruta del ejecutable")?;

    let _child = std::process::Command::new(current_exe)
        .arg("daemon")
        .arg(&selected)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("Error al lanzar el daemon de '{}'", selected))?;

    print!("Conectando con el supervisor");
    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        print!(".");
        let _ = std::io::Write::flush(&mut std::io::stdout());
        if is_daemon_running(&socket_path).await {
            println!("\n{}", format!("✓ Servidor '{}' iniciado correctamente.", selected).green().bold());
            println!("   Para ver la consola en vivo: {}", format!("mcserman logs {} -f", selected).cyan());
            println!("   Para interactuar:             {}", format!("mcserman console {}", selected).cyan());
            return Ok(());
        }
    }

    println!("\n{}", "⚠️  El daemon fue lanzado pero el socket aún no responde. Revisa logs con:".yellow());
    println!("   mcserman logs {}", selected);
    Ok(())
}

pub async fn handle_stop(name: Option<String>, paths: &SystemPaths) -> Result<()> {
    let selected = resolve_target_instance(name, paths, "Selecciona la instancia a detener:")?;
    let socket_path = paths.socket_path(&selected);

    if !is_daemon_running(&socket_path).await {
        println!("{}", format!("ℹ️  La instancia '{}' no está en ejecución.", selected).yellow());
        return Ok(());
    }

    println!("{}", format!("🛑 Enviando orden de guardado y parada a '{}'...", selected).cyan());
    match send_request(&socket_path, &SocketRequest::Stop).await {
        Ok(SocketResponse::Ok(msg)) => {
            println!("{}", format!("✓ {}", msg).green());
        }
        Ok(SocketResponse::Error(err)) => {
            eprintln!("{}", format!("Error: {}", err).red());
        }
        Err(e) => {
            eprintln!("{}", format!("Fallo al comunicar con socket: {}", e).red());
        }
    }
    Ok(())
}

pub async fn handle_restart(name: Option<String>, paths: &SystemPaths) -> Result<()> {
    let selected = resolve_target_instance(name, paths, "Selecciona la instancia a reiniciar:")?;
    handle_stop(Some(selected.clone()), paths).await?;

    println!("Esperando apagado completo...");
    let socket_path = paths.socket_path(&selected);
    for _ in 0..40 {
        if !is_daemon_running(&socket_path).await {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    handle_start(Some(selected), paths).await?;
    Ok(())
}

pub async fn handle_status(name: Option<String>, paths: &SystemPaths) -> Result<()> {
    let selected = resolve_target_instance(name, paths, "Selecciona la instancia:")?;
    let socket_path = paths.socket_path(&selected);
    let config_path = paths.instance_config(&selected);
    let config = InstanceConfig::load(&config_path)?;

    println!("{}", format!("\n📊 Estado de la instancia '{}':", selected).cyan().bold());
    println!("   Versión:   {} ({})", config.mc_version, config.loader);
    println!("   Puerto:    {}", config.port);
    println!("   RAM:       {}", config.ram_max);

    if is_daemon_running(&socket_path).await {
        println!("   Estado:    {}", "● ONLINE".green().bold());
        if let Ok(SocketResponse::Ok(status_info)) = send_request(&socket_path, &SocketRequest::Status).await {
            println!("   Supervisor: {}", status_info.green());
        }
    } else {
        println!("   Estado:    {}", "○ OFFLINE".dimmed());
    }
    println!();
    Ok(())
}

pub async fn handle_cmd(name: &str, command: &str, paths: &SystemPaths) -> Result<()> {
    let socket_path = paths.socket_path(name);
    if !is_daemon_running(&socket_path).await {
        bail!("La instancia '{}' no está encendida.", name);
    }

    match send_request(&socket_path, &SocketRequest::Command(command.to_string())).await? {
        SocketResponse::Ok(msg) => println!("{}", format!("✓ {}", msg).green()),
        SocketResponse::Error(err) => eprintln!("{}", format!("✗ Error: {}", err).red()),
    }
    Ok(())
}

pub async fn handle_logs(name: Option<String>, follow: bool, lines_count: usize, paths: &SystemPaths) -> Result<()> {
    let selected = resolve_target_instance(name, paths, "Selecciona la instancia para ver logs:")?;
    let log_path = paths.logs_file(&selected);

    if !log_path.exists() {
        println!("{}", format!("No hay archivo de logs para '{}' todavía.", selected).yellow());
        return Ok(());
    }

    let file = tokio::fs::File::open(&log_path).await?;
    let mut reader = BufReader::new(file).lines();
    let mut buffer = Vec::new();

    while let Ok(Some(line)) = reader.next_line().await {
        buffer.push(line);
    }

    let start_idx = if buffer.len() > lines_count {
        buffer.len() - lines_count
    } else {
        0
    };

    for line in &buffer[start_idx..] {
        print_colored_log(line);
    }

    if follow {
        println!("{}", "--- Transmitiendo logs en tiempo real (Ctrl+C para salir) ---".dimmed());
        let mut file = tokio::fs::File::open(&log_path).await?;
        let _ = file.seek(std::io::SeekFrom::End(0)).await;
        let mut stream_reader = BufReader::new(file).lines();

        loop {
            tokio::select! {
                line_res = stream_reader.next_line() => {
                    match line_res {
                        Ok(Some(line)) => print_colored_log(&line),
                        Ok(None) => tokio::time::sleep(std::time::Duration::from_millis(150)).await,
                        Err(_) => break,
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    println!("\nDeteniendo visualización de logs.");
                    break;
                }
            }
        }
    }

    Ok(())
}

pub async fn handle_console(name: Option<String>, paths: &SystemPaths) -> Result<()> {
    let selected = resolve_target_instance(name, paths, "Selecciona la instancia para abrir consola:")?;
    let socket_path = paths.socket_path(&selected);

    if !is_daemon_running(&socket_path).await {
        bail!("La instancia '{}' no está encendida. Iníciala primero.", selected);
    }

    println!("{}", format!("🖥️  Consola interactiva de '{}'. Escribe comandos o ':q' para desconectar.", selected).cyan().bold());

    let sock_clone = socket_path.clone();
    let log_path = paths.logs_file(&selected);

    // Tarea de streaming de logs
    let log_task = tokio::spawn(async move {
        if let Ok(mut file) = tokio::fs::File::open(&log_path).await {
            let _ = file.seek(std::io::SeekFrom::End(0)).await;
            let mut reader = BufReader::new(file).lines();
            loop {
                match reader.next_line().await {
                    Ok(Some(line)) => print_colored_log(&line),
                    Ok(None) => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
                    Err(_) => break,
                }
            }
        }
    });

    // Entrada interactiva por teclado
    let mut stdin_reader = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = stdin_reader.next_line().await {
        let trimmed = line.trim();
        if trimmed == ":q" || trimmed == "exit" {
            println!("Desconectando consola interactiva (el servidor sigue corriendo en segundo plano).");
            break;
        }
        if !trimmed.is_empty() {
            let _ = send_request(&sock_clone, &SocketRequest::Command(trimmed.to_string())).await;
        }
    }

    log_task.abort();
    Ok(())
}

pub async fn handle_add_mod(name: Option<String>, query: Option<String>, paths: &SystemPaths) -> Result<()> {
    let selected = resolve_target_instance(name, paths, "Selecciona la instancia para instalar mods/plugins:")?;
    let config_path = paths.instance_config(&selected);
    let config = InstanceConfig::load(&config_path)?;

    let search_query = prompts::prompt_string(query, "Nombre o palabra clave del mod/plugin a buscar en Modrinth:", "lithium")?;
    let loader_str = config.loader.to_string();

    println!("{}", format!("🔍 Buscando '{}' en Modrinth para {} {}...", search_query, loader_str, config.mc_version).cyan());
    let client = ModrinthClient::new();
    let hits = client.search(&search_query, &loader_str, &config.mc_version).await?;

    if hits.is_empty() {
        println!("{}", "No se encontraron resultados compatibles en Modrinth.".yellow());
        return Ok(());
    }

    let display_options: Vec<String> = hits.iter().map(|h| {
        format!("{} - {}", h.title.bold(), h.description)
    }).collect();

    let choice = prompts::handle_inquire_cancel(
        inquire::Select::new("Selecciona el mod/plugin a instalar:", display_options).prompt()
    )?;

    let selected_hit = hits.into_iter().find(|h| choice.contains(&h.title))
        .with_context(|| "Opción seleccionada inválida")?;

    let version_info = client.get_compatible_version(&selected_hit.project_id, &loader_str, &config.mc_version).await?;
    let primary_file = version_info.files.iter().find(|f| f.primary).or_else(|| version_info.files.first())
        .with_context(|| "La versión seleccionada no contiene archivos descargables")?;

    let target_folder = match config.loader {
        LoaderType::Paper => paths.instance_dir(&selected).join("plugins"),
        LoaderType::Fabric | LoaderType::Vanilla => paths.instance_dir(&selected).join("mods"),
    };

    client.download_file(primary_file, &target_folder).await?;
    println!("{}", format!("\n✓ ¡{} instalado exitosamente en '{}'!", selected_hit.title, selected).green().bold());
    println!("Reinicia el servidor para cargar el nuevo archivo con: {}", format!("mcserman restart {}", selected).cyan());

    Ok(())
}

fn print_colored_log(line: &str) {
    if line.contains("ERROR") || line.contains("Exception") {
        eprintln!("{}", line.red());
    } else if line.contains("WARN") {
        println!("{}", line.yellow());
    } else if line.contains("INFO") {
        println!("{}", line);
    } else {
        println!("{}", line.dimmed());
    }
}
