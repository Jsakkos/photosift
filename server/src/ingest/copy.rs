//! Verified file copy into the library. A file only ever appears under its
//! final name after its bytes were written, re-read from disk and matched
//! against the hash of what was written, so a crash or bad sector can never
//! leave a half-copied photo that looks complete.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use sha2::{Digest, Sha256};

const CHUNK: usize = 1024 * 1024;
const PARTIAL_SUFFIX: &str = ".partial";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopiedFile {
    pub dest: PathBuf,
    /// Lowercase hex SHA-256 of the bytes written (= of the source).
    pub sha256: String,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(digest: &[u8]) -> String {
    use std::fmt::Write as _;
    digest.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Streams the file through SHA-256 so large videos never sit in memory.
pub fn hash_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hex(&hasher.finalize()));
        }
        hasher.update(&buf[..n]);
    }
}

/// Copies `src` into `dest_dir` (which must exist) and verifies the result.
/// `stem_override` replaces the file stem; the extension keeps the source's
/// spelling. Without an override, a name collision picks the lowest free
/// `_{n}` suffix. With one, the caller already claimed the name (see
/// [`NameClaims`]): an identical file already there counts as success, a
/// different one is an `AlreadyExists` error rather than being overwritten.
pub fn copy_verified(
    src: &Path,
    dest_dir: &Path,
    stem_override: Option<&str>,
) -> io::Result<CopiedFile> {
    copy_verified_expecting(src, dest_dir, stem_override, None)
}

/// As [`copy_verified`], but fails if the bytes read from `src` do not hash to
/// `expected`; guards against a card that changed between the dedupe hash and
/// the copy.
pub fn copy_verified_expecting(
    src: &Path,
    dest_dir: &Path,
    stem_override: Option<&str>,
    expected: Option<&str>,
) -> io::Result<CopiedFile> {
    let name = file_name_of(src)?;
    let target = match stem_override {
        Some(stem) => with_stem(&name, stem),
        None => {
            let sha = match expected {
                Some(e) => e.to_string(),
                None => hash_file(src)?,
            };
            let member = Member::of(&name, &sha);
            let n = free_suffix(dest_dir, &[member], &|_| false);
            with_stem(&name, &suffixed(&member_stem(&name), n))
        }
    };
    let dest = dest_dir.join(&target);
    let partial = partial_path(&dest);

    let mut reader = File::open(src)?;
    let mut out = File::create(&partial)?; // truncates a stale partial from a crashed run
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let written = (|| -> io::Result<String> {
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            out.write_all(&buf[..n])?;
        }
        out.sync_all()?;
        Ok(hex(&hasher.finalize_reset()))
    })();
    drop(out);
    let sha256 = match written {
        Ok(sha) => sha,
        Err(e) => {
            let _ = fs::remove_file(&partial);
            return Err(e);
        }
    };
    if let Some(expected) = expected {
        if sha256 != expected {
            let _ = fs::remove_file(&partial);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} changed while it was being copied", src.display()),
            ));
        }
    }
    publish(&partial, &dest, &sha256)?;
    Ok(CopiedFile { dest, sha256 })
}

/// In-memory counterpart of [`copy_verified`] for files the import job has
/// already read (photos are read once and reused for hashing, EXIF and
/// previews). `src_name` supplies the stem and extension.
pub fn write_verified(
    bytes: &[u8],
    src_name: &str,
    dest_dir: &Path,
    stem_override: Option<&str>,
) -> io::Result<CopiedFile> {
    let sha256 = sha256_hex(bytes);
    let target = match stem_override {
        Some(stem) => with_stem(src_name, stem),
        None => {
            let n = free_suffix(dest_dir, &[Member::of(src_name, &sha256)], &|_| false);
            with_stem(src_name, &suffixed(&member_stem(src_name), n))
        }
    };
    let dest = dest_dir.join(target);
    let partial = partial_path(&dest);
    let result = (|| {
        let mut out = File::create(&partial)?;
        out.write_all(bytes)?;
        out.sync_all()
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&partial);
        return Err(e);
    }
    publish(&partial, &dest, &sha256)?;
    Ok(CopiedFile { dest, sha256 })
}

