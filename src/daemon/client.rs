use crate::ipc::{IpcRequest, IpcResponse, get_ipc_socket_path};
use anyhow::Result;
use interprocess::local_socket::tokio::prelude::LocalSocketStream;
use interprocess::local_socket::traits::tokio::Stream;
use interprocess::local_socket::{ToFsName, GenericFilePath};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use crate::daemon::traits::DaemonError;

pub async fn send_request(req: IpcRequest) -> Result<IpcResponse, DaemonError> {
    let socket_path = get_ipc_socket_path().map_err(|e| DaemonError::ConnectionFailed(e.to_string()))?;
    
    // Connect
    // On Unix, name is path. On Windows, name is pipe name.
    let name = socket_path.to_fs_name::<GenericFilePath>().map_err(|e| DaemonError::ConnectionFailed(e.to_string()))?;
    
    let mut stream: LocalSocketStream = LocalSocketStream::connect(name).await
        .map_err(|e| DaemonError::ConnectionFailed(e.to_string()))?;
        
    // Send Request
    let req_str = serde_json::to_string(&req).map_err(|e| DaemonError::IpcError(crate::ipc::IpcError::SerializationError(e.to_string())))?;
    stream.write_all(req_str.as_bytes()).await.map_err(|e| DaemonError::IoError(e))?;
    stream.write_all(b"\n").await.map_err(|e| DaemonError::IoError(e))?;
    
    // Read Response
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await.map_err(|e| DaemonError::IoError(e))?;
    
    let resp = serde_json::from_str(&line).map_err(|e| DaemonError::IpcError(crate::ipc::IpcError::SerializationError(e.to_string())))?;
    Ok(resp)
}