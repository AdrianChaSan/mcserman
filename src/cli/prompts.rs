use std::io::IsTerminal;
use anyhow::{bail, Result};
use colored::Colorize;
use inquire::{InquireError, Select, Text};
use crate::core::config::LoaderType;

pub fn is_tty() -> bool {
    std::io::stdin().is_terminal()
}

pub fn handle_inquire_cancel<T>(res: Result<T, InquireError>) -> Result<T> {
    match res {
        Ok(val) => Ok(val),
        Err(InquireError::OperationCanceled) | Err(InquireError::OperationInterrupted) => {
            // Restaurar cursor de terminal de forma limpia
            print!("\x1B[?25h");
            println!("\n{}", "Operación cancelada por el usuario.".yellow());
            std::process::exit(130);
        }
        Err(e) => bail!("Error en entrada interactiva: {}", e),
    }
}

pub fn select_instance(instances: &[String], prompt_text: &str) -> Result<String> {
    if instances.is_empty() {
        bail!("No hay ninguna instancia creada todavía. Crea una primero con 'mcserman create'.");
    }
    if instances.len() == 1 {
        return Ok(instances[0].clone());
    }
    if !is_tty() {
        bail!("Se requiere especificar el nombre de la instancia en entornos no interactivos.");
    }

    let choice = handle_inquire_cancel(
        Select::new(prompt_text, instances.to_vec()).prompt()
    )?;
    Ok(choice)
}

pub fn prompt_loader(loader: Option<String>) -> Result<LoaderType> {
    if let Some(l) = loader {
        return l.parse();
    }
    if !is_tty() {
        return Ok(LoaderType::Paper); // Default seguro no-interactivo
    }

    let options = vec![
        "paper (Recomendado para servidores con plugins y alto rendimiento)",
        "fabric (Recomendado para mods técnicos y optimizaciones modernas)",
        "vanilla (Servidor oficial de Mojang sin modificaciones)",
    ];

    let choice = handle_inquire_cancel(
        Select::new("Selecciona el tipo de servidor (Loader):", options).prompt()
    )?;

    if choice.starts_with("paper") {
        Ok(LoaderType::Paper)
    } else if choice.starts_with("fabric") {
        Ok(LoaderType::Fabric)
    } else {
        Ok(LoaderType::Vanilla)
    }
}

pub fn prompt_mc_version(version: Option<String>) -> Result<String> {
    if let Some(v) = version {
        return Ok(v);
    }
    if !is_tty() {
        return Ok("1.21.4".to_string());
    }

    let common_versions = vec![
        "1.21.4 (Última versión estable actual)",
        "1.21.1",
        "1.20.4",
        "1.20.1",
        "1.16.5",
        "Otra versión personalizada...",
    ];

    let choice = handle_inquire_cancel(
        Select::new("Selecciona la versión de Minecraft:", common_versions).prompt()
    )?;

    if choice.starts_with("Otra") {
        let custom = handle_inquire_cancel(
            Text::new("Ingresa la versión exacta de Minecraft (ej. 1.21):").prompt()
        )?;
        Ok(custom.trim().to_string())
    } else {
        let ver = choice.split_whitespace().next().unwrap_or("1.21.4");
        Ok(ver.to_string())
    }
}

pub fn prompt_string(val: Option<String>, prompt_text: &str, default: &str) -> Result<String> {
    if let Some(v) = val {
        return Ok(v);
    }
    if !is_tty() {
        return Ok(default.to_string());
    }

    let res = handle_inquire_cancel(
        Text::new(prompt_text).with_default(default).prompt()
    )?;
    Ok(res.trim().to_string())
}
