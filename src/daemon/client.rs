use crate::ipc::get_ipc_socket_path;
use crate::ipc::wire::{self, ClientMessage, Request, Response, ServerEvent, ServerMessage};
use anyhow::Result;
use interprocess::local_socket::tokio::prelude::LocalSocketStream;
use interprocess::local_socket::traits::tokio::Stream;
use interprocess::local_socket::{GenericFilePath, ToFsName};
use crate::daemon::traits::DaemonError;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};

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

// ---------------------------------------------------------------------------
// Persistent client for browse mode
// ---------------------------------------------------------------------------

/// Map of in-flight request IDs to their response senders.
type PendingMap = Arc<Mutex<HashMap<u32, oneshot::Sender<Response>>>>;

/// Persistent IPC client that maintains a single connection to the daemon.
///
/// Supports both request/response (with ID-based correlation) and receiving
/// unsolicited `ServerEvent` pushes from the daemon. Designed for the TUI
/// browse session where we need full-duplex communication.
pub struct PersistentClient {
    /// Writer half of the socket, behind a mutex so `request()` is `&self`.
    writer: Mutex<tokio::io::WriteHalf<LocalSocketStream>>,
    /// Pending request map — background reader routes responses here.
    pending: PendingMap,
    /// Handle to the background reader task (aborted on drop).
    _reader_handle: tokio::task::JoinHandle<()>,
}

impl PersistentClient {
    /// Connect to the daemon, send `Subscribe`, and start the background
    /// reader that demuxes responses and events.
    ///
    /// Returns `(client, event_receiver)`. The caller owns the event channel.
    pub async fn connect() -> Result<(Self, mpsc::Receiver<ServerEvent>), DaemonError> {
        let socket_path = get_ipc_socket_path()
            .map_err(|e| DaemonError::ConnectionFailed(e.to_string()))?;
        let name = socket_path
            .to_fs_name::<GenericFilePath>()
            .map_err(|e| DaemonError::ConnectionFailed(e.to_string()))?;

        let conn: LocalSocketStream = LocalSocketStream::connect(name)
            .await
            .map_err(|e| DaemonError::ConnectionFailed(e.to_string()))?;

        let (reader, mut writer) = tokio::io::split(conn);

        // Subscribe to push events immediately.
        wire::write_frame(&mut writer, &ClientMessage::Subscribe)
            .await
            .map_err(DaemonError::IpcError)?;

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let (event_tx, event_rx) = mpsc::channel::<ServerEvent>(64);

        let reader_pending = Arc::clone(&pending);
        let reader_handle = tokio::spawn(Self::reader_loop(reader, reader_pending, event_tx));

        let client = Self {
            writer: Mutex::new(writer),
            pending,
            _reader_handle: reader_handle,
        };

        Ok((client, event_rx))
    }

    /// Send a request and wait for the matching response.
    pub async fn request(&self, req: Request) -> Result<Response, DaemonError> {
        let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();

        // Register the pending request before writing to avoid a race.
        self.pending.lock().await.insert(id, tx);

        let msg = ClientMessage::Request { id, payload: req };
        {
            let mut writer = self.writer.lock().await;
            if let Err(e) = wire::write_frame(&mut *writer, &msg).await {
                self.pending.lock().await.remove(&id);
                return Err(DaemonError::IpcError(e));
            }
        }

        rx.await.map_err(|_| {
            DaemonError::ConnectionFailed("daemon disconnected while awaiting response".into())
        })
    }

    /// Background task: reads frames from the daemon and routes them.
    /// - `Response{id}` → matched `oneshot::Sender` in `pending`
    /// - `Event(...)` → forwarded to `event_tx`
    async fn reader_loop(
        mut reader: tokio::io::ReadHalf<LocalSocketStream>,
        pending: PendingMap,
        event_tx: mpsc::Sender<ServerEvent>,
    ) {
        loop {
            let frame: std::result::Result<Option<ServerMessage>, _> =
                wire::read_frame(&mut reader).await;
            match frame {
                Ok(Some(ServerMessage::Response { id, payload })) => {
                    if let Some(tx) = pending.lock().await.remove(&id) {
                        // Receiver may have been dropped (timeout) — that's fine.
                        let _ = tx.send(payload);
                    }
                }
                Ok(Some(ServerMessage::Event(event))) => {
                    // If the channel is full or closed, drop the event.
                    let _ = event_tx.try_send(event);
                }
                // EOF or frame error — daemon disconnected.
                Ok(None) | Err(_) => break,
            }
        }
    }
}

impl Drop for PersistentClient {
    fn drop(&mut self) {
        self._reader_handle.abort();
    }
}