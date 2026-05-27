//! Binary wire protocol for persistent bidirectional IPC.
//!
//! Uses wincode (bincode-compatible) for serialization and length-prefixed
//! framing over a local socket stream.
//!
//! ## Frame format
//!
//! ```text
//! ┌───────────┬────────────────────┐
//! │ u32 LE    │ wincode payload    │
//! │ (length)  │                    │
//! └───────────┴────────────────────┘
//! ```
//!
//! ## Message flow
//!
//! - Client sends `ClientMessage`, daemon replies with `ServerMessage`
//! - Requests carry a `u32` id so responses can be matched
//! - Subscribed clients also receive `ServerEvent` pushes

use wincode::{SchemaRead, SchemaWrite};

use super::IpcError;

// ---------------------------------------------------------------------------
// Wire types — all paths are String (wincode doesn't support PathBuf)
// ---------------------------------------------------------------------------

/// Client → Daemon message.
#[derive(Debug, Clone, SchemaWrite, SchemaRead)]
pub enum ClientMessage {
    /// One-shot request expecting exactly one `ServerMessage::Response`.
    Request { id: u32, payload: Request },
    /// Ask the daemon to push `ServerEvent`s on this connection.
    Subscribe,
    /// Stop receiving push events.
    Unsubscribe,
}

/// Typed request payloads.
#[derive(Debug, Clone, SchemaWrite, SchemaRead)]
pub enum Request {
    Ping,
    Shutdown,

    // -- Queries --
    ListTags,
    ListFiles,
    ListAllPaths,
    SearchFiles { params: WireSearchParams },
    GetTags { file: String },
    GetNote { file: String },
    FindByTag { tag: String },
    FindByTags { tags: Vec<String>, match_all: bool },
    FindByTagRegex { pattern: String },
    ListNotes,

    // -- Mutations --
    AddTags { file: String, tags: Vec<String> },
    SetTags { file: String, tags: Vec<String> },
    RemoveTags { file: String, tags: Vec<String>, all: bool },
    SetNote { file: String, content: String },
    DeleteNote { file: String },
    DeleteFromDb { file: String },
    Cleanup,
}

/// Daemon → Client message.
#[derive(Debug, Clone, SchemaWrite, SchemaRead)]
pub enum ServerMessage {
    /// Reply to a `ClientMessage::Request`.
    Response { id: u32, payload: Response },
    /// Unsolicited notification (only sent to subscribed connections).
    Event(ServerEvent),
}

/// Typed response payloads.
#[derive(Debug, Clone, SchemaWrite, SchemaRead)]
pub enum Response {
    Pong,
    Ok,
    Tags(Vec<WireTagInfo>),
    Files(Vec<WireFilePair>),
    FileTags(Vec<String>),
    FilePaths(Vec<String>),
    Notes(Vec<WireNoteEntry>),
    /// Single note response (None = no note for this file).
    Note(Option<WireNoteEntry>),
    CleanupResult { removed: u64 },
    Error(String),
}

/// Pushed when the daemon modifies data.
#[derive(Debug, Clone, SchemaWrite, SchemaRead)]
pub enum ServerEvent {
    /// A file was tagged (via watch rule or IPC mutation).
    FileTagged { file: String, tags: Vec<String> },
    /// Tags were removed from a file.
    FileUntagged { file: String, tags: Vec<String> },
    /// A file entry was deleted from the database.
    FileRemoved { file: String },
    /// A note was created/updated (content = Some) or deleted (content = None).
    NoteChanged { file: String, content: Option<String> },
    /// watch.toml was reloaded.
    ConfigReloaded,
}

// ---------------------------------------------------------------------------
// Wire data types
// ---------------------------------------------------------------------------

/// Tag name with file count.
#[derive(Debug, Clone, SchemaWrite, SchemaRead, PartialEq, Eq)]
pub struct WireTagInfo {
    pub name: String,
    pub file_count: u64,
}

/// A file path with its tags.
#[derive(Debug, Clone, SchemaWrite, SchemaRead, PartialEq, Eq)]
pub struct WireFilePair {
    pub file: String,
    pub tags: Vec<String>,
}

