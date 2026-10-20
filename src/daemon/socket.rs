use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

#[derive(Debug, Serialize, Deserialize)]
pub enum SocketRequest {
    Command(String),
    Stop,
    Ping,
    Status,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum SocketResponse {
    Ok(String),
    Error(String),
}

pub async fn send_request(socket_path: &Path, req: &SocketRequest) -> Result<SocketResponse> {
    let mut stream = UnixStream::connect(socket_path).await.with_context(|| {
        format!(
            "No se pudo conectar al socket en {}. ¿Está el servidor encendido?",
            socket_path.display()
        )
    })?;

    let json = serde_json::to_string(req)? + "\n";
    stream.write_all(json.as_bytes()).await?;
    stream.flush().await?;

    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line).await?;

    let resp: SocketResponse = serde_json::from_str(&response_line)
        .context("Respuesta inválida recibida del socket de mcserman")?;

    Ok(resp)
}

pub async fn is_daemon_running(socket_path: &Path) -> bool {
    if !socket_path.exists() {
        return false;
    }
    match send_request(socket_path, &SocketRequest::Ping).await {
        Ok(SocketResponse::Ok(_)) => true,
        _ => {
            // El socket existe pero no responde (socket huérfano de crash previo)
            let _ = std::fs::remove_file(socket_path);
            false
        }
    }
}
