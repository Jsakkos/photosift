//! Schema steps applied after v1 (`schema.sql`, which is never edited). Add a
//! step by appending `(N, "sql")`; `migrate` runs every step above the
//! database's `user_version`, each in its own transaction.

pub const STEPS: &[(i64, &str)] = &[
    // NULL: enriched without a face detector. Otherwise the model stamp the
    // faces were produced with (`enrich::FACES_MODEL`).
    (2, "ALTER TABLE photos ADD COLUMN faces_model TEXT;"),
];
