//! CLI conformance tests.
//!
//! Verify that the actual `tagr` binary behaves as documented:
//! - `--help` output contains expected subcommands and flags
//! - Documented usage patterns work end-to-end
//! - Exit codes match spec (0 = success, 1 = failure)
//! - Output formats (`--quiet`) produce expected results
//!
//! Run with: `cargo test --test cli_conformance_test -- --ignored`

use std::process::Command;
use tempfile::TempDir;

/// Run tagr with isolated config/data dirs so tests don't affect real state.
/// Always sets up a minimal config to prevent the first-run setup wizard.
fn tagr_cmd(data_dir: &TempDir) -> Command {
    let binary = env!("CARGO_BIN_EXE_tagr");
    let mut cmd = Command::new(binary);
    // Ensure config exists to prevent interactive setup wizard
    setup_db(data_dir);
    cmd.env("XDG_CONFIG_HOME", data_dir.path())
        .env("XDG_DATA_HOME", data_dir.path())
        .env("XDG_STATE_HOME", data_dir.path());
    cmd
}

/// Set up a minimal database config so commands that need a DB can work.
fn setup_db(data_dir: &TempDir) {
    let config_path = data_dir.path().join("tagr");
    std::fs::create_dir_all(&config_path).unwrap();
    let db_path = data_dir.path().join("test_db");
    std::fs::write(
        config_path.join("config.toml"),
        format!(
            "default_database = \"default\"\n\n[databases]\ndefault = \"{}\"\n",
            db_path.display()
        ),
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// Help text conformance
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_top_level_help_lists_all_commands() {
    let data_dir = TempDir::new().unwrap();
    let output = tagr_cmd(&data_dir).arg("--help").output().unwrap();
    let help = String::from_utf8(output.stdout).unwrap();

    let expected_commands = [
        "browse", "tag", "search", "list", "bulk", "note", "cleanup",
        "tags", "alias", "filter", "watch", "db", "config", "completions",
    ];
    for cmd in expected_commands {
        assert!(
            help.contains(cmd),
            "Top-level --help should list '{cmd}' command.\nGot:\n{help}"
        );
    }
}

#[test]
#[ignore]
fn test_watch_help_lists_subcommands() {
    let data_dir = TempDir::new().unwrap();
    let output = tagr_cmd(&data_dir)
        .args(["watch", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8(output.stdout).unwrap();

    let expected = ["add", "remove", "list", "status", "start", "stop"];
    for sub in expected {
        assert!(
            help.contains(sub),
            "watch --help should list '{sub}' subcommand.\nGot:\n{help}"
        );
    }
}

#[test]
#[ignore]
fn test_watch_add_help_shows_filter_flag() {
    let data_dir = TempDir::new().unwrap();
    let output = tagr_cmd(&data_dir)
        .args(["watch", "add", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8(output.stdout).unwrap();

    assert!(
        help.contains("--filter"),
        "watch add --help should show --filter flag.\nGot:\n{help}"
    );
    assert!(
        help.contains("--tag") || help.contains("-t"),
        "watch add --help should show --tag/-t flag.\nGot:\n{help}"
    );
    // The help should encourage using saved filters
    assert!(
        help.contains("filter save"),
        "watch add --help should mention `tagr filter save`.\nGot:\n{help}"
    );
}

#[test]
#[ignore]
fn test_bulk_help_lists_subcommands() {
    let data_dir = TempDir::new().unwrap();
    let output = tagr_cmd(&data_dir)
        .args(["bulk", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8(output.stdout).unwrap();

    let expected = [
        "tag", "untag", "rename-tag", "merge-tags", "copy-tags",
        "from-file", "map-tags", "delete-files", "transform",
        "propagate-by-dir", "propagate-by-ext",
    ];
    for sub in expected {
        assert!(
            help.contains(sub),
            "bulk --help should list '{sub}'.\nGot:\n{help}"
        );
    }
}

// ---------------------------------------------------------------------------
// Exit code conformance
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_help_exits_zero() {
    let data_dir = TempDir::new().unwrap();
    let status = tagr_cmd(&data_dir).arg("--help").status().unwrap();
    assert!(status.success(), "--help should exit 0");
}

#[test]
#[ignore]
fn test_invalid_command_exits_nonzero() {
    let data_dir = TempDir::new().unwrap();
    let status = tagr_cmd(&data_dir).arg("nonexistent").status().unwrap();
    assert!(!status.success(), "Invalid command should exit non-zero");
}

#[test]
#[ignore]
fn test_search_no_results_exits_zero() {
    let data_dir = TempDir::new().unwrap();
    let output = tagr_cmd(&data_dir)
        .args(["search", "--tag", "nonexistent-tag-xyz"])
        .output()
        .unwrap();
    // Currently search exits 0 even with no results (empty output)
    assert!(output.status.success(), "Search with no results currently exits 0");
}

// ---------------------------------------------------------------------------
// End-to-end usage patterns
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_tag_and_search_roundtrip() {
    let data_dir = TempDir::new().unwrap();
    setup_db(&data_dir);

    // Create a test file
    let test_file = data_dir.path().join("hello.txt");
    std::fs::write(&test_file, "hello").unwrap();

    // Tag the file
    let status = tagr_cmd(&data_dir)
        .args(["tag", &test_file.to_string_lossy(), "greeting"])
        .status()
        .unwrap();
    assert!(status.success(), "tag command should succeed");

    // Search should find it
    let output = tagr_cmd(&data_dir)
        .args(["search", "--tag", "greeting", "-q"])
        .output()
        .unwrap();
    assert!(output.status.success(), "search should succeed");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("hello.txt"),
        "search output should contain the tagged file.\nGot:\n{stdout}"
    );
}

#[test]
#[ignore]
fn test_tag_and_list_roundtrip() {
    let data_dir = TempDir::new().unwrap();
    setup_db(&data_dir);

    let test_file = data_dir.path().join("test.rs");
    std::fs::write(&test_file, "fn main() {}").unwrap();

    let status = tagr_cmd(&data_dir)
        .args(["tag", &test_file.to_string_lossy(), "rust", "code"])
        .status()
        .unwrap();
    assert!(status.success());

    // List should show the file
    let output = tagr_cmd(&data_dir)
        .args(["list", "files", "-q"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("test.rs"),
        "list files should contain the tagged file.\nGot:\n{stdout}"
    );
}

#[test]
#[ignore]
fn test_quiet_mode_output_is_pipe_friendly() {
    let data_dir = TempDir::new().unwrap();
    setup_db(&data_dir);

    // Tag two files
    let f1 = data_dir.path().join("a.txt");
    let f2 = data_dir.path().join("b.txt");
    std::fs::write(&f1, "a").unwrap();
    std::fs::write(&f2, "b").unwrap();

    tagr_cmd(&data_dir)
        .args(["tag", &f1.to_string_lossy(), "shared"])
        .status()
        .unwrap();
    tagr_cmd(&data_dir)
        .args(["tag", &f2.to_string_lossy(), "shared"])
        .status()
        .unwrap();

    let output = tagr_cmd(&data_dir)
        .args(["search", "--tag", "shared", "-q"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Quiet mode: one path per line, no headers/decoration
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "Quiet search should output exactly 2 lines (one per file).\nGot:\n{stdout}"
    );
    // Each line should be a path (no brackets, no tags inline)
    for line in &lines {
        assert!(
            !line.contains('['),
            "Quiet output should not contain brackets.\nLine: {line}"
        );
    }
}

// ---------------------------------------------------------------------------
// Watch subcommand conformance
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_watch_list_empty() {
    let data_dir = TempDir::new().unwrap();
    setup_db(&data_dir);

    let output = tagr_cmd(&data_dir)
        .args(["watch", "list"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("No watch rules configured"),
        "watch list on fresh config should say no rules.\nGot:\n{stdout}"
    );
}

#[test]
#[ignore]
fn test_watch_status_shows_stopped() {
    let data_dir = TempDir::new().unwrap();
    let runtime_dir = TempDir::new().unwrap();
    setup_db(&data_dir);

    let output = tagr_cmd(&data_dir)
        .env("XDG_RUNTIME_DIR", runtime_dir.path())
        .args(["watch", "status"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("stopped"),
        "watch status with no daemon should show 'stopped'.\nGot:\n{stdout}"
    );
}

#[test]
#[ignore]
fn test_watch_remove_invalid_index() {
    let data_dir = TempDir::new().unwrap();
    setup_db(&data_dir);

    let output = tagr_cmd(&data_dir)
        .args(["watch", "remove", "0"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "remove 0 should fail");

    let output = tagr_cmd(&data_dir)
        .args(["watch", "remove", "999"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "remove 999 on empty config should fail");
}

// Note: --format json is not yet implemented on search.
// When added, add a conformance test here to verify valid JSON output.
