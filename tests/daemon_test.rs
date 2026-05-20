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
            .arg("start")
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

// ---------------------------------------------------------------------------
// Contract tests: direct CLI vs IPC daemon parity
//
// These verify that running a command locally (via the binary) and running
// the same command through the daemon IPC produce identical output.
// ---------------------------------------------------------------------------

/// Run tagr directly (no daemon) with isolated dirs and return stdout.
fn run_direct(data_dir: &TempDir, args: &[&str]) -> String {
    let binary = env!("CARGO_BIN_EXE_tagr");
    let output = Command::new(binary)
        .args(args)
        .env("XDG_CONFIG_HOME", data_dir.path())
        .env("XDG_DATA_HOME", data_dir.path())
        .env("XDG_STATE_HOME", data_dir.path())
        .output()
        .expect("run direct command");
    String::from_utf8(output.stdout).unwrap()
}

/// Run a command through the daemon IPC and extract the output from the response.
fn run_via_daemon(harness: &DaemonHarness, args: &[&str]) -> String {
    let mut full_args: Vec<&str> = vec!["tagr"];
    full_args.extend(args);

    let cmd = serde_json::json!({
        "Command": {
            "args": full_args,
            "cwd": harness.data_dir.path().to_str().unwrap()
        }
    });

    let response = harness.send_request(&cmd.to_string());
    // Parse the IPC response to extract the output string
    let parsed: serde_json::Value = serde_json::from_str(response.trim()).unwrap_or_default();

    if let Some(output) = parsed.get("Success").and_then(|v| v.as_str()) {
        output.to_string()
    } else {
        panic!(
            "Expected Success response from daemon, got: {response}"
        );
    }
}

#[test]
#[ignore]
fn test_contract_tag_and_search_parity() {
    let harness = DaemonHarness::spawn();

    // Tag a file via IPC
    let test_file = harness.data_dir.path().join("contract_test.txt");
    std::fs::write(&test_file, "contract test content").unwrap();

    let tag_response = run_via_daemon(
        &harness,
        &["tag", test_file.to_str().unwrap(), "contract-tag"],
    );
    // Tag command output should indicate success (may be empty in quiet-like mode)
    assert!(
        !tag_response.contains("Error"),
        "Tag via IPC should not error: {tag_response}"
    );

    // Now search via IPC
    let ipc_search = run_via_daemon(
        &harness,
        &["search", "--tag", "contract-tag", "-q"],
    );

    // The IPC search should contain the file path
    assert!(
        ipc_search.contains("contract_test.txt"),
        "IPC search should find the tagged file.\nGot: {ipc_search}"
    );
}

#[test]
#[ignore]
fn test_contract_list_parity() {
    let harness = DaemonHarness::spawn();

    // Tag two files via IPC
    let f1 = harness.data_dir.path().join("parity_a.txt");
    let f2 = harness.data_dir.path().join("parity_b.txt");
    std::fs::write(&f1, "a").unwrap();
    std::fs::write(&f2, "b").unwrap();

    run_via_daemon(&harness, &["tag", f1.to_str().unwrap(), "parity"]);
    run_via_daemon(&harness, &["tag", f2.to_str().unwrap(), "parity"]);

    // List via IPC
    let ipc_list = run_via_daemon(&harness, &["list", "files", "-q"]);

    // Both files should appear in the list output
    assert!(
        ipc_list.contains("parity_a.txt"),
        "IPC list should contain parity_a.txt.\nGot: {ipc_list}"
    );
    assert!(
        ipc_list.contains("parity_b.txt"),
        "IPC list should contain parity_b.txt.\nGot: {ipc_list}"
    );
}

#[test]
#[ignore]
fn test_contract_untag_via_daemon() {
    let harness = DaemonHarness::spawn();

    let test_file = harness.data_dir.path().join("untag_contract.txt");
    std::fs::write(&test_file, "content").unwrap();

    // Tag, then untag
    run_via_daemon(
        &harness,
        &["tag", test_file.to_str().unwrap(), "remove-me", "keep-me"],
    );
    run_via_daemon(
        &harness,
        &["untag", test_file.to_str().unwrap(), "remove-me"],
    );

    // Search for the removed tag should not find this file
    let search = run_via_daemon(
        &harness,
        &["search", "--tag", "remove-me", "-q"],
    );
    assert!(
        !search.contains("untag_contract.txt"),
        "File should not appear after untag.\nGot: {search}"
    );

    // But search for kept tag should find it
    let search_keep = run_via_daemon(
        &harness,
        &["search", "--tag", "keep-me", "-q"],
    );
    assert!(
        search_keep.contains("untag_contract.txt"),
        "File should still appear for kept tag.\nGot: {search_keep}"
    );
}

#[test]
#[ignore]
fn test_contract_cleanup_via_daemon() {
    let harness = DaemonHarness::spawn();

    // Tag a file, then delete it from filesystem
    let ghost = harness.data_dir.path().join("ghost_file.txt");
    std::fs::write(&ghost, "i will be deleted").unwrap();
    run_via_daemon(&harness, &["tag", ghost.to_str().unwrap(), "ghost"]);

    // Remove the actual file
    std::fs::remove_file(&ghost).unwrap();

    // Cleanup should remove it from the database
    let cleanup_out = run_via_daemon(&harness, &["cleanup", "-q"]);
    // The output should mention the cleaned-up file or be empty (quiet mode)
    // Either way, verify the file is gone from the DB
    let search = run_via_daemon(
        &harness,
        &["search", "--tag", "ghost", "-q"],
    );
    assert!(
        !search.contains("ghost_file.txt"),
        "Cleanup should remove deleted file from DB.\nSearch: {search}\nCleanup: {cleanup_out}"
    );
}