/// A note entry.
#[derive(Debug, Clone, SchemaWrite, SchemaRead)]
pub struct WireNoteEntry {
    pub path: String,
    pub content: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Search parameters over the wire.
#[derive(Debug, Clone, SchemaWrite, SchemaRead)]
#[allow(clippy::struct_excessive_bools)]
pub struct WireSearchParams {
    pub query: Option<String>,
    pub tags: Vec<String>,
    pub file_patterns: Vec<String>,
    pub exclude_tags: Vec<String>,
    pub virtual_tags: Vec<String>,
    pub tag_mode: WireSearchMode,
    pub file_mode: WireSearchMode,
    pub virtual_mode: WireSearchMode,
    pub no_hierarchy: bool,
    pub regex_tag: bool,
    pub regex_file: bool,
    pub glob_files: bool,
}

/// Wire-safe search mode (mirrors `cli::SearchMode`).
#[derive(Debug, Clone, SchemaWrite, SchemaRead)]
pub enum WireSearchMode {
    Any,
    All,
}

// ---------------------------------------------------------------------------
// Conversions between wire types and domain types
// ---------------------------------------------------------------------------

impl From<&crate::cli::SearchParams> for WireSearchParams {
    fn from(p: &crate::cli::SearchParams) -> Self {
        Self {
            query: p.query.clone(),
            tags: p.tags.clone(),
            file_patterns: p.file_patterns.clone(),
            exclude_tags: p.exclude_tags.clone(),
            virtual_tags: p.virtual_tags.clone(),
            tag_mode: (&p.tag_mode).into(),
            file_mode: (&p.file_mode).into(),
            virtual_mode: (&p.virtual_mode).into(),
            no_hierarchy: p.no_hierarchy,
            regex_tag: p.regex_tag,
            regex_file: p.regex_file,
            glob_files: p.glob_files,
        }
    }
}

impl From<&WireSearchParams> for crate::cli::SearchParams {
    fn from(w: &WireSearchParams) -> Self {
        Self {
            query: w.query.clone(),
            tags: w.tags.clone(),
            file_patterns: w.file_patterns.clone(),
            exclude_tags: w.exclude_tags.clone(),
            virtual_tags: w.virtual_tags.clone(),
            tag_mode: (&w.tag_mode).into(),
            file_mode: (&w.file_mode).into(),
            virtual_mode: (&w.virtual_mode).into(),
            no_hierarchy: w.no_hierarchy,
            regex_tag: w.regex_tag,
            regex_file: w.regex_file,
            glob_files: w.glob_files,
        }
    }
}

impl From<&crate::cli::SearchMode> for WireSearchMode {
    fn from(m: &crate::cli::SearchMode) -> Self {
        match m {
            crate::cli::SearchMode::Any => Self::Any,
            crate::cli::SearchMode::All => Self::All,
        }
    }
}

impl From<&WireSearchMode> for crate::cli::SearchMode {
    fn from(m: &WireSearchMode) -> Self {
        match m {
            WireSearchMode::Any => Self::Any,
            WireSearchMode::All => Self::All,
        }
    }
}

impl From<&crate::Pair> for WireFilePair {
    fn from(p: &crate::Pair) -> Self {
        Self {
            file: p.file.to_string_lossy().into_owned(),
            tags: p.tags.clone(),
        }
    }
}

impl From<crate::Pair> for WireFilePair {
    fn from(p: crate::Pair) -> Self {
        Self {
            file: p.file.to_string_lossy().into_owned(),
            tags: p.tags,
        }
    }
}

impl From<&WireFilePair> for crate::Pair {
    fn from(w: &WireFilePair) -> Self {
        Self::new(w.file.clone().into(), w.tags.clone())
    }
}

impl From<WireFilePair> for crate::Pair {
    fn from(w: WireFilePair) -> Self {
        Self::new(w.file.into(), w.tags)
    }
}

impl From<&crate::ipc::TagInfo> for WireTagInfo {
    fn from(t: &crate::ipc::TagInfo) -> Self {
        Self {
            name: t.name.clone(),
            file_count: t.file_count as u64,
        }
    }
}

impl From<&WireTagInfo> for crate::ipc::TagInfo {
    #[allow(clippy::cast_possible_truncation)]
    fn from(w: &WireTagInfo) -> Self {
        Self {
            name: w.name.clone(),
            file_count: w.file_count as usize,
        }
    }
}

impl From<crate::cli::SearchParams> for WireSearchParams {
    fn from(p: crate::cli::SearchParams) -> Self {
        Self {
            tags: p.tags,
            file_patterns: p.file_patterns,
            virtual_tags: p.virtual_tags,
            exclude_tags: p.exclude_tags,
            regex_tag: p.regex_tag,
            regex_file: p.regex_file,
            glob_files: p.glob_files,
            no_hierarchy: p.no_hierarchy,
            query: p.query,
            tag_mode: WireSearchMode::from(&p.tag_mode),
            file_mode: WireSearchMode::from(&p.file_mode),
            virtual_mode: WireSearchMode::from(&p.virtual_mode),
        }
    }
}

impl From<WireSearchParams> for crate::cli::SearchParams {
    fn from(w: WireSearchParams) -> Self {
        Self {
            tags: w.tags,
            file_patterns: w.file_patterns,
            virtual_tags: w.virtual_tags,
            exclude_tags: w.exclude_tags,
            regex_tag: w.regex_tag,
            regex_file: w.regex_file,
            glob_files: w.glob_files,
            no_hierarchy: w.no_hierarchy,
            query: w.query,
            tag_mode: crate::cli::SearchMode::from(&w.tag_mode),
            file_mode: crate::cli::SearchMode::from(&w.file_mode),
            virtual_mode: crate::cli::SearchMode::from(&w.virtual_mode),
        }
    }
}

// ---------------------------------------------------------------------------
// Framed async I/O
// ---------------------------------------------------------------------------

/// Maximum allowed frame size (16 MiB) to prevent malformed length from
/// causing huge allocations.
const MAX_FRAME_SIZE: u32 = 16 * 1024 * 1024;

/// Write a length-prefixed wincode frame to an async writer.
///
/// # Errors
/// Returns an error if serialization or the write fails.
#[allow(clippy::future_not_send)]
pub async fn write_frame<W, T>(writer: &mut W, msg: &T) -> Result<(), IpcError>
where
    W: tokio::io::AsyncWriteExt + Unpin,
    T: SchemaWrite<wincode::config::DefaultConfig, Src = T> + ?Sized,
{
    let bytes = wincode::serialize(msg).map_err(|e| {
        IpcError::SerializationError(format!("wincode serialize: {e}"))
    })?;

    let len = u32::try_from(bytes.len()).map_err(|_| {
        IpcError::SerializationError("frame too large".into())
    })?;

    writer
        .write_all(&len.to_le_bytes())
        .await
        .map_err(|e| IpcError::ConnectionFailed(e.to_string()))?;

    writer
        .write_all(&bytes)
        .await
        .map_err(|e| IpcError::ConnectionFailed(e.to_string()))?;

    Ok(())
}

/// Read a length-prefixed wincode frame from an async reader.
///
/// Returns `None` on clean EOF (peer closed connection).
///
/// # Errors
/// Returns an error if the read or deserialization fails.
#[allow(clippy::future_not_send)]
pub async fn read_frame<R, T>(reader: &mut R) -> Result<Option<T>, IpcError>
where
    R: tokio::io::AsyncReadExt + Unpin,
    T: for<'a> SchemaRead<'a, wincode::config::DefaultConfig, Dst = T>,
{
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(IpcError::ConnectionFailed(e.to_string())),
    }

