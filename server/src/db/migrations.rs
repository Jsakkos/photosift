//! Schema steps applied after v1 (`schema.sql`, which is never edited). Add a
//! step by appending `(N, "sql")`; `migrate` runs every step above the
//! database's `user_version`, each in its own transaction.

pub const STEPS: &[(i64, &str)] = &[
    // NULL: enriched without a face detector. Otherwise the model stamp the
    // faces were produced with (`enrich::FACES_MODEL`).
    (2, "ALTER TABLE photos ADD COLUMN faces_model TEXT;"),
    // The Claude helper. It writes only helper_runs, proposals and
    // your_call_pairs; decisions record what the user then chose.
    (
        3,
        "CREATE TABLE helper_runs (
            id INTEGER PRIMARY KEY,
            shoot_id INT NOT NULL REFERENCES shoots,
            kind TEXT CHECK(kind IN ('background','chat','external','eval')),
            status TEXT CHECK(status IN ('queued','running','done','failed','interrupted')),
            session_id TEXT,
            model TEXT,
            prompt_version TEXT,
            started_at TEXT,
            finished_at TEXT,
            summary TEXT,
            error TEXT,
            cost_usd REAL,
            input_tokens INT,
            output_tokens INT,
            cache_read_tokens INT,
            num_turns INT
        );
        CREATE INDEX helper_runs_shoot ON helper_runs(shoot_id, id);
        -- The latest proposal per photo; a newer run overwrites it.
        CREATE TABLE proposals (
            photo_id INTEGER PRIMARY KEY REFERENCES photos ON DELETE CASCADE,
            run_id INT NOT NULL REFERENCES helper_runs,
            outcome TEXT NOT NULL CHECK(outcome IN ('album','library','archive','delete')),
            blooper INT NOT NULL DEFAULT 0,
            reason TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE TABLE your_call_pairs (
            id INTEGER PRIMARY KEY,
            run_id INT NOT NULL,
            shoot_id INT NOT NULL,
            photo_a INT NOT NULL,
            photo_b INT NOT NULL,
            tradeoff TEXT NOT NULL,
            UNIQUE(photo_a, photo_b)
        );
        CREATE INDEX your_call_pairs_shoot ON your_call_pairs(shoot_id);
        CREATE TABLE decisions (
            photo_id INT NOT NULL,
            run_id INT NOT NULL,
            proposed TEXT NOT NULL,
            proposed_reason TEXT NOT NULL,
            chosen TEXT NOT NULL,
            criterion TEXT CHECK(criterion IN ('expression','who','moment','sharpness','duplicate','composition','other')),
            note TEXT,
            decided_at TEXT NOT NULL,
            PRIMARY KEY(photo_id, run_id)
        );
        CREATE TABLE taste (
            id INTEGER PRIMARY KEY CHECK(id = 1),
            text TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE chat_messages (
            id INTEGER PRIMARY KEY,
            shoot_id INT NOT NULL,
            role TEXT CHECK(role IN ('user','helper')),
            text TEXT NOT NULL,
            at TEXT NOT NULL
        );",
    ),
    // Automatic helper runs waiting for the worker, so a restart does not
    // lose them; `retry` marks one re-queued for a run a restart interrupted.
    // `helper_runs.retry` marks a run that was itself that retry: it is not
    // retried again.
    (
        4,
        "CREATE TABLE helper_pending (
            shoot_id INTEGER PRIMARY KEY REFERENCES shoots ON DELETE CASCADE,
            queued_at TEXT NOT NULL,
            retry INTEGER NOT NULL DEFAULT 0
        );
        ALTER TABLE helper_runs ADD COLUMN retry INTEGER NOT NULL DEFAULT 0;",
    ),
];
