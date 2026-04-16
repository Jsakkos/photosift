use sha2::{Digest, Sha256};
use std::io::{BufReader, Read};
use std::path::Path;

/// Stream SHA-256 hash of a file in 8 KB chunks. Does not load the entire file.
pub fn sha256_stream(path: &Path) -> std::io::Result<[u8; 32]> {
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::with_capacity(8192, file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().into())
}

pub fn hash_hex(hash: &[u8; 32]) -> String {
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}
