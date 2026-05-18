//! Integration tests for daemon mode.
//!
//! These tests spawn the tagr binary in daemon mode and verify IPC communication.
//! Each test uses an isolated XDG_RUNTIME_DIR to avoid conflicts with real daemons.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Test harness that manages a daemon process with isolated paths.
struct DaemonHarness {
    child: Child,
    runtime_dir: TempDir,
    config_dir: TempDir,
    data_dir: TempDir,
    socket_path: PathBuf,
}

impl DaemonHarness {
    /// Spawn a daemon with isolated directories.
    /// Blocks until the socket is ready or times out.
    fn spawn() -> Self {
        let runtime_dir = TempDir::new().expect("create runtime_dir");
        let config_dir = TempDir::new().expect("create config_dir");
        let data_dir = TempDir::new().expect("create data_dir");

        let socket_path = runtime_dir.path().join("tagr_daemon.sock");
        let db_path = data_dir.path().join("test_db");

        // Write a minimal config so the daemon can find a database
        let config_path = config_dir.path().join("tagr");
        std::fs::create_dir_all(&config_path).unwrap();
        let config_file = config_path.join("config.toml");
        std::fs::write(
            &config_file,
            format!(
                "default_database = \"default\"\n\n[databases]\ndefault = \"{}\"\n",
                db_path.display()
            ),
        )
        .unwrap();

        let binary = env!("CARGO_BIN_EXE_tagr");

        let child = Command::new(binary)
            .arg("watch")
            .arg("--daemon")
            .env("XDG_RUNTIME_DIR", runtime_dir.path())
            .env("XDG_CONFIG_HOME", config_dir.path())
            .env("XDG_DATA_HOME", data_dir.path())
            .env("XDG_STATE_HOME", data_dir.path())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn daemon binary");

        let mut harness = Self {
            child,
            runtime_dir,
            config_dir,
            data_dir,
            socket_path,
        };

        harness.wait_for_socket(Duration::from_secs(5));
        harness
    }

    fn wait_for_socket(&mut self, timeout: Duration) {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if self.socket_path.exists() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        // Read stderr for diagnostics
        let _ = self.child.kill();
        panic!(
            "Daemon socket did not appear at {:?} within {:?}",
            self.socket_path, timeout
        );
    }

    /// Send a JSON request and receive a JSON response via the Unix socket.
    fn send_request(&self, request: &str) -> String {
        let mut stream =
            UnixStream::connect(&self.socket_path).expect("connect to daemon socket");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();

        let msg = format!("{request}\n");
        stream.write_all(msg.as_bytes()).expect("write request");

        let mut reader = BufReader::new(stream);
        let mut response = String::new();
        reader.read_line(&mut response).expect("read response");
        response
    }

    /// Send typed IPC request
    fn ping(&self) -> String {
        self.send_request("\"Ping\"")
    }

    fn shutdown(&self) -> String {
        self.send_request("\"Shutdown\"")
    }
}

impl Drop for DaemonHarness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
#[ignore] // Requires built binary — run with: cargo test --test daemon_test -- --ignored
fn test_daemon_ping_pong() {
    let harness = DaemonHarness::spawn();
    let response = harness.ping();
    assert!(
        response.contains("Success"),
        "Expected Success response to Ping, got: {response}"
    );
}

#[test]
#[ignore]
fn test_daemon_shutdown_cleans_up_socket() {
    let harness = DaemonHarness::spawn();
    let socket = harness.socket_path.clone();

    assert!(socket.exists(), "Socket should exist while daemon runs");

    let response = harness.shutdown();
    assert!(
        response.contains("Success"),
        "Expected Success response to Shutdown, got: {response}"
    );

    // Wait for socket removal
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        if !socket.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("Socket was not cleaned up after shutdown");
}

#[test]
#[ignore]
fn test_daemon_handles_invalid_json() {
    let harness = DaemonHarness::spawn();
    let _response = harness.send_request("this is not json");
    // Daemon should not crash — it should return an error or close the connection
    // The connection may just be closed (empty response) or return an error
    // Either way, verify daemon is still alive after:
    let ping_response = harness.ping();
    assert!(
        ping_response.contains("Success"),
        "Daemon should survive invalid JSON. Ping got: {ping_response}"
    );
}

#[test]
#[ignore]
fn test_daemon_command_tag() {
    let harness = DaemonHarness::spawn();

    // Create a temp file to tag
    let test_file = harness.data_dir.path().join("hello.txt");
    std::fs::write(&test_file, "hello world").unwrap();

    let cmd = serde_json::json!({
        "Command": {
            "args": ["tagr", "tag", test_file.to_str().unwrap(), "test-tag"],
            "cwd": harness.data_dir.path().to_str().unwrap()
        }
    });

    let response = harness.send_request(&cmd.to_string());
    assert!(
        response.contains("Success"),
        "Tag command should succeed, got: {response}"
    );
}

#[test]
#[ignore]
fn test_daemon_multiple_connections() {
    let harness = DaemonHarness::spawn();

    // Send multiple pings in quick succession
    for i in 0..5 {
        let response = harness.ping();
        assert!(
            response.contains("Success"),
            "Ping {i} failed: {response}"
        );
    }
}