/// Re-reads the partial from disk, and only if it matches `expected` moves it
/// to `dest`. An identical file already at `dest` is kept (a resumed import).
fn publish(partial: &Path, dest: &Path, expected: &str) -> io::Result<()> {
    let verdict = verify_then_place(partial, dest, expected);
    // A successful rename already consumed the partial; every other outcome
    // (mismatch, refusal, adopting an identical file) must not leave it behind.
    let _ = fs::remove_file(partial);
    verdict
}

fn verify_then_place(partial: &Path, dest: &Path, expected: &str) -> io::Result<()> {
    let on_disk = hash_file(partial)?;
    if on_disk != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "verification failed for {}: wrote {expected}, read back {on_disk}",
                dest.display()
            ),
        ));
    }
    if dest.exists() {
        // std's rename silently replaces on Windows, so decide first.
        return if hash_file(dest)? == expected {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} exists with different content", dest.display()),
            ))
        };
    }
    fs::rename(partial, dest)
}

fn partial_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(PARTIAL_SUFFIX);
    dest.with_file_name(name)
}

/// Removes `*.partial` files a crashed run left in `dir`. Only safe while no
/// import is writing there, which the job registry guarantees.
pub fn sweep_partials(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name
            .to_string_lossy()
            .to_ascii_lowercase()
            .ends_with(PARTIAL_SUFFIX)
        {
            let _ = fs::remove_file(entry.path());
        }
    }
}

// --- naming -----------------------------------------------------------------

fn file_name_of(path: &Path) -> io::Result<String> {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))
}

fn member_stem(name: &str) -> String {
    Path::new(name)
        .file_stem()
        .map(OsStr::to_string_lossy)
        .map(|s| s.into_owned())
        .unwrap_or_default()
}

fn member_ext(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn with_stem(name: &str, stem: &str) -> String {
    match member_ext(name).as_str() {
        "" => stem.to_string(),
        ext => format!("{stem}.{ext}"),
    }
}

/// `n == 0` is the original stem; `n >= 1` appends `_{n}`.
fn suffixed(stem: &str, n: u32) -> String {
    if n == 0 {
        stem.to_string()
    } else {
        format!("{stem}_{n}")
    }
}

/// One file of a frame that must share a collision suffix with its siblings.
#[derive(Debug, Clone, Copy)]
pub struct Member<'a> {
    pub name: &'a str,
    pub sha256: &'a str,
}

impl<'a> Member<'a> {
    pub fn of(name: &'a str, sha256: &'a str) -> Self {
        Self { name, sha256 }
    }
}

enum Slot {
    Free,
    /// The file is already there with exactly these bytes (resumed import).
    Same,
    Different,
}

fn slot(path: &Path, sha256: &str) -> Slot {
    if !path.exists() {
        return Slot::Free;
    }
    match hash_file(path) {
        Ok(h) if h == sha256 => Slot::Same,
        _ => Slot::Different,
    }
}

/// Lowest suffix `n` at which every member's file and the first member's
/// `.xmp` sidecar name are usable. A member slot is usable when free or when it
/// already holds identical bytes. The sidecar name must be free, unless a
/// member already holds identical bytes (it is then that frame's own sidecar).
fn free_suffix(dir: &Path, members: &[Member], reserved: &dyn Fn(&Path) -> bool) -> u32 {
    let primary_stem = member_stem(members[0].name);
    for n in 0.. {
        let mut usable = true;
        let mut resuming = false;
        let mut paths = Vec::with_capacity(members.len() + 1);
        for m in members {
            let path = dir.join(with_stem(m.name, &suffixed(&member_stem(m.name), n)));
            match slot(&path, m.sha256) {
                Slot::Free => {}
                Slot::Same => resuming = true,
                Slot::Different => usable = false,
            }
            paths.push(path);
        }
        let xmp = dir.join(format!("{}.xmp", suffixed(&primary_stem, n)));
        if xmp.exists() && !resuming {
            usable = false;
        }
        paths.push(xmp);
        if usable && !paths.iter().any(|p| reserved(p)) {
            return n;
        }
    }
    unreachable!("u32 suffix space exhausted")
}

