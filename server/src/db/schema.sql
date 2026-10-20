-- Schema version 1. Applied once by db::migrate (keyed on PRAGMA user_version).
-- Add later changes as a new migration step; never edit this file after release.

CREATE TABLE shoots (
    id         INTEGER PRIMARY KEY,
    kind       TEXT NOT NULL CHECK (kind IN ('named', 'everyday')),
    name       TEXT,
    folder     TEXT NOT NULL UNIQUE,
    date_start TEXT,                 -- YYYY-MM-DD
    date_end   TEXT,                 -- YYYY-MM-DD
    created_at TEXT NOT NULL         -- %Y-%m-%dT%H:%M:%S, local time
);

CREATE TABLE photos (
    id                INTEGER PRIMARY KEY,
    shoot_id          INTEGER NOT NULL REFERENCES shoots(id),
    raw_path          TEXT,
    jpeg_path         TEXT,
    video_path        TEXT,
    sha256            TEXT NOT NULL UNIQUE,
    camera            TEXT,
    capture_time_orig TEXT,          -- as read from EXIF; %Y-%m-%dT%H:%M:%S
    capture_time      TEXT,          -- after per-camera offset
    time_source       TEXT CHECK (time_source IN ('exif', 'mtime')),
    orientation       INTEGER,
    -- width/height are the DISPLAY (post-orientation) dimensions, so a portrait
    -- shot with orientation 6 stores width < height. Layout code relies on this.
    width             INTEGER,
    height            INTEGER,
    -- 64-bit pHash stored bit-for-bit in a signed INTEGER (u64 <-> i64 cast).
    phash             INTEGER,
    sharpness         REAL,
    face_sharpness    REAL,
    mean_luma         REAL,
    clip_hi           REAL,
    clip_lo           REAL,
    moment_idx        INTEGER,
    burst_idx         INTEGER,
    signal_score      REAL,
    outcome           TEXT NOT NULL DEFAULT 'unreviewed'
        CHECK (outcome IN ('unreviewed', 'album', 'library', 'archive', 'delete')),
    blooper           INTEGER NOT NULL DEFAULT 0 CHECK (blooper IN (0, 1)),
    enriched_at       TEXT,
    trashed_at        TEXT
);

CREATE INDEX photos_shoot_time ON photos(shoot_id, capture_time);

-- Every copied file (RAW, sibling JPEG, MOV) for global content-hash dedupe.
-- name+size let a card scan skip already-imported files without hashing them.
-- photo_id is NULL for files that belong to no photo row (e.g. videos).
CREATE TABLE files (
    sha256   TEXT PRIMARY KEY,
    path     TEXT NOT NULL,
    name     TEXT NOT NULL,
    size     INTEGER NOT NULL,
    photo_id INTEGER REFERENCES photos(id) ON DELETE CASCADE
);

CREATE INDEX files_name_size ON files(name, size);
CREATE INDEX files_photo ON files(photo_id);

-- Boxes are normalized 0-1 in display orientation.
CREATE TABLE faces (
    id        INTEGER PRIMARY KEY,
    photo_id  INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    x         REAL NOT NULL,
    y         REAL NOT NULL,
    w         REAL NOT NULL,
    h         REAL NOT NULL,
    score     REAL,
    landmarks TEXT,
    embedding BLOB,
    sharpness REAL
);

CREATE INDEX faces_photo ON faces(photo_id);

CREATE TABLE camera_offsets (
    shoot_id       INTEGER NOT NULL REFERENCES shoots(id) ON DELETE CASCADE,
    camera         TEXT NOT NULL,
    offset_seconds INTEGER NOT NULL,
    PRIMARY KEY (shoot_id, camera)
);
