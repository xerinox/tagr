//! Integration tests for daemon mode.
//!
//! These tests spawn the tagr binary in daemon mode and verify IPC communication
//! using the binary wire protocol (wincode + length-prefixed framing).
//! Each test uses an isolated `XDG_RUNTIME_DIR` to avoid conflicts with real daemons.

#![allow(dead_code, clippy::ignore_without_reason)]

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
    rt: tokio::runtime::Runtime,
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

        let rt = tokio::runtime::Runtime::new().expect("create tokio runtime");

        let mut harness = Self {
            child,
            runtime_dir,
            config_dir,
            data_dir,
            socket_path,
            rt,
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
        let _ = self.child.kill();
        panic!(
            "Daemon socket did not appear at {} within {:?}",
            self.socket_path.display(), timeout
        );
    }

    /// Send a wire protocol request and receive the response.
    fn send_request(&self, request: tagr::ipc::wire::Request) -> tagr::ipc::wire::Response {
        use interprocess::local_socket::tokio::prelude::LocalSocketStream;
        use interprocess::local_socket::traits::tokio::Stream;
        use interprocess::local_socket::{GenericFilePath, ToFsName};
        use std::sync::atomic::{AtomicU32, Ordering};
        use tagr::ipc::wire::{ClientMessage, ServerMessage, read_frame, write_frame};

        static NEXT_ID: AtomicU32 = AtomicU32::new(1);

        let socket_path = self.socket_path.clone();
        self.rt.block_on(async {
            let name = socket_path.to_fs_name::<GenericFilePath>().unwrap();
            let conn = LocalSocketStream::connect(name).await.unwrap();
            let (mut reader, mut writer) = tokio::io::split(conn);

            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let msg = ClientMessage::Request {
                id,
                payload: request,
            };
            write_frame(&mut writer, &msg).await.unwrap();

            let server_msg: ServerMessage = read_frame(&mut reader).await.unwrap().unwrap();
            match server_msg {
                ServerMessage::Response { payload, .. } => payload,
                ServerMessage::Event(_) => panic!("unexpected event"),
            }
        })
    }

    fn ping(&self) -> tagr::ipc::wire::Response {
        self.send_request(tagr::ipc::wire::Request::Ping)
    }

    fn shutdown(&self) -> tagr::ipc::wire::Response {
        self.send_request(tagr::ipc::wire::Request::Shutdown)
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
        matches!(response, tagr::ipc::wire::Response::Pong),
        "Expected Pong response to Ping, got: {response:?}"
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
        matches!(response, tagr::ipc::wire::Response::Ok),
        "Expected Ok response to Shutdown, got: {response:?}"
    );

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
fn test_daemon_multiple_connections() {
    let harness = DaemonHarness::spawn();

    for i in 0..5 {
        let response = harness.ping();
        assert!(
            matches!(response, tagr::ipc::wire::Response::Pong),
            "Ping {i} failed: {response:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Contract tests: typed wire protocol parity
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_contract_tag_and_search_parity() {
    use tagr::ipc::wire::{Request, Response};

    let harness = DaemonHarness::spawn();

    let test_file = harness.data_dir.path().join("contract_test.txt");
    std::fs::write(&test_file, "contract test content").unwrap();

    let tag_response = harness.send_request(Request::AddTags {
        file: test_file.to_string_lossy().into_owned(),
        tags: vec!["contract-tag".into()],
    });
    assert!(
        matches!(tag_response, Response::Ok),
        "Tag via IPC should succeed: {tag_response:?}"
    );

    let search_response = harness.send_request(Request::FindByTag {
        tag: "contract-tag".into(),
    });
    match search_response {
        Response::FilePaths(paths) => {
            assert!(
                paths.iter().any(|p| p.contains("contract_test.txt")),
                "Search should find the tagged file. Got: {paths:?}"
            );
        }
        other => panic!("Expected FilePaths response, got: {other:?}"),
    }
}

#[test]
#[ignore]
fn test_contract_list_parity() {
    use tagr::ipc::wire::{Request, Response};

    let harness = DaemonHarness::spawn();

    let f1 = harness.data_dir.path().join("parity_a.txt");
    let f2 = harness.data_dir.path().join("parity_b.txt");
    std::fs::write(&f1, "a").unwrap();
    std::fs::write(&f2, "b").unwrap();

    harness.send_request(Request::AddTags {
        file: f1.to_string_lossy().into_owned(),
        tags: vec!["parity".into()],
    });
    harness.send_request(Request::AddTags {
        file: f2.to_string_lossy().into_owned(),
        tags: vec!["parity".into()],
    });

    let response = harness.send_request(Request::ListFiles);
    match response {
        Response::Files(files) => {
            let file_strs: Vec<_> = files.iter().map(|f| f.file.as_str()).collect();
            assert!(
                file_strs.iter().any(|f| f.contains("parity_a.txt")),
                "List should contain parity_a.txt. Got: {file_strs:?}"
            );
            assert!(
                file_strs.iter().any(|f| f.contains("parity_b.txt")),
                "List should contain parity_b.txt. Got: {file_strs:?}"
            );
        }
        other => panic!("Expected Files response, got: {other:?}"),
    }
}

#[test]
#[ignore]
fn test_contract_untag_via_daemon() {
    use tagr::ipc::wire::{Request, Response};

    let harness = DaemonHarness::spawn();

    let test_file = harness.data_dir.path().join("untag_contract.txt");
    std::fs::write(&test_file, "content").unwrap();
    let file_str = test_file.to_string_lossy().into_owned();

    harness.send_request(Request::AddTags {
        file: file_str.clone(),
        tags: vec!["remove-me".into(), "keep-me".into()],
    });
    harness.send_request(Request::RemoveTags {
        file: file_str,
        tags: vec!["remove-me".into()],
        all: false,
    });

    let search = harness.send_request(Request::FindByTag {
        tag: "remove-me".into(),
    });
    match search {
        Response::FilePaths(paths) => {
            assert!(
                !paths.iter().any(|p| p.contains("untag_contract.txt")),
                "File should not appear after untag. Got: {paths:?}"
            );
        }
        other => panic!("Expected FilePaths, got: {other:?}"),
    }

    let search_keep = harness.send_request(Request::FindByTag {
        tag: "keep-me".into(),
    });
    match search_keep {
        Response::FilePaths(paths) => {
            assert!(
                paths.iter().any(|p| p.contains("untag_contract.txt")),
                "File should still appear for kept tag. Got: {paths:?}"
            );
        }
        other => panic!("Expected FilePaths, got: {other:?}"),
    }
}

#[test]
#[ignore]
fn test_contract_cleanup_via_daemon() {
    use tagr::ipc::wire::{Request, Response};

    let harness = DaemonHarness::spawn();

    let ghost = harness.data_dir.path().join("ghost_file.txt");
    std::fs::write(&ghost, "i will be deleted").unwrap();
    harness.send_request(Request::AddTags {
        file: ghost.to_string_lossy().into_owned(),
        tags: vec!["ghost".into()],
    });

    std::fs::remove_file(&ghost).unwrap();

    let cleanup = harness.send_request(Request::Cleanup);
    match cleanup {
        Response::CleanupResult { removed } => {
            assert!(
                removed >= 1,
                "Should have removed at least 1 entry, got: {removed}"
            );
        }
        other => panic!("Expected CleanupResult, got: {other:?}"),
    }

    let search = harness.send_request(Request::FindByTag {
        tag: "ghost".into(),
    });
    match search {
        Response::FilePaths(paths) => {
            assert!(
                !paths.iter().any(|p| p.contains("ghost_file.txt")),
                "Cleanup should remove deleted file from DB. Got: {paths:?}"
            );
        }
        other => panic!("Expected FilePaths, got: {other:?}"),
    }
}
