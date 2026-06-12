//! Binary wire protocol for persistent bidirectional IPC.
//!
//! Uses postcard for serialization and length-prefixed
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
use crate::types::{TagName, TagrPath};

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
    Query {
        criteria: WireQueryCriteria,
    },
    GetTags {
        file: String,
    },
    GetNote {
        file: String,
    },
    FindByTag {
        tag: String,
    },
    FindByTags {
        tags: Vec<String>,
        match_all: bool,
    },
    FindByTagRegex {
        pattern: String,
    },
    ListNotes,

    // -- Mutations --
    AddTags {
        file: String,
        tags: Vec<String>,
    },
    SetTags {
        file: String,
        tags: Vec<String>,
    },
    RemoveTags {
        file: String,
        tags: Vec<String>,
        all: bool,
    },
    SetNote {
        file: String,
        content: String,
    },
    DeleteNote {
        file: String,
    },
    DeleteFromDb {
        file: String,
    },
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
    CleanupResult {
        removed: u64,
    },
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
    NoteChanged {
        file: String,
        content: Option<String>,
    },
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

/// Wire-safe match mode.
#[derive(Debug, Clone, SchemaWrite, SchemaRead)]
pub enum WireMatchMode {
    All,
    Any,
}

/// Wire-safe tag expression tree.
///
/// Uses `Vec` instead of `Box` for `Not` because wincode's derive macros
/// don't support recursive types through `Box`.
#[derive(Debug, Clone, SchemaWrite, SchemaRead)]
#[allow(clippy::use_self)]
pub enum WireTagExpr {
    Tag(String),
    Not(Vec<WireTagExpr>),
    And(Vec<WireTagExpr>),
    Or(Vec<WireTagExpr>),
}

/// Query criteria over the wire — mirrors `types::QueryCriteria`.
#[derive(Debug, Clone, SchemaWrite, SchemaRead)]
#[allow(clippy::struct_excessive_bools)]
pub struct WireQueryCriteria {
    pub tag_expr: Option<WireTagExpr>,
    pub regex_tags: bool,
    pub expand_hierarchy: bool,
    pub file_patterns: Vec<String>,
    pub file_mode: WireMatchMode,
    pub regex_files: bool,
    pub virtual_tags: Vec<String>,
    pub virtual_mode: WireMatchMode,
    pub query: Option<String>,
}

// ---------------------------------------------------------------------------
// Conversions between wire types and domain types
// ---------------------------------------------------------------------------

impl From<&crate::Pair> for WireFilePair {
    fn from(p: &crate::Pair) -> Self {
        Self {
            file: p.file.as_str().to_owned(),
            tags: p.tags.iter().map(|t| t.as_str().to_owned()).collect(),
        }
    }
}

impl From<crate::Pair> for WireFilePair {
    fn from(p: crate::Pair) -> Self {
        Self {
            file: p.file.into_inner(),
            tags: p.tags.into_iter().map(TagName::into_inner).collect(),
        }
    }
}

impl From<&WireFilePair> for crate::Pair {
    fn from(w: &WireFilePair) -> Self {
        Self::new(
            TagrPath::from_string(w.file.clone()),
            w.tags.iter().filter_map(|s| TagName::new(s).ok()).collect(),
        )
    }
}

