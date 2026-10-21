mod common;

use std::{
    net::TcpListener,
    path::Path,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

use common::spawn_server;

/// Runs the real binary against its own temp data root whose settings.toml
/// asks for `port`, so it collides with whatever already holds that port.
fn run_second_instance(port: u16, args: &[&str]) -> (Output, Duration) {
    let home = tempfile::tempdir().expect("temp data root");
    let settings = photosift::config::Settings {
        port,
        ..Default::default()
    };
    std::fs::write(
        home.path().join("settings.toml"),
        toml::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();
    let bin = Path::new(env!("CARGO_BIN_EXE_photosift"));
    let started = Instant::now();
    let child = Command::new(bin)
        .args(args)
        .env("PHOTOSIFT_HOME", home.path())
        .env_remove("PHOTOSIFT_WEB_DIST")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn photosift");
    let out = child.wait_with_output().expect("wait for photosift");
    (out, started.elapsed())
}

fn port_of(base_url: &str) -> u16 {
    base_url.rsplit(':').next().unwrap().parse().unwrap()
}

#[tokio::test]
async fn second_launch_with_open_exits_zero_and_leaves_first_running() {
    let first = spawn_server().await;
    let port = port_of(&first.base_url);

    let (out, took) =
        tokio::task::spawn_blocking(move || run_second_instance(port, &["--no-browser", "--open"]))
            .await
            .unwrap();

    assert!(
        out.status.success(),
        "exit {:?}; stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(took < Duration::from_secs(5), "took {took:?}");
    let health = reqwest::get(format!("{}/api/health", first.base_url))
        .await
        .unwrap();
    assert_eq!(health.status(), 200);
}

#[tokio::test]
async fn port_held_by_something_else_fails_with_clear_message() {
    let squatter = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = squatter.local_addr().unwrap().port();

    let (out, took) =
        tokio::task::spawn_blocking(move || run_second_instance(port, &["--no-browser", "--open"]))
            .await
            .unwrap();

    assert!(!out.status.success());
    assert!(took < Duration::from_secs(10), "took {took:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(&format!("port {port}")) && stderr.contains("not PhotoSift"),
        "stderr: {stderr}"
    );
}
