CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY NOT NULL,
    path TEXT NOT NULL,
    UNIQUE(path)
);

CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    UNIQUE(name)
);

CREATE TABLE IF NOT EXISTS tags_x_files (
    id INTEGER PRIMARY KEY NOT NULL,
    tag_id INTEGER NOT NULL,
    file_id INTEGER NOT NULL,
    FOREIGN KEY(tag_id) REFERENCES tags(id),
    FOREIGN KEY(file_id) REFERENCES files(id),
    UNIQUE(tag_id, file_id)
);
