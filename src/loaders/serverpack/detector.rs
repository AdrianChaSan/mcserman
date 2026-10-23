use std::path::Path;

/// Información técnica del servidor inferida tras inspeccionar el árbol de archivos descomprimido.
#[derive(Debug, Default)]
pub struct DetectedServerInfo {
    pub server_jar: Option<String>,
    pub mc_version: Option<String>,
    pub loader: Option<String>,
    pub loader_version: Option<String>,
}

/// Intenta extraer la versión de Forge a partir de textos, readmes o scripts presentes en el Server Pack.
pub fn extract_forge_version_from_text(content: &str, mc_ver: Option<&str>) -> Option<String> {
    if let Some(mc) = mc_ver {
        for prefix in [format!("forge-{}-", mc), format!("forge/{}-", mc)] {
            if let Some(pos) = content.find(&prefix) {
                let rest = &content[pos + prefix.len()..];
                let ver: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
                if ver.split('.').count() >= 3 {
                    return Some(ver);
                }
            }
        }
    }

    // Formato manifest.json de CurseForge ("id": "forge-<version>")
    if let Some(pos) = content.find("\"forge-") {
        let rest = &content[pos + 7..];
        let ver: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if ver.split('.').count() >= 3 {
            return Some(ver);
        }
    }

    for word in content.split_whitespace() {
        if let Some(pos) = word.find("forge-") {
            let candidate = &word[pos + 6..];
            let clean = candidate
                .trim_end_matches(".jar")
                .trim_end_matches("-installer.jar")
                .trim_matches('"')
                .trim_matches('\'');
            let parts: Vec<&str> = clean.split('-').collect();
            for part in parts {
                if part.split('.').count() == 4
                    && part.chars().all(|c| c.is_ascii_digit() || c == '.')
                {
                    return Some(part.to_string());
                }
            }
        }
    }

    None
}

