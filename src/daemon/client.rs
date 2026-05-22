use crate::ipc::get_ipc_socket_path;
use crate::ipc::wire::{self, ClientMessage, Request, Response, ServerMessage};
use anyhow::Result;
use interprocess::local_socket::tokio::prelude::LocalSocketStream;
use interprocess::local_socket::traits::tokio::Stream;
use interprocess::local_socket::{GenericFilePath, ToFsName};
use crate::daemon::traits::DaemonError;
use std::sync::atomic::{AtomicU32, Ordering};

/// Monotonic request ID counter (per-process).
static NEXT_REQUEST_ID: AtomicU32 = AtomicU32::new(1);

/// Send a one-shot request over a new connection and return the response.
///
/// Opens a fresh Unix socket connection, sends a single `ClientMessage::Request`,
/// reads the corresponding `ServerMessage::Response`, and disconnects.
pub async fn send_request(req: Request) -> Result<Response, DaemonError> {
    let socket_path = get_ipc_socket_path()
        .map_err(|e| DaemonError::ConnectionFailed(e.to_string()))?;
    let name = socket_path
        .to_fs_name::<GenericFilePath>()
        .map_err(|e| DaemonError::ConnectionFailed(e.to_string()))?;

    let conn: LocalSocketStream = LocalSocketStream::connect(name)
        .await
        .map_err(|e| DaemonError::ConnectionFailed(e.to_string()))?;

    let (mut reader, mut writer) = tokio::io::split(conn);

    let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let msg = ClientMessage::Request { id, payload: req };

    wire::write_frame(&mut writer, &msg)
        .await
        .map_err(|e| DaemonError::IpcError(e))?;

    // Read the response frame.
    let server_msg: ServerMessage = wire::read_frame(&mut reader)
        .await
        .map_err(|e| DaemonError::IpcError(e))?
        .ok_or_else(|| DaemonError::ConnectionFailed("daemon closed connection before responding".into()))?;

    match server_msg {
        ServerMessage::Response { id: resp_id, payload } if resp_id == id => Ok(payload),
        ServerMessage::Response { id: resp_id, payload } => {
            eprintln!("Warning: request ID mismatch (sent {id}, got {resp_id})");
            Ok(payload)
        }
        ServerMessage::Event(_) => {
            Err(DaemonError::ConnectionFailed("unexpected event instead of response".into()))
        }
    }
}