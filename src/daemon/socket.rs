use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::timeout;

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
    let connect_future = UnixStream::connect(socket_path);
    let mut stream = timeout(Duration::from_millis(600), connect_future)
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "Timeout al conectar con el socket en {}",
                socket_path.display()
            )
        })?
        .with_context(|| {
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
    timeout(
        Duration::from_millis(600),
        reader.read_line(&mut response_line),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Timeout esperando respuesta del socket"))??;

    let resp: SocketResponse = serde_json::from_str(&response_line)
        .context("Respuesta inválida recibida del socket de mcserman")?;

    Ok(resp)
}

pub async fn is_daemon_running(socket_path: &Path) -> bool {
    if !socket_path.exists() {
        return false;
    }
    matches!(
        send_request(socket_path, &SocketRequest::Ping).await,
        Ok(SocketResponse::Ok(_))
    )
}