/// Escanea la estructura de directorios para detectar el jar ejecutable, modloader y versión de Minecraft.
pub fn detect_server_info(dir: &Path) -> DetectedServerInfo {
    let mut server_jar = None;
    let mut mc_version = None;
    let mut loader = None;
    let mut loader_version = None;

    if dir.join("forge.jar").is_file() {
        server_jar = Some("forge.jar".to_string());
        loader = loader.or_else(|| Some("forge".to_string()));
    } else if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                if let Some(file_name) = p.file_name().and_then(|n| n.to_str()) {
                    if file_name.starts_with("forge-")
                        && file_name.ends_with(".jar")
                        && !file_name.contains("installer")
                    {
                        server_jar = Some(file_name.to_string());
                        loader = loader.or_else(|| Some("forge".to_string()));

                        let name_clean = file_name
                            .strip_prefix("forge-")
                            .unwrap_or(file_name)
                            .strip_suffix("-universal.jar")
                            .unwrap_or_else(|| {
                                file_name
                                    .strip_prefix("forge-")
                                    .unwrap_or(file_name)
                                    .strip_suffix(".jar")
                                    .unwrap_or(file_name)
                            });
                        let parts: Vec<&str> = name_clean.split('-').collect();
                        if parts.len() >= 2 {
                            if parts[0].starts_with("1.") {
                                mc_version = mc_version.or_else(|| Some(parts[0].to_string()));
                            }
                            loader_version = loader_version.or_else(|| Some(parts[1].to_string()));
                        }
                        break;
                    }
                }
            }
        }
    }

    if server_jar.is_none() {
        let specific_jars = [
            ("fabric-server-launch.jar", "fabric"),
            ("fabric-server.jar", "fabric"),
        ];
        for (name, l) in specific_jars {
            if dir.join(name).is_file() {
                server_jar = Some(name.to_string());
                loader = loader.or_else(|| Some(l.to_string()));
                break;
            }
        }
    }

    if server_jar.is_none() {
        let generic_jars = ["server.jar", "run.jar"];
        for name in &generic_jars {
            if dir.join(name).is_file() {
                server_jar = Some(name.to_string());
                break;
            }
        }
    }

    if server_jar.is_none() {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Some(file_name) = p.file_name().and_then(|n| n.to_str()) {
                        let lower = file_name.to_lowercase();
                        if lower.ends_with(".jar")
                            && lower.contains("server")
                            && !lower.starts_with("minecraft_server")
                        {
                            server_jar = Some(file_name.to_string());
                            break;
                        }
                    }
                }
            }
        }
    }

    let var_file = dir.join("variables.txt");
    if var_file.is_file() {
        if let Ok(content) = std::fs::read_to_string(&var_file) {
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with('#') || line.is_empty() {
                    continue;
                }
                if let Some((k, v)) = line.split_once('=') {
                    let k = k.trim();
                    let v = v.trim().trim_matches('"').trim_matches('\'');
                    match k {
                        "MINECRAFT_VERSION" if !v.is_empty() => {
                            mc_version = Some(v.to_string());
                        }
                        "MODLOADER" if !v.is_empty() => {
                            loader = Some(v.to_lowercase());
                        }
                        "MODLOADER_VERSION" if !v.is_empty() => {
                            loader_version = Some(v.to_string());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Escanear readmes, scripts y manifiestos en la raíz para detectar versión de Forge
    if loader_version.is_none() {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                    let name_lower = name.to_lowercase();
                    if ext == "txt"
                        || ext == "bat"
                        || ext == "sh"
                        || ext == "json"
                        || name_lower.contains("readme")
                    {
                        if let Ok(content) = std::fs::read_to_string(&p) {
                            if let Some(ver) =
                                extract_forge_version_from_text(&content, mc_version.as_deref())
                            {
                                loader_version = Some(ver);
                                loader = loader.or_else(|| Some("forge".to_string()));
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    // Inspeccionar mods/ para corroborar loader y versión de MC por frecuencia
    let mods_dir = dir.join("mods");
    if mods_dir.is_dir() {
        let mut mc_version_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        if let Ok(entries) = std::fs::read_dir(&mods_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if loader.is_none() {
                    let name_lower = name.to_lowercase();
                    if name_lower.contains("neoforge") {
                        loader = Some("neoforge".to_string());
                    } else if name_lower.contains("forge") {
                        loader = Some("forge".to_string());
                    } else if name_lower.contains("fabric") {
                        loader = Some("fabric".to_string());
                    }
                }
                if mc_version.is_none() {
                    for part in name.split(|c: char| !c.is_alphanumeric() && c != '.') {
                        let candidate = part.trim_start_matches(|c: char| c.is_alphabetic());
                        if candidate.starts_with("1.") {
                            let segs: Vec<&str> = candidate.split('.').collect();
                            if segs.len() >= 2 && segs.len() <= 3 {
                                if let (Ok(1), Ok(minor)) =
                                    (segs[0].parse::<u32>(), segs[1].parse::<u32>())
                                {
                                    if (7..=30).contains(&minor) {
                                        let valid_patch = if segs.len() == 3 {
                                            segs[2].parse::<u32>().is_ok()
                                        } else {
                                            true
                                        };
                                        if valid_patch {
                                            *mc_version_counts
                                                .entry(candidate.to_string())
                                                .or_insert(0) += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if mc_version.is_none() {
            if let Some((best_ver, _)) =
                mc_version_counts.into_iter().max_by_key(|(_, count)| *count)
            {
                mc_version = Some(best_ver);
            }
        }
    }

    if loader_version.is_none() {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                    let name_lower = name.to_lowercase();
                    if ext == "txt"
                        || ext == "bat"
                        || ext == "sh"
                        || ext == "json"
                        || name_lower.contains("readme")
                    {
                        if let Ok(content) = std::fs::read_to_string(&p) {
                            if let Some(ver) =
                                extract_forge_version_from_text(&content, mc_version.as_deref())
                            {
                                loader_version = Some(ver);
                                loader = loader.or_else(|| Some("forge".to_string()));
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    DetectedServerInfo {
        server_jar,
        mc_version,
        loader,
        loader_version,
    }
}