impl From<WireFilePair> for crate::Pair {
    fn from(w: WireFilePair) -> Self {
        Self::new(
            TagrPath::from_string(w.file),
            w.tags
                .into_iter()
                .filter_map(|s| TagName::new(&s).ok())
                .collect(),
        )
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

// ---------------------------------------------------------------------------
// QueryCriteria <-> WireQueryCriteria conversions
// ---------------------------------------------------------------------------

impl From<&crate::types::MatchMode> for WireMatchMode {
    fn from(m: &crate::types::MatchMode) -> Self {
        match m {
            crate::types::MatchMode::All => Self::All,
            crate::types::MatchMode::Any => Self::Any,
        }
    }
}

impl From<&WireMatchMode> for crate::types::MatchMode {
    fn from(m: &WireMatchMode) -> Self {
        match m {
            WireMatchMode::All => Self::All,
            WireMatchMode::Any => Self::Any,
        }
    }
}

impl From<&crate::types::TagExpr> for WireTagExpr {
    fn from(expr: &crate::types::TagExpr) -> Self {
        match expr {
            crate::types::TagExpr::Tag(t) => Self::Tag(t.to_string()),
            crate::types::TagExpr::Not(inner) => Self::Not(vec![Self::from(inner.as_ref())]),
            crate::types::TagExpr::And(exprs) => Self::And(exprs.iter().map(Self::from).collect()),
            crate::types::TagExpr::Or(exprs) => Self::Or(exprs.iter().map(Self::from).collect()),
        }
    }
}

impl TryFrom<&WireTagExpr> for crate::types::TagExpr {
    type Error = crate::types::ValidationError;
    fn try_from(w: &WireTagExpr) -> Result<Self, Self::Error> {
        match w {
            WireTagExpr::Tag(s) => Ok(Self::Tag(crate::types::TagName::new(s)?)),
            WireTagExpr::Not(inner) => {
                let first = inner.first().ok_or(crate::types::ValidationError::Empty {
                    kind: crate::types::NameKind::Tag,
                })?;
                Ok(Self::Not(Box::new(Self::try_from(first)?)))
            }
            WireTagExpr::And(exprs) => Ok(Self::And(
                exprs.iter().map(Self::try_from).collect::<Result<_, _>>()?,
            )),
            WireTagExpr::Or(exprs) => Ok(Self::Or(
                exprs.iter().map(Self::try_from).collect::<Result<_, _>>()?,
            )),
        }
    }
}

impl From<&crate::types::QueryCriteria> for WireQueryCriteria {
    fn from(qc: &crate::types::QueryCriteria) -> Self {
        Self {
            tag_expr: qc.tag_expr.as_ref().map(WireTagExpr::from),
            regex_tags: qc.regex_tags,
            expand_hierarchy: qc.expand_hierarchy,
            file_patterns: qc.file_patterns.clone(),
            file_mode: WireMatchMode::from(&qc.file_mode),
            regex_files: qc.regex_files,
            virtual_tags: qc.virtual_tags.clone(),
            virtual_mode: WireMatchMode::from(&qc.virtual_mode),
            query: qc.query.clone(),
        }
    }
}

/// Convert a `WireTagExpr` to `TagExpr` using `from_raw` (no validation).
/// Used when `regex_tags` is true — patterns contain regex metacharacters.
fn wire_tag_expr_to_raw(w: &WireTagExpr) -> crate::types::TagExpr {
    match w {
        WireTagExpr::Tag(s) => {
            crate::types::TagExpr::Tag(crate::types::TagName::from_raw(s.clone()))
        }
        WireTagExpr::Not(inner) => {
            let first = inner.first().map_or_else(
                || crate::types::TagExpr::Tag(crate::types::TagName::from_raw(String::new())),
                wire_tag_expr_to_raw,
            );
            crate::types::TagExpr::Not(Box::new(first))
        }
        WireTagExpr::And(exprs) => {
            crate::types::TagExpr::And(exprs.iter().map(wire_tag_expr_to_raw).collect())
        }
        WireTagExpr::Or(exprs) => {
            crate::types::TagExpr::Or(exprs.iter().map(wire_tag_expr_to_raw).collect())
        }
    }
}

impl TryFrom<&WireQueryCriteria> for crate::types::QueryCriteria {
    type Error = crate::types::ValidationError;
    fn try_from(w: &WireQueryCriteria) -> Result<Self, Self::Error> {
        let tag_expr = w
            .tag_expr
            .as_ref()
            .map(|expr| {
                if w.regex_tags {
                    Ok(wire_tag_expr_to_raw(expr))
                } else {
                    crate::types::TagExpr::try_from(expr)
                }
            })
            .transpose()?;
        Ok(Self {
            tag_expr,
            regex_tags: w.regex_tags,
            expand_hierarchy: w.expand_hierarchy,
            file_patterns: w.file_patterns.clone(),
            file_mode: crate::types::MatchMode::from(&w.file_mode),
            regex_files: w.regex_files,
            virtual_tags: w.virtual_tags.clone(),
            virtual_mode: crate::types::MatchMode::from(&w.virtual_mode),
            query: w.query.clone(),
        })
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
    let bytes = wincode::serialize(msg)
        .map_err(|e| IpcError::SerializationError(format!("wincode serialize: {e}")))?;

    let len = u32::try_from(bytes.len())
        .map_err(|_| IpcError::SerializationError("frame too large".into()))?;

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

    let msg: T = wincode::deserialize(&buf)
        .map_err(|e| IpcError::SerializationError(format!("wincode deserialize: {e}")))?;

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
            ServerMessage::Event(_) => panic!("wrong variant"),
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
    fn test_wire_file_pair_conversion() {
        let pair = crate::Pair::new(
            TagrPath::from_string("/tmp/test.txt".to_owned()),
            vec![TagName::new("a").unwrap(), TagName::new("b").unwrap()],
        );
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

    #[test]
    fn test_wire_query_criteria_round_trip() {
        use crate::types::{MatchMode, QueryCriteria, TagExpr, TagName};

        let criteria = QueryCriteria {
            tag_expr: Some(TagExpr::And(vec![
                TagExpr::Tag(TagName::new("rust").unwrap()),
                TagExpr::Not(Box::new(TagExpr::Tag(TagName::new("draft").unwrap()))),
            ])),
            regex_tags: false,
            expand_hierarchy: true,
            file_patterns: vec!["*.rs".to_string()],
            file_mode: MatchMode::Any,
            regex_files: false,
            virtual_tags: vec!["size:large".to_string()],
            virtual_mode: MatchMode::All,
            query: Some("search term".to_string()),
        };

        let wire = WireQueryCriteria::from(&criteria);
        let bytes = wincode::serialize(&wire).unwrap();
        let decoded: WireQueryCriteria = wincode::deserialize(&bytes).unwrap();
        let back = QueryCriteria::try_from(&decoded).unwrap();

        assert_eq!(back, criteria);
    }

    #[test]
    fn test_request_query_round_trip() {
        let req = Request::Query {
            criteria: WireQueryCriteria {
                tag_expr: Some(WireTagExpr::Tag("rust".into())),
                regex_tags: false,
                expand_hierarchy: true,
                file_patterns: vec![],
                file_mode: WireMatchMode::All,
                regex_files: false,
                virtual_tags: vec![],
                virtual_mode: WireMatchMode::All,
                query: None,
            },
        };
        let bytes = wincode::serialize(&req).unwrap();
        let decoded: Request = wincode::deserialize(&bytes).unwrap();
        match decoded {
            Request::Query { criteria } => {
                assert!(matches!(criteria.tag_expr, Some(WireTagExpr::Tag(ref s)) if s == "rust"));
            }
            _ => panic!("wrong variant"),
        }
    }
}