    let len = u32::from_le_bytes(len_buf);
    if len > MAX_FRAME_SIZE {
        return Err(IpcError::SerializationError(format!(
            "frame size {len} exceeds maximum {MAX_FRAME_SIZE}"
        )));
    }

    let mut buf = vec![0u8; len as usize];
    reader
        .read_exact(&mut buf)
        .await
        .map_err(|e| IpcError::ConnectionFailed(e.to_string()))?;

    let msg: T = wincode::deserialize(&buf).map_err(|e| {
        IpcError::SerializationError(format!("wincode deserialize: {e}"))
    })?;

    Ok(Some(msg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_round_trip() {
        let req = Request::AddTags {
            file: "/home/user/test.txt".into(),
            tags: vec!["rust".into(), "wasm".into()],
        };
        let bytes = wincode::serialize(&req).unwrap();
        let decoded: Request = wincode::deserialize(&bytes).unwrap();

        match decoded {
            Request::AddTags { file, tags } => {
                assert_eq!(file, "/home/user/test.txt");
                assert_eq!(tags, vec!["rust", "wasm"]);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_response_round_trip() {
        let resp = Response::Tags(vec![
            WireTagInfo {
                name: "rust".into(),
                file_count: 42,
            },
            WireTagInfo {
                name: "wasm".into(),
                file_count: 7,
            },
        ]);
        let bytes = wincode::serialize(&resp).unwrap();
        let decoded: Response = wincode::deserialize(&bytes).unwrap();

        match decoded {
            Response::Tags(tags) => {
                assert_eq!(tags.len(), 2);
                assert_eq!(tags[0].name, "rust");
                assert_eq!(tags[0].file_count, 42);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_client_message_round_trip() {
        let msg = ClientMessage::Request {
            id: 1,
            payload: Request::Ping,
        };
        let bytes = wincode::serialize(&msg).unwrap();
        let decoded: ClientMessage = wincode::deserialize(&bytes).unwrap();

        match decoded {
            ClientMessage::Request { id, payload } => {
                assert_eq!(id, 1);
                assert!(matches!(payload, Request::Ping));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_server_message_round_trip() {
        let msg = ServerMessage::Response {
            id: 42,
            payload: Response::Pong,
        };
        let bytes = wincode::serialize(&msg).unwrap();
        let decoded: ServerMessage = wincode::deserialize(&bytes).unwrap();

        match decoded {
            ServerMessage::Response { id, payload } => {
                assert_eq!(id, 42);
                assert!(matches!(payload, Response::Pong));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_server_event_round_trip() {
        let msg = ServerMessage::Event(ServerEvent::FileTagged {
            file: "/tmp/doc.md".into(),
            tags: vec!["notes".into()],
        });
        let bytes = wincode::serialize(&msg).unwrap();
        let decoded: ServerMessage = wincode::deserialize(&bytes).unwrap();

        match decoded {
            ServerMessage::Event(ServerEvent::FileTagged { file, tags }) => {
                assert_eq!(file, "/tmp/doc.md");
                assert_eq!(tags, vec!["notes"]);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_wire_search_params_conversion() {
        let domain = crate::cli::SearchParams {
            query: None,
            tags: vec!["rust".into()],
            file_patterns: vec!["*.rs".into()],
            exclude_tags: vec!["draft".into()],
            virtual_tags: vec![],
            tag_mode: crate::cli::SearchMode::All,
            file_mode: crate::cli::SearchMode::Any,
            virtual_mode: crate::cli::SearchMode::Any,
            no_hierarchy: true,
            regex_tag: false,
            regex_file: false,
            glob_files: false,
        };

        let wire: WireSearchParams = (&domain).into();
        let back: crate::cli::SearchParams = (&wire).into();

        assert_eq!(back.tags, domain.tags);
        assert_eq!(back.file_patterns, domain.file_patterns);
        assert_eq!(back.exclude_tags, domain.exclude_tags);
        assert_eq!(back.no_hierarchy, domain.no_hierarchy);
    }

    #[test]
    fn test_wire_file_pair_conversion() {
        let pair = crate::Pair::new("/tmp/test.txt".into(), vec!["a".into(), "b".into()]);
        let wire: WireFilePair = (&pair).into();
        let back: crate::Pair = (&wire).into();

        assert_eq!(back.file, pair.file);
        assert_eq!(back.tags, pair.tags);
    }

    #[tokio::test]
    async fn test_framed_io_round_trip() {
        let msg = ClientMessage::Request {
            id: 99,
            payload: Request::ListTags,
        };

        let mut buf: Vec<u8> = Vec::new();
        write_frame(&mut buf, &msg).await.unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let decoded: Option<ClientMessage> = read_frame(&mut cursor).await.unwrap();
        let decoded = decoded.expect("should decode frame");

        match decoded {
            ClientMessage::Request { id, payload } => {
                assert_eq!(id, 99);
                assert!(matches!(payload, Request::ListTags));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn test_framed_io_eof() {
        let mut cursor = std::io::Cursor::new(Vec::<u8>::new());
        let result: Option<ClientMessage> = read_frame(&mut cursor).await.unwrap();
        assert!(result.is_none(), "EOF should return None");
    }
}