/// Hands out collision suffixes to concurrent workers. Two cards folders can
/// both hold `DSCF0001.JPG`; without a shared reservation, both workers would
/// see the name as free and one would clobber the other.
#[derive(Default)]
pub struct NameClaims {
    reserved: Mutex<HashSet<String>>,
}

fn key(path: &Path) -> String {
    // Windows paths are case-insensitive.
    path.to_string_lossy().to_lowercase()
}

impl NameClaims {
    /// Picks and reserves the lowest suffix usable by all `members` plus the
    /// frame's `.xmp`. Returns the suffix (0 = no suffix); apply it to each
    /// member's own stem with [`suffixed_stem`].
    pub fn claim(&self, dir: &Path, members: &[Member]) -> u32 {
        let mut reserved = self.reserved.lock().unwrap_or_else(|e| e.into_inner());
        let n = free_suffix(dir, members, &|p| reserved.contains(&key(p)));
        for m in members {
            let name = with_stem(m.name, &suffixed(&member_stem(m.name), n));
            reserved.insert(key(&dir.join(name)));
        }
        let xmp = format!("{}.xmp", suffixed(&member_stem(members[0].name), n));
        reserved.insert(key(&dir.join(xmp)));
        n
    }
}

/// The stem `name` gets when its frame was assigned collision suffix `n`.
pub fn suffixed_stem(name: &str, n: u32) -> String {
    suffixed(&member_stem(name), n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_verified_copies_hashes_and_leaves_no_partial() {
        let src_dir = tempfile::tempdir().unwrap();
        let dest_dir = tempfile::tempdir().unwrap();
        let src = src_dir.path().join("CLIP0001.MOV");
        let payload = vec![7u8; CHUNK + 123]; // spans a chunk boundary
        fs::write(&src, &payload).unwrap();

        let copied = copy_verified(&src, dest_dir.path(), None).unwrap();

        assert_eq!(copied.dest, dest_dir.path().join("CLIP0001.MOV"));
        assert_eq!(copied.sha256, sha256_hex(&payload));
        assert_eq!(fs::read(&copied.dest).unwrap(), payload);
        let names: Vec<_> = fs::read_dir(dest_dir.path()).unwrap().collect();
        assert_eq!(names.len(), 1, "no .partial left behind");
    }

    #[test]
    fn copy_verified_picks_the_lowest_free_suffix_on_collision() {
        let src_dir = tempfile::tempdir().unwrap();
        let dest_dir = tempfile::tempdir().unwrap();
        let src = src_dir.path().join("A.JPG");
        fs::write(&src, b"mine").unwrap();
        fs::write(dest_dir.path().join("A.JPG"), b"theirs").unwrap();
        fs::write(dest_dir.path().join("A_1.JPG"), b"theirs too").unwrap();

        let copied = copy_verified(&src, dest_dir.path(), None).unwrap();

        assert_eq!(copied.dest, dest_dir.path().join("A_2.JPG"));
        assert_eq!(fs::read(dest_dir.path().join("A.JPG")).unwrap(), b"theirs");
    }

    #[test]
    fn copy_verified_honours_a_stem_override_and_overwrites_a_stale_partial() {
        let src_dir = tempfile::tempdir().unwrap();
        let dest_dir = tempfile::tempdir().unwrap();
        let src = src_dir.path().join("A.RAF");
        fs::write(&src, b"raw bytes").unwrap();
        fs::write(
            dest_dir.path().join("A_3.RAF.partial"),
            b"stale junk, longer",
        )
        .unwrap();

        let copied = copy_verified(&src, dest_dir.path(), Some("A_3")).unwrap();

        assert_eq!(copied.dest, dest_dir.path().join("A_3.RAF"));
        assert_eq!(fs::read(&copied.dest).unwrap(), b"raw bytes");
        assert!(!dest_dir.path().join("A_3.RAF.partial").exists());
    }

    #[test]
    fn copy_verified_rejects_a_source_that_is_not_what_was_hashed() {
        let src_dir = tempfile::tempdir().unwrap();
        let dest_dir = tempfile::tempdir().unwrap();
        let src = src_dir.path().join("A.MOV");
        fs::write(&src, b"changed since hashing").unwrap();

        let err = copy_verified_expecting(
            &src,
            dest_dir.path(),
            None,
            Some(&sha256_hex(b"what the dedupe pass saw")),
        )
        .unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(fs::read_dir(dest_dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn write_verified_writes_and_names_like_copy_verified() {
        let dest_dir = tempfile::tempdir().unwrap();
        fs::write(dest_dir.path().join("A.JPG"), b"theirs").unwrap();

        let copied = write_verified(b"mine", "A.JPG", dest_dir.path(), None).unwrap();

        assert_eq!(copied.dest, dest_dir.path().join("A_1.JPG"));
        assert_eq!(copied.sha256, sha256_hex(b"mine"));
        assert_eq!(fs::read_dir(dest_dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn sweep_partials_removes_only_partials() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.jpg"), b"keep").unwrap();
        fs::write(dir.path().join("a.jpg.partial"), b"x").unwrap();
        fs::write(dir.path().join("B.MOV.PARTIAL"), b"x").unwrap();

        sweep_partials(dir.path());

        let left: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, ["a.jpg"]);
    }

    #[test]
    fn publish_refuses_a_partial_that_does_not_match() {
        let dir = tempfile::tempdir().unwrap();
        let partial = dir.path().join("a.jpg.partial");
        let dest = dir.path().join("a.jpg");
        fs::write(&partial, b"corrupted on the way to disk").unwrap();

        let err = publish(&partial, &dest, &sha256_hex(b"what we meant to write")).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(
            !dest.exists(),
            "unverified bytes must never get a final name"
        );
        assert!(!partial.exists(), "failed partial is cleaned up");
    }

    #[test]
    fn publish_never_overwrites_a_different_file() {
        let dir = tempfile::tempdir().unwrap();
        let partial = dir.path().join("a.jpg.partial");
        let dest = dir.path().join("a.jpg");
        fs::write(&partial, b"new").unwrap();
        fs::write(&dest, b"somebody else's").unwrap();

        let err = publish(&partial, &dest, &sha256_hex(b"new")).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&dest).unwrap(), b"somebody else's");
        assert!(!partial.exists());
    }

    #[test]
    fn publish_adopts_an_identical_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let partial = dir.path().join("a.jpg.partial");
        let dest = dir.path().join("a.jpg");
        fs::write(&partial, b"same").unwrap();
        fs::write(&dest, b"same").unwrap();

        publish(&partial, &dest, &sha256_hex(b"same")).unwrap();

        assert_eq!(fs::read(&dest).unwrap(), b"same");
        assert!(!partial.exists());
    }

    #[test]
    fn claims_never_hand_the_same_name_out_twice() {
        let dir = tempfile::tempdir().unwrap();
        let claims = NameClaims::default();
        let a = claims.claim(dir.path(), &[Member::of("DSCF0001.JPG", "aa")]);
        let b = claims.claim(dir.path(), &[Member::of("DSCF0001.JPG", "bb")]);
        let c = claims.claim(dir.path(), &[Member::of("DSCF0001.JPG", "cc")]);
        assert_eq!((a, b, c), (0, 1, 2));
    }

    #[test]
    fn a_pair_shares_one_suffix() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("A.JPG"), b"other jpg").unwrap();
        let claims = NameClaims::default();

        // RAW slot is free but the JPEG slot is taken: the pair moves together.
        let n = claims.claim(
            dir.path(),
            &[Member::of("A.RAF", "raw"), Member::of("A.JPG", "jpg")],
        );
        assert_eq!(n, 1);
    }

    #[test]
    fn a_claimed_frame_reserves_its_sidecar_name() {
        let dir = tempfile::tempdir().unwrap();
        let claims = NameClaims::default();
        let pair = claims.claim(
            dir.path(),
            &[Member::of("A.RAF", "raw"), Member::of("A.JPG", "jpg")],
        );
        // A same-stem extra JPEG is a different photo; it must not share A.xmp.
        let extra = claims.claim(dir.path(), &[Member::of("A.JPEG", "extra")]);
        assert_eq!((pair, extra), (0, 1));
    }

    #[test]
    fn existing_sidecar_forces_a_new_name() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("A.xmp"), b"<xmp/>").unwrap();
        let n = free_suffix(dir.path(), &[Member::of("A.RAF", "raw")], &|_| false);
        assert_eq!(n, 1);
    }
}
