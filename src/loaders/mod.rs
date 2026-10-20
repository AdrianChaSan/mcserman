pub mod fabric;
pub mod paper;
pub mod vanilla;

use crate::core::config::LoaderType;
use anyhow::Result;
use std::path::Path;

use crate::core::progress::ProgressReporter;

pub async fn download_server_jar(
    loader: LoaderType,
    mc_version: &str,
    target_path: &Path,
    reporter: &dyn ProgressReporter,
) -> Result<Option<String>> {
    match loader {
        LoaderType::Vanilla => {
            vanilla::download_vanilla(mc_version, target_path, reporter).await?;
            Ok(None)
        }
        LoaderType::Paper => {
            let build = paper::download_paper(mc_version, target_path, reporter).await?;
            Ok(Some(build))
        }
        LoaderType::Fabric => {
            let loader_ver = fabric::download_fabric(mc_version, target_path, reporter).await?;
            Ok(Some(loader_ver))
        }
    }
}

pub async fn fetch_available_versions(loader: &LoaderType) -> Result<Vec<String>> {
    let client = reqwest::Client::builder()
        .user_agent("mcserman/0.1.0 (https://github.com/AdrianChaSan/mcserman)")
        .build()?;

    let mut versions = match loader {
        LoaderType::Paper => {
            let res = client
                .get("https://fill.papermc.io/v3/projects/paper")
                .send()
                .await?;
            if !res.status().is_success() {
                anyhow::bail!(
                    "Error al consultar versiones de PaperMC: HTTP {}",
                    res.status()
                );
            }
            let data: serde_json::Value = res.json().await?;
            let mut list = Vec::new();
            if let Some(versions_map) = data.get("versions").and_then(|v| v.as_object()) {
                for (_major, val) in versions_map {
                    if let Some(arr) = val.as_array() {
                        for v in arr {
                            if let Some(s) = v.as_str() {
                                if !s.contains("-pre") && !s.contains("-rc") {
                                    list.push(s.to_string());
                                }
                            }
                        }
                    }
                }
            }
            list
        }
        LoaderType::Fabric => {
            let res = client
                .get("https://meta.fabricmc.net/v2/versions/game")
                .send()
                .await?;
            if !res.status().is_success() {
                anyhow::bail!(
                    "Error al consultar versiones de Fabric: HTTP {}",
                    res.status()
                );
            }
            let data: serde_json::Value = res.json().await?;
            let mut list = Vec::new();
            if let Some(arr) = data.as_array() {
                for item in arr {
                    if item
                        .get("stable")
                        .and_then(|b| b.as_bool())
                        .unwrap_or(false)
                    {
                        if let Some(v) = item.get("version").and_then(|s| s.as_str()) {
                            list.push(v.to_string());
                        }
                    }
                }
            }
            list
        }
        LoaderType::Vanilla => {
            let res = client
                .get("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")
                .send()
                .await?;
            if !res.status().is_success() {
                anyhow::bail!(
                    "Error al consultar versiones de Mojang: HTTP {}",
                    res.status()
                );
            }
            let data: serde_json::Value = res.json().await?;
            let mut list = Vec::new();
            if let Some(arr) = data.get("versions").and_then(|v| v.as_array()) {
                for item in arr {
                    if item.get("type").and_then(|t| t.as_str()) == Some("release") {
                        if let Some(v) = item.get("id").and_then(|s| s.as_str()) {
                            list.push(v.to_string());
                        }
                    }
                }
            }
            list
        }
    };

    sort_minecraft_versions(&mut versions);
    Ok(versions)
}

fn sort_minecraft_versions(versions: &mut Vec<String>) {
    versions.sort_by(|a, b| {
        let ka = parse_version_key(a);
        let kb = parse_version_key(b);
        kb.cmp(&ka) // Descendente: la versión más reciente primero
    });
    versions.dedup();
}

fn parse_version_key(v: &str) -> Vec<u32> {
    let num_part = v.split('-').next().unwrap_or(v);
    num_part
        .split('.')
        .filter_map(|p| p.parse::<u32>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires network access (skipped in NixOS build sandbox)"]
    async fn test_fetch_paper_versions() {
        let versions = fetch_available_versions(&LoaderType::Paper).await.unwrap();
        assert!(!versions.is_empty());
        assert!(versions.iter().any(|v| v == "1.21.4" || v == "1.20.4"));
    }

    #[tokio::test]
    #[ignore = "requires network access (skipped in NixOS build sandbox)"]
    async fn test_fetch_fabric_versions() {
        let versions = fetch_available_versions(&LoaderType::Fabric).await.unwrap();
        assert!(!versions.is_empty());
        assert!(versions.iter().any(|v| v == "1.21.4" || v == "1.20.1"));
    }

    #[tokio::test]
    #[ignore = "requires network access (skipped in NixOS build sandbox)"]
    async fn test_fetch_vanilla_versions() {
        let versions = fetch_available_versions(&LoaderType::Vanilla)
            .await
            .unwrap();
        assert!(!versions.is_empty());
        assert!(versions.iter().any(|v| v == "1.21.4" || v == "1.16.5"));
    }

    #[test]
    fn test_minecraft_version_sorting() {
        let mut sample = vec![
            "1.7.10".to_string(),
            "1.21.4".to_string(),
            "26.3".to_string(),
            "1.8.8".to_string(),
            "1.20.4".to_string(),
            "1.9.4".to_string(),
            "26.1.2".to_string(),
        ];
        sort_minecraft_versions(&mut sample);
        assert_eq!(
            sample,
            vec!["26.3", "26.1.2", "1.21.4", "1.20.4", "1.9.4", "1.8.8", "1.7.10",]
        );
    }
}
