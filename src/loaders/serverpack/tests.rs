use super::*;
use crate::core::paths::SystemPaths;

#[test]
fn test_parse_curseforge_url() {
    let url1 =
        "https://www.curseforge.com/minecraft/modpacks/better-mc-fabric-bmc2/files/7522918";
    let (slug1, file_id1) = parse_curseforge_url(url1);
    assert_eq!(slug1, Some("better-mc-fabric-bmc2".to_string()));
    assert_eq!(file_id1, Some(7522918));

    let url2 = "https://curseforge.com/minecraft/modpacks/all-the-mods-9/files/5123456";
    let (slug2, file_id2) = parse_curseforge_url(url2);
    assert_eq!(slug2, Some("all-the-mods-9".to_string()));
    assert_eq!(file_id2, Some(5123456));

    let url_zip = "https://mediafilez.forgecdn.net/files/7522/918/BMC2_Server_Pack_v40.zip";
    assert!(is_zip_url(url_zip));
    assert!(!is_curseforge_url(url_zip));
}

#[test]
fn test_detect_server_info_from_variables() {
    let temp_dir =
        std::env::temp_dir().join(format!("mcserman_test_detect_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let var_content = "MINECRAFT_VERSION=1.20.1\nMODLOADER=Fabric\nMODLOADER_VERSION=0.18.4\n";
    std::fs::write(temp_dir.join("variables.txt"), var_content).unwrap();

    let info = detect_server_info(&temp_dir);
    assert_eq!(info.mc_version, Some("1.20.1".to_string()));
    assert_eq!(info.loader, Some("fabric".to_string()));
    assert_eq!(info.loader_version, Some("0.18.4".to_string()));
    assert_eq!(info.server_jar, None);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_extract_serverpack_zip_single_root_unwrapping() {
    use std::io::Write;
    let temp_dir =
        std::env::temp_dir().join(format!("mcserman_test_sp_zip_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let zip_path = temp_dir.join("pack.zip");

    {
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();

        zip.start_file("MyServerPack/server.properties", options)
            .unwrap();
        zip.write_all(b"motd=Hello Serverpack\n").unwrap();

        zip.start_file("MyServerPack/mods/test-mod-1.20.1.jar", options)
            .unwrap();
        zip.write_all(b"dummy jar content").unwrap();

        zip.start_file("MyServerPack/run.sh", options)
            .unwrap();
        zip.write_all(b"#!/bin/sh\necho running\n").unwrap();

        zip.finish().unwrap();
    }

    let out_dir = temp_dir.join("out");
    let (tx, _rx) = tokio::sync::mpsc::channel(10);
    let reporter = crate::core::progress::ChannelProgressReporter::new(tx);

    extract_serverpack_zip(&zip_path, &out_dir, &reporter).unwrap();

    // Debe haber desenvuelto "MyServerPack" directamente en out_dir
    assert!(out_dir.join("server.properties").exists());
    assert!(out_dir.join("mods").join("test-mod-1.20.1.jar").exists());
    let run_sh = out_dir.join("run.sh");
    assert!(run_sh.exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::metadata(&run_sh).unwrap().permissions();
        assert_eq!(perms.mode() & 0o111, 0o111, "run.sh debe tener permisos de ejecución");
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_detect_server_info_neoforge() {
    let temp_dir =
        std::env::temp_dir().join(format!("mcserman_test_detect_neo_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let var_content = "MINECRAFT_VERSION=1.21.1\nMODLOADER=NeoForge\nMODLOADER_VERSION=21.1.250\nUSE_SSJ=true\n";
    std::fs::write(temp_dir.join("variables.txt"), var_content).unwrap();

    let info = detect_server_info(&temp_dir);
    assert_eq!(info.mc_version, Some("1.21.1".to_string()));
    assert_eq!(info.loader, Some("neoforge".to_string()));
    assert_eq!(info.loader_version, Some("21.1.250".to_string()));
    assert_eq!(info.server_jar, None);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
#[ignore = "requires network access (skipped in NixOS build sandbox)"]
async fn test_curseforge_client_auto_resolves_to_serverpack() {
    let client_url =
        "https://www.curseforge.com/minecraft/modpacks/better-mc-neoforge-bmc5/files/8835241";
    let resolved = resolve_serverpack(client_url)
        .await
        .expect("Debe resolver automáticamente el Server Pack vinculado");

    assert_eq!(resolved.version_id, "8835262");
    assert_eq!(resolved.mc_version, "1.21.1");
    assert_eq!(resolved.loader, "neoforge");
    assert_eq!(resolved.mrpack_filename, "BMC5Neo_ServerPack_v53.zip");
    assert!(resolved.mrpack_url.contains("BMC5Neo_ServerPack_v53.zip"));
}

#[tokio::test]
#[ignore = "requires network access (skipped in NixOS build sandbox)"]
async fn test_curseforge_bmc2_serverpack() {
    let url =
        "https://www.curseforge.com/minecraft/modpacks/better-mc-fabric-bmc2/files/7522918";
    let resolved = resolve_serverpack(url)
        .await
        .expect("Error al resolver URL de CurseForge");

    assert_eq!(resolved.version_id, "7522918");
    assert_eq!(resolved.mc_version, "1.20.1");
    assert_eq!(resolved.loader, "fabric");
    assert!(resolved.mrpack_url.ends_with(".zip"));

    let temp_dir =
        std::env::temp_dir().join(format!("mcserman_bmc2_sp_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let (tx, _rx) = tokio::sync::mpsc::channel(10);
    let reporter = crate::core::progress::ChannelProgressReporter::new(tx);

    let paths = SystemPaths::resolve();
    let inst_name = format!("bmc2_test_{}", std::process::id());
    let inst_dir = paths.instance_dir(&inst_name);
    std::fs::create_dir_all(&inst_dir).unwrap();

    let cached_zip = std::path::Path::new("/tmp/bmc2_server_pack.zip");
    let zip_to_use = if cached_zip.is_file() {
        cached_zip.to_path_buf()
    } else {
        let client = reqwest::Client::new();
        let dl_path = temp_dir.join("downloaded.zip");
        download_serverpack_file(&client, &resolved.mrpack_url, &dl_path, &reporter)
            .await
            .unwrap();
        dl_path
    };

    extract_serverpack_zip(&zip_to_use, &inst_dir, &reporter).unwrap();

    let mods_dir = inst_dir.join("mods");
    assert!(mods_dir.is_dir());

    let mut has_balm = false;
    let mut has_fallingtree = false;
    if let Ok(entries) = std::fs::read_dir(&mods_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.to_lowercase().contains("balm") {
                has_balm = true;
            }
            if name.to_lowercase().contains("fallingtree") {
                has_fallingtree = true;
            }
        }
    }
    assert!(has_balm, "El mod Balm debe estar instalado en mods/");
    assert!(
        has_fallingtree,
        "El mod FallingTree debe estar instalado en mods/"
    );

    let detected = detect_server_info(&inst_dir);
    assert_eq!(detected.mc_version.as_deref(), Some("1.20.1"));
    assert_eq!(detected.loader.as_deref(), Some("fabric"));

    let jar_name = format!("fabric-server-{}.jar", "1.20.1");
    let target_jar = inst_dir.join(&jar_name);
    crate::loaders::fabric::download_fabric_custom(
        "1.20.1",
        detected.loader_version.as_deref(),
        &target_jar,
        &reporter,
    )
    .await
    .expect("Error al descargar Fabric Server");
    assert!(target_jar.is_file());

    let eula_path = inst_dir.join("eula.txt");
    std::fs::write(&eula_path, "eula=true\n").unwrap();
    assert!(std::fs::read_to_string(&eula_path)
        .unwrap()
        .contains("eula=true"));

    let _ = std::fs::remove_dir_all(&inst_dir);
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_detect_server_info_legacy_forge_preinstalled() {
    let temp_dir = std::env::temp_dir()
        .join(format!("mcserman_test_detect_forge_legacy_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    // Crear tanto forge-*.jar como minecraft_server.*.jar
    std::fs::write(
        temp_dir.join("minecraft_server.1.12.2.jar"),
        b"dummy vanilla",
    )
    .unwrap();
    std::fs::write(
        temp_dir.join("forge-1.12.2-14.23.5.2860.jar"),
        b"dummy forge",
    )
    .unwrap();

    let info = detect_server_info(&temp_dir);
    // Debe priorizar forge-*.jar sobre minecraft_server.*.jar
    assert_eq!(
        info.server_jar,
        Some("forge-1.12.2-14.23.5.2860.jar".to_string())
    );
    assert_eq!(info.loader, Some("forge".to_string()));
    assert_eq!(info.mc_version, Some("1.12.2".to_string()));
    assert_eq!(info.loader_version, Some("14.23.5.2860".to_string()));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_detect_server_info_legacy_forge_forge_dot_jar() {
    let temp_dir = std::env::temp_dir()
        .join(format!("mcserman_test_detect_forge_dot_jar_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    std::fs::write(
        temp_dir.join("minecraft_server.1.12.2.jar"),
        b"dummy vanilla",
    )
    .unwrap();
    std::fs::write(temp_dir.join("forge.jar"), b"dummy forge").unwrap();

    let info = detect_server_info(&temp_dir);
    assert_eq!(info.server_jar, Some("forge.jar".to_string()));
    assert_eq!(info.loader, Some("forge".to_string()));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_extract_forge_version_from_readme() {
    let text = r#"
    Quick Windows Server Install Guide
    2. Download the Forge 2860 server installer: https://maven.minecraftforge.net/net/minecraftforge/forge/1.12.2-14.23.5.2860/forge-1.12.2-14.23.5.2860-installer.jar
    java -server -Xmx6G -jar forge-1.12.2-14.23.5.2860.jar nogui
    "#;
    let ver = extract_forge_version_from_text(text, Some("1.12.2"));
    assert_eq!(ver, Some("14.23.5.2860".to_string()));

    let manifest = r#"{"minecraft":{"version":"1.12.2","modLoaders":[{"id":"forge-14.23.5.2860","primary":true}]}}"#;
    let ver_m = extract_forge_version_from_text(manifest, Some("1.12.2"));
    assert_eq!(ver_m, Some("14.23.5.2860".to_string()));
}

#[test]
fn test_detect_server_info_anomalous_mods_frequency() {
    let temp_dir = std::env::temp_dir()
        .join(format!("mcserman_test_anomalous_mods_{}", std::process::id()));
    let mods_dir = temp_dir.join("mods");
    std::fs::create_dir_all(&mods_dir).unwrap();

    // 1 mod con 1.9.1 que alfabéticamente va primero por el guion '-'
    std::fs::write(
        mods_dir.join("-mod-director-launchwrapper-1.9.1.jar"),
        b"dummy",
    )
    .unwrap();
    // Varios mods con 1.12.2
    std::fs::write(mods_dir.join("ToolBelt-1.12.2-1.9.14.jar"), b"dummy").unwrap();
    std::fs::write(mods_dir.join("jei_1.12.2-4.16.1.302.jar"), b"dummy").unwrap();
    std::fs::write(mods_dir.join("appleskin-mc1.12.2-1.0.14.jar"), b"dummy").unwrap();

    let info = detect_server_info(&temp_dir);
    // Debe ganar 1.12.2 por consenso de frecuencia y no 1.9.1
    assert_eq!(info.mc_version, Some("1.12.2".to_string()));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_detect_server_info_with_readme_version() {
    let temp_dir = std::env::temp_dir()
        .join(format!("mcserman_test_readme_forge_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let readme_content = r#"
    Download Forge 2860: https://maven.minecraftforge.net/net/minecraftforge/forge/1.12.2-14.23.5.2860/forge-1.12.2-14.23.5.2860-installer.jar
    "#;
    std::fs::write(
        temp_dir.join("SERVER README OR DIE.txt"),
        readme_content,
    )
    .unwrap();

    let info = detect_server_info(&temp_dir);
    assert_eq!(info.loader_version, Some("14.23.5.2860".to_string()));
    assert_eq!(info.loader, Some("forge".to_string()));

    let _ = std::fs::remove_dir_all(&temp_dir);
}
