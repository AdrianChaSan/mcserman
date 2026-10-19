pub mod vanilla;
pub mod paper;
pub mod fabric;

use std::path::Path;
use anyhow::Result;
use crate::core::config::LoaderType;

pub async fn download_server_jar(loader: LoaderType, mc_version: &str, target_path: &Path) -> Result<Option<String>> {
    match loader {
        LoaderType::Vanilla => {
            vanilla::download_vanilla(mc_version, target_path).await?;
            Ok(None)
        }
        LoaderType::Paper => {
            let build = paper::download_paper(mc_version, target_path).await?;
            Ok(Some(build))
        }
        LoaderType::Fabric => {
            let loader_ver = fabric::download_fabric(mc_version, target_path).await?;
            Ok(Some(loader_ver))
        }
    }
}

pub async fn fetch_available_versions(loader: &LoaderType) -> Result<Vec<String>> {
    let client = reqwest::Client::builder()
        .user_agent("mcserman/0.1.0 (https://github.com/AdrianChaSan/mcserman)")
        .build()?;

    match loader {
        LoaderType::Paper => {
            let res = client.get("https://fill.papermc.io/v3/projects/paper").send().await?;
            if !res.status().is_success() {
                anyhow::bail!("Error al consultar versiones de PaperMC: HTTP {}", res.status());
            }
            let data: serde_json::Value = res.json().await?;
            let mut versions = Vec::new();
            if let Some(versions_map) = data.get("versions").and_then(|v| v.as_object()) {
                for (_major, list) in versions_map {
                    if let Some(arr) = list.as_array() {
                        for v in arr {
                            if let Some(s) = v.as_str() {
                                if !s.contains("-pre") && !s.contains("-rc") {
                                    versions.push(s.to_string());
                                }
                            }
                        }
                    }
                }
            }
            Ok(versions)
        }
        LoaderType::Fabric => {
            let res = client.get("https://meta.fabricmc.net/v2/versions/game").send().await?;
            if !res.status().is_success() {
                anyhow::bail!("Error al consultar versiones de Fabric: HTTP {}", res.status());
            }
            let data: serde_json::Value = res.json().await?;
            let mut versions = Vec::new();
            if let Some(arr) = data.as_array() {
                for item in arr {
                    if item.get("stable").and_then(|b| b.as_bool()).unwrap_or(false) {
                        if let Some(v) = item.get("version").and_then(|s| s.as_str()) {
                            versions.push(v.to_string());
                        }
                    }
                }
            }
            Ok(versions)
        }
        LoaderType::Vanilla => {
            let res = client.get("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").send().await?;
            if !res.status().is_success() {
                anyhow::bail!("Error al consultar versiones de Mojang: HTTP {}", res.status());
            }
            let data: serde_json::Value = res.json().await?;
            let mut versions = Vec::new();
            if let Some(arr) = data.get("versions").and_then(|v| v.as_array()) {
                for item in arr {
                    if item.get("type").and_then(|t| t.as_str()) == Some("release") {
                        if let Some(v) = item.get("id").and_then(|s| s.as_str()) {
                            versions.push(v.to_string());
                        }
                    }
                }
            }
            Ok(versions)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_paper_versions() {
        let versions = fetch_available_versions(&LoaderType::Paper).await.unwrap();
        assert!(!versions.is_empty());
        assert!(versions.iter().any(|v| v == "1.21.4" || v == "1.20.4"));
    }

    #[tokio::test]
    async fn test_fetch_fabric_versions() {
        let versions = fetch_available_versions(&LoaderType::Fabric).await.unwrap();
        assert!(!versions.is_empty());
        assert!(versions.iter().any(|v| v == "1.21.4" || v == "1.20.1"));
    }

    #[tokio::test]
    async fn test_fetch_vanilla_versions() {
        let versions = fetch_available_versions(&LoaderType::Vanilla).await.unwrap();
        assert!(!versions.is_empty());
        assert!(versions.iter().any(|v| v == "1.21.4" || v == "1.16.5"));
    }
}
