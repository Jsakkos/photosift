//! Single-instance support for the localhost server. The port bind is the
//! lock: if it fails, we ask whatever holds the port whether it is PhotoSift.

use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    time::Duration,
};

const PROBE_TIMEOUT: Duration = Duration::from_millis(1000);
const LOG_MAX_BYTES: u64 = 5 * 1024 * 1024;

/// Keeps one previous generation: a log over `max_bytes` becomes `<name>.1`
/// (replacing an older one) so the next run starts a fresh file.
pub fn rotate_log(log: &Path, max_bytes: u64) {
    let too_big = std::fs::metadata(log).is_ok_and(|m| m.len() > max_bytes);
    if too_big {
        let mut rotated = log.as_os_str().to_owned();
        rotated.push(".1");
        let rotated = std::path::PathBuf::from(rotated);
        let _ = std::fs::remove_file(&rotated);
        let _ = std::fs::rename(log, rotated);
    }
}

/// True when something at `addr` answers `GET /api/health` with PhotoSift's
/// `{"ok": true, "version": ...}` body. Blocking, bounded by `PROBE_TIMEOUT`
/// per step; the sent `Host` header satisfies the server's DNS-rebinding guard.
pub fn health_answers(addr: SocketAddr) -> bool {
    probe(addr).unwrap_or(false)
}

/// Like `health_answers`, but keeps trying until `patience` runs out: the
/// first instance may have bound the port and not yet started serving.
pub fn wait_for_health(addr: SocketAddr, patience: Duration) -> bool {
    let deadline = std::time::Instant::now() + patience;
    loop {
        if health_answers(addr) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn probe(addr: SocketAddr) -> std::io::Result<bool> {
    let mut stream = TcpStream::connect_timeout(&addr, PROBE_TIMEOUT)?;
    stream.set_read_timeout(Some(PROBE_TIMEOUT))?;
    stream.set_write_timeout(Some(PROBE_TIMEOUT))?;
    write!(
        stream,
        "GET /api/health HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    )?;
    let mut raw = Vec::new();
    // A timeout mid-read still leaves whatever arrived in `raw`.
    let _ = stream.take(64 * 1024).read_to_end(&mut raw);
    Ok(is_health_response(&String::from_utf8_lossy(&raw)))
}

fn is_health_response(response: &str) -> bool {
    let Some((head, body)) = response.split_once("\r\n\r\n") else {
        return false;
    };
    if !head.starts_with("HTTP/1.1 200") && !head.starts_with("HTTP/1.0 200") {
        return false;
    }
    serde_json::from_str::<serde_json::Value>(body.trim()).is_ok_and(|v| {
        v.get("ok") == Some(&serde_json::Value::Bool(true)) && v.get("version").is_some()
    })
}

/// Opens `url` in the default browser. Windows only (the app is Windows-only);
/// elsewhere it reports an error rather than guessing a launcher.
pub fn open_browser(url: &str) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // The empty string is `start`'s window-title argument; without it a
        // quoted URL would be taken as the title.
        std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ())
    }
    #[cfg(not(windows))]
    {
        let _ = url;
        Err(std::io::Error::other("opening a browser is Windows-only"))
    }
}

/// A GUI-subsystem process (release builds, so the logon task shows no
/// window) starts with no standard handles, and its output would be dropped.
/// Point stdout/stderr at `<data root>\logs\photosift.log` instead. No-op when
/// stderr is already a console, pipe or file.
pub fn redirect_stdio_if_detached(data_root: &Path) {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        type Handle = *mut core::ffi::c_void;
        const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
        const STD_ERROR_HANDLE: u32 = -12i32 as u32;
        extern "system" {
            fn GetStdHandle(which: u32) -> Handle;
            fn SetStdHandle(which: u32, handle: Handle) -> i32;
        }
        // SAFETY: both are plain kernel32 calls; the file handle stays alive
        // (leaked below) for the whole process, as the std handles require.
        unsafe {
            let err = GetStdHandle(STD_ERROR_HANDLE);
            if !err.is_null() && err as isize != -1 {
                return;
            }
        }
        let dir = data_root.join("logs");
        let log = dir.join("photosift.log");
        let opened = std::fs::create_dir_all(&dir).and_then(|_| {
            rotate_log(&log, LOG_MAX_BYTES);
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log)
        });
        let Ok(file) = opened else { return };
        let handle = file.as_raw_handle() as Handle;
        // SAFETY: see above; `file` is forgotten so the handle is never closed.
        unsafe {
            SetStdHandle(STD_OUTPUT_HANDLE, handle);
            SetStdHandle(STD_ERROR_HANDLE, handle);
        }
        std::mem::forget(file);
    }
    #[cfg(not(windows))]
    let _ = data_root;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_only_oversized_logs_and_replaces_old_generation() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("photosift.log");
        let rotated = dir.path().join("photosift.log.1");

        std::fs::write(&log, b"small").unwrap();
        rotate_log(&log, 10);
        assert!(log.exists() && !rotated.exists());

        std::fs::write(&rotated, b"old generation").unwrap();
        std::fs::write(&log, b"this one is over the limit").unwrap();
        rotate_log(&log, 10);
        assert!(!log.exists());
        assert_eq!(
            std::fs::read(&rotated).unwrap(),
            b"this one is over the limit"
        );
    }

    #[test]
    fn recognises_health_body() {
        let ok = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n{\"ok\":true,\"version\":\"0.1.0\"}";
        assert!(is_health_response(ok));
    }

    #[test]
    fn rejects_other_servers() {
        assert!(!is_health_response("HTTP/1.1 200 OK\r\n\r\n<html></html>"));
        assert!(!is_health_response("HTTP/1.1 200 OK\r\n\r\n{\"ok\":true}"));
        assert!(!is_health_response(
            "HTTP/1.1 403 Forbidden\r\n\r\n{\"ok\":true,\"version\":\"1\"}"
        ));
        assert!(!is_health_response(""));
    }
}
