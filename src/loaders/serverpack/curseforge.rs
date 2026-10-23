use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::modrinth::modpack::{sanitize_instance_name, PackType, ResolvedModpack};

pub const CURSEFORGE_API_URL: &str = "https://api.curseforge.com/v1/mods/files";
pub const DEFAULT_CURSEFORGE_KEY: &str =
    "$2a$10$bL4bIL5pUWqfcO7KQtnMReakwtfHbNKh6v1uTpKlzhwoueEJQnPnm";

#[derive(Debug, Deserialize)]
pub(crate) struct CfFilesResponse {
    pub(crate) data: Vec<CfFileInfo>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub(crate) struct CfFileInfo {
    pub(crate) id: u64,
    pub(crate) mod_id: Option<u64>,
    pub(crate) display_name: String,
    pub(crate) file_name: String,
    pub(crate) download_url: Option<String>,
    pub(crate) file_length: Option<u64>,
    pub(crate) game_versions: Option<Vec<String>>,
    pub(crate) is_server_pack: Option<bool>,
    pub(crate) server_pack_file_id: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CfModResponse {
    pub(crate) data: CfModInfo,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct CfModInfo {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) slug: String,
}

/// Comprueba si la cadena provista corresponde a una URL de CurseForge o a un archivo ZIP directo.
pub fn is_curseforge_or_zip(input: &str) -> bool {
    is_curseforge_url(input) || is_zip_url(input)
}

/// Determina si una URL pertenece al dominio de CurseForge.
pub fn is_curseforge_url(input: &str) -> bool {
    let lower = input.to_lowercase();
    lower.contains("curseforge.com")
}

/// Determina si una URL apunta directamente a un archivo comprimido .zip.
pub fn is_zip_url(input: &str) -> bool {
    let lower = input.to_lowercase();
    if let Some(clean) = lower.split('?').next() {
        clean.ends_with(".zip")
    } else {
        lower.ends_with(".zip")
    }
}

/// Extrae el slug del proyecto y el ID numérico de archivo desde una URL de CurseForge.
pub fn parse_curseforge_url(input: &str) -> (Option<String>, Option<u64>) {
    let trimmed = input.trim();
    let without_proto = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);

    let clean = without_proto.trim_end_matches('/');
    let parts: Vec<&str> = clean.split('/').collect();

    let mut file_id = None;
    let mut slug = None;

    // Buscar "files" o "download" seguido de un número
    for (i, part) in parts.iter().enumerate() {
        if (*part == "files" || *part == "download") && i + 1 < parts.len() {
            if let Ok(id) = parts[i + 1].parse::<u64>() {
                file_id = Some(id);
                // Si antes de "files" hay un segmento que no sea "modpacks" o "mc-mods"
                if i > 0 {
                    let candidate = parts[i - 1];
                    if candidate != "modpacks" && candidate != "mc-mods" && candidate != "mods" {
                        slug = Some(candidate.to_string());
                    } else if i >= 2 {
                        slug = Some(parts[i - 1].to_string());
                    }
                }
                break;
            }
        }
    }

    (slug, file_id)
}

/// Resuelve los metadatos de un Server Pack ya sea desde CurseForge o mediante enlace directo .zip.
pub async fn resolve_serverpack(query: &str) -> Result<ResolvedModpack> {
    let trimmed = query.trim();
    if is_curseforge_url(trimmed) {
        resolve_curseforge_url(trimmed).await
    } else if is_zip_url(trimmed) {
        resolve_direct_zip_url(trimmed).await
    } else {
        bail!("No se reconoce como una URL válida de CurseForge o archivo .zip");
    }
}

pub(crate) async fn fetch_cf_file_info(
    client: &reqwest::Client,
    api_key: &str,
    file_id: u64,
) -> Result<CfFileInfo> {
    let payload = serde_json::json!({
        "fileIds": [file_id]
    });

    let resp = client
        .post(CURSEFORGE_API_URL)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("x-api-key", api_key)
        .json(&payload)
        .send()
        .await
        .with_context(|| {
            format!(
                "Error conectando a CurseForge API para el archivo {}",
                file_id
            )
        })?;

    if !resp.status().is_success() {
        bail!(
            "CurseForge API retornó error HTTP {}: {}",
            resp.status(),
            file_id
        );
    }

    let parsed: CfFilesResponse = resp.json().await.with_context(|| {
        format!(
            "Error al parsear metadatos de CurseForge API para el archivo {}",
            file_id
        )
    })?;

    parsed
        .data
        .into_iter()
        .find(|f| f.id == file_id)
        .with_context(|| {
            format!(
                "CurseForge no devolvió información para el archivo {}",
                file_id
            )
        })
}

/// Consulta información del mod o modpack por su identificador numérico de proyecto en CurseForge.
pub(crate) async fn fetch_cf_mod_info(
    client: &reqwest::Client,
    api_key: &str,
    mod_id: u64,
) -> Result<CfModInfo> {
    let url = format!("https://api.curseforge.com/v1/mods/{}", mod_id);
    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .header("x-api-key", api_key)
        .send()
        .await
        .with_context(|| format!("Error conectando a CurseForge API para el mod {}", mod_id))?;

    if !resp.status().is_success() {
        bail!(
            "CurseForge API retornó error HTTP {}: {}",
            resp.status(),
            mod_id
        );
    }

    let parsed: CfModResponse = resp.json().await.with_context(|| {
        format!(
            "Error al parsear información del mod {} de CurseForge",
            mod_id
        )
    })?;

    Ok(parsed.data)
}

/// Resuelve metadatos consultando la API de CurseForge y validando la existencia de un Server Pack.
pub async fn resolve_curseforge_url(url: &str) -> Result<ResolvedModpack> {
    let (slug_opt, file_id_opt) = parse_curseforge_url(url);
    let file_id = file_id_opt.with_context(|| {
        "No se pudo extraer el ID numérico de archivo de la URL de CurseForge (ej. /files/7522918)"
    })?;

    let client = reqwest::Client::builder()
        .user_agent(concat!("mcserman/", env!("CARGO_PKG_VERSION")))
        .build()?;

    let api_key = std::env::var("CURSEFORGE_API_KEY")
        .or_else(|_| std::env::var("CF_API_KEY"))
        .unwrap_or_else(|_| DEFAULT_CURSEFORGE_KEY.to_string());

    let mut file_info = fetch_cf_file_info(&client, &api_key, file_id).await?;
    let initial_mod_id = file_info.mod_id;
    let mut target_file_id = file_id;

    // Validación estricta y resolución de Server Pack
    if !file_info.is_server_pack.unwrap_or(false) {
        if let Some(sp_id) = file_info.server_pack_file_id {
            let mut sp_info = fetch_cf_file_info(&client, &api_key, sp_id).await.with_context(|| {
                format!(
                    "Error al obtener metadatos del Server Pack vinculado (ID {})",
                    sp_id
                )
            })?;
            if sp_info.mod_id.is_none() {
                sp_info.mod_id = initial_mod_id;
            }
            target_file_id = sp_id;
            file_info = sp_info;
        } else {
            bail!("El modpack seleccionado no cuenta con un Server Pack oficial en CurseForge. No se puede crear una instancia de servidor con el archivo de cliente.");
        }
    }

    let (modpack_name, mod_id_opt) = if let Some(mid) = file_info.mod_id.or(initial_mod_id) {
        match fetch_cf_mod_info(&client, &api_key, mid).await {
            Ok(info) => (info.name, Some(mid)),
            Err(_) => (file_info.display_name.clone(), Some(mid)),
        }
    } else {
        (file_info.display_name.clone(), None)
    };

    let version_str = file_info
        .display_name
        .trim()
        .strip_prefix(&modpack_name)
        .map(|s| s.trim().trim_start_matches('-').trim_start_matches('v').trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(&file_info.display_name)
        .to_string();

    let modpack_info = Some(crate::core::config::ModpackInfo {
        provider: "CurseForge".to_string(),
        name: modpack_name.clone(),
        version: version_str,
        project_id: mod_id_opt.map(|id| id.to_string()),
        file_id: Some(target_file_id.to_string()),
    });

    let download_url = match file_info.download_url {
        Some(ref u) if !u.is_empty() => u.clone(),
        _ => {
            let p1 = target_file_id / 1000;
            let p2 = target_file_id % 1000;
            format!(
                "https://mediafilez.forgecdn.net/files/{}/{}/{}",
                p1, p2, file_info.file_name
            )
        }
    };

    let mut mc_version: Option<String> = None;
    let mut loader = "fabric".to_string();

    if let Some(ref gv) = file_info.game_versions {
        for v in gv {
            let v_lower = v.to_lowercase();
            if v_lower == "fabric"
                || v_lower == "forge"
                || v_lower == "neoforge"
                || v_lower == "quilt"
            {
                loader = v_lower;
            } else if v.starts_with("1.") && v[2..].chars().all(|c| c.is_ascii_digit() || c == '.')
            {
                // Preferir versiones con mayor especificidad (ej. 1.20.1 sobre 1.20)
                match &mc_version {
                    None => mc_version = Some(v.clone()),
                    Some(current) => {
                        let cur_dots = current.split('.').count();
                        let new_dots = v.split('.').count();
                        if new_dots > cur_dots {
                            mc_version = Some(v.clone());
                        }
                    }
                }
            }
        }
    }
    let mc_version = mc_version.unwrap_or_else(|| "1.20.1".to_string());

    let project_slug = slug_opt.unwrap_or_else(|| {
        file_info
            .file_name
            .strip_suffix(".zip")
            .unwrap_or(&file_info.file_name)
            .to_string()
    });

    let suggested_name = sanitize_instance_name(&project_slug);

    Ok(ResolvedModpack {
        pack_type: PackType::ServerPackZip,
        project_id: mod_id_opt
            .map(|id| id.to_string())
            .unwrap_or_else(|| target_file_id.to_string()),
        project_title: modpack_name,
        version_id: target_file_id.to_string(),
        version_name: file_info.file_name.clone(),
        suggested_instance_name: suggested_name,
        mc_version,
        loader,
        loader_version: None,
        mrpack_url: download_url,
        mrpack_filename: file_info.file_name,
        mrpack_sha512: None,
        mrpack_size: file_info.file_length.unwrap_or(0),
        modpack_info,
    })
}

/// Inspecciona y resuelve un archivo ZIP remoto directamente mediante cabeceras HTTP.
pub async fn resolve_direct_zip_url(url: &str) -> Result<ResolvedModpack> {
    let clean = url.trim();
    let filename = clean
        .split('?')
        .next()
        .unwrap_or(clean)
        .split('/')
        .next_back()
        .unwrap_or("serverpack.zip")
        .to_string();

    let name_stem = filename.strip_suffix(".zip").unwrap_or(&filename);
    let suggested_name = sanitize_instance_name(name_stem);

    Ok(ResolvedModpack {
        pack_type: PackType::ServerPackZip,
        project_id: suggested_name.clone(),
        project_title: name_stem.to_string(),
        version_id: "custom".to_string(),
        version_name: filename.clone(),
        suggested_instance_name: suggested_name,
        mc_version: "1.20.1".to_string(),
        loader: "fabric".to_string(),
        loader_version: None,
        mrpack_url: clean.to_string(),
        mrpack_filename: filename,
        mrpack_sha512: None,
        mrpack_size: 0,
        modpack_info: None,
    })
}
