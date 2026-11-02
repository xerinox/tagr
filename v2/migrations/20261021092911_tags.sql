CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    basename TEXT NOT NULL,
    extension TEXT,
    size_bytes INTEGER NOT NULL DEFAULT 0,
    mtime_epoch_ms INTEGER NOT NULL DEFAULT 0,
    file_exists BOOLEAN NOT NULL DEFAULT 1,
    last_seen_at_epoch_ms INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    basename TEXT NOT NULL COLLATE NOCASE,
    parent_id INTEGER NULL,
    created_epoch_ms INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (parent_id) REFERENCES tags(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS tag_segments (
    tag_id INTEGER NOT NULL,
    segment TEXT NOT NULL COLLATE NOCASE,
    position INTEGER NOT NULL,
    PRIMARY KEY (tag_id, position),
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS file_tags (
    file_id INTEGER NOT NULL,
    tag_id INTEGER NOT NULL,
    PRIMARY KEY (file_id, tag_id),
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS tag_aliases (
    alias TEXT NOT NULL UNIQUE COLLATE NOCASE,
    canonical_tag_id INTEGER NOT NULL,
    FOREIGN KEY (canonical_tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS tag_relations (
    ancestor_tag_id INTEGER NOT NULL,
    descendant_tag_id INTEGER NOT NULL,
    depth INTEGER NOT NULL,
    PRIMARY KEY (ancestor_tag_id, descendant_tag_id),
    FOREIGN KEY (ancestor_tag_id) REFERENCES tags(id) ON DELETE CASCADE,
    FOREIGN KEY (descendant_tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

CREATE INDEX idx_files_path ON files(path);
CREATE INDEX idx_files_basename ON files(basename);
CREATE INDEX idx_files_mtime ON files(mtime_epoch_ms);
CREATE INDEX idx_files_size ON files(size_bytes);
CREATE INDEX idx_files_exists ON files(file_exists);

CREATE INDEX idx_tags_parent_id ON tags(parent_id);
CREATE INDEX idx_tags_basename ON tags(basename);
CREATE INDEX idx_tags_name ON tags(name);

CREATE INDEX idx_tag_segments_segment ON tag_segments(segment);
CREATE INDEX idx_tag_segments_tag_id ON tag_segments(tag_id);

CREATE INDEX idx_file_tags_tag_id ON file_tags(tag_id);
CREATE INDEX idx_file_tags_file_id ON file_tags(file_id);

CREATE INDEX idx_tag_aliases_canonical ON tag_aliases(canonical_tag_id);
CREATE INDEX idx_tag_relations_ancestor ON tag_relations(ancestor_tag_id);
CREATE INDEX idx_tag_relations_descendant ON tag_relations(descendant_tag_id);


