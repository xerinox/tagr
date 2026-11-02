use sqlx::FromRow;

use crate::store::ids::{FileId, TagId};
#[derive(Debug, Clone, FromRow)]
pub struct FileRecord {
    pub id: FileId,
    pub path: String,
    pub basename: String,
    pub extension: Option<String>,
    pub size_bytes: i64,
    pub mtime_epoch_ms: i64,
    pub file_exists: bool,
    pub last_seen_at_epoch_ms: i64
}

#[derive(Debug, Clone, FromRow)]
pub struct TagRecord {
    id: TagId,
    name: String,
    basename: String,
    parent_id: Option<TagId>,
    created_epoch_ms: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct TagSegmentRecord {
    tag_id: TagId,
    position: i64,
    segment: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct FileTagsRecord {
    file_id: FileId,
    tag_id: TagId,
}

#[derive(Debug, Clone, FromRow)]
pub struct TagAliasRecord {
    alias: String,
    canonical_tag_id: TagId
}


#[derive(Debug, Clone, FromRow)]
pub struct TagRelationRecord {
    ancestor_tag_id: TagId,
    descendant_tag_id: TagId,
    depth: i64,
}
