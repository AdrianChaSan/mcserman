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
