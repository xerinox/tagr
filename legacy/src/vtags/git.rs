//! Git status checking for virtual tags using gitoxide (`gix`).
//!
//! Provides lazy, cached git status evaluation for files. The repository is
//! discovered once, and a full `git status` is computed once then cached in
//! a `HashMap` for O(1) lookups on subsequent files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::vtags::types::GitCondition;

/// Cached git status flags for a single file.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default)]
struct FileGitStatus {
    tracked: bool,
    modified: bool,
    staged: bool,
    ignored: bool,
}

/// Git status checker backed by gitoxide.
///
/// Lazily computes full repository status on first query, then serves
/// all subsequent lookups from an in-memory cache.
pub struct GitStatusChecker {
    repo: gix::Repository,
    workdir: PathBuf,
    /// Cached index-worktree + tree-index status, keyed by repo-relative path.
    status_cache: Option<HashMap<String, FileGitStatus>>,
    /// Cached commit log results, keyed by repo-relative path.
    commit_cache: HashMap<String, CommitInfo>,
}

#[derive(Debug, Clone)]
struct CommitInfo {
    /// Unix timestamp of the most recent commit touching this file, if any.
    last_commit_time: Option<i64>,
}

impl GitStatusChecker {
    /// Discover a git repository from a file path.
    ///
    /// Returns `None` if the path is not inside a git repository.
    #[must_use]
    pub fn discover(path: &Path) -> Option<Self> {
        let dir = if path.is_file() { path.parent()? } else { path };

        let repo = gix::discover(dir).ok()?;
        let workdir = repo.workdir()?.to_path_buf();

        Some(Self {
            repo,
            workdir,
            status_cache: None,
            commit_cache: HashMap::new(),
        })
    }

    /// Check if a file matches a git condition.
    pub fn matches(&mut self, path: &Path, cond: &GitCondition, stale_days: u32) -> bool {
        let Some(rela) = self.repo_relative_path(path) else {
            return false;
        };

        match cond {
            GitCondition::Tracked => self.is_tracked(&rela),
            GitCondition::Untracked => self.is_untracked(&rela),
            GitCondition::Modified => self.is_modified(&rela),
            GitCondition::Staged => self.is_staged(&rela),
            GitCondition::Ignored => self.is_ignored(&rela),
            GitCondition::CommittedToday => self.is_committed_today(&rela),
            GitCondition::NeverCommitted => self.is_never_committed(&rela),
            GitCondition::Stale => self.is_stale(&rela, stale_days),
        }
    }

    /// Convert an absolute path to a repo-relative string with forward slashes.
    fn repo_relative_path(&self, path: &Path) -> Option<String> {
        let abs = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().ok()?.join(path)
        };

        let rel = abs.strip_prefix(&self.workdir).ok()?;
        // gix uses forward slashes even on Windows
        Some(rel.to_string_lossy().replace('\\', "/"))
    }

    /// Ensure the status cache is populated.
    fn ensure_status_cache(&mut self) {
        if self.status_cache.is_some() {
            return;
        }

        let mut cache: HashMap<String, FileGitStatus> = HashMap::new();

        // Populate tracked files from the index
        if let Ok(index) = self.repo.index_or_empty() {
            for entry in index.entries() {
                let path = entry.path(&index).to_string();
                cache.entry(path).or_default().tracked = true;
            }
        }

        // Run full status to get modified/staged/untracked/ignored
        #[allow(clippy::collapsible_if)]
        if let Ok(platform) = self.repo.status(gix::progress::Discard) {
            if let Ok(iter) = platform
                .dirwalk_options(|opts| {
                    opts.emit_untracked(gix::dir::walk::EmissionMode::Matching)
                        .emit_ignored(Some(gix::dir::walk::EmissionMode::Matching))
                })
                .into_iter(Vec::<gix::bstr::BString>::new())
            {
                for item in iter.filter_map(Result::ok) {
                    match item {
                        gix::status::Item::IndexWorktree(entry) => {
                            Self::process_index_worktree(&mut cache, entry);
                        }
                        gix::status::Item::TreeIndex(change) => {
                            Self::process_tree_index(&mut cache, &change);
                        }
                    }
                }
            }
        }

        self.status_cache = Some(cache);
    }

    fn process_index_worktree(
        cache: &mut HashMap<String, FileGitStatus>,
        entry: gix::status::index_worktree::Item,
    ) {
        use gix::status::index_worktree::Item;

        match entry {
            Item::Modification {
                rela_path, status, ..
            } => {
                let path = rela_path.to_string();
                let file_status = cache.entry(path).or_default();
                file_status.tracked = true;

                // Any Change variant means modified in the worktree
                if matches!(
                    status,
                    gix::status::plumbing::index_as_worktree::EntryStatus::Change(_)
                        | gix::status::plumbing::index_as_worktree::EntryStatus::Conflict { .. }
                ) {
                    file_status.modified = true;
                }
                if matches!(
                    status,
                    gix::status::plumbing::index_as_worktree::EntryStatus::IntentToAdd
                ) {
                    file_status.staged = true;
                }
            }
            Item::DirectoryContents { entry, .. } => {
                let path = entry.rela_path.to_string();
                let file_status = cache.entry(path).or_default();

                match entry.status {
                    gix::dir::entry::Status::Ignored(_) => {
                        file_status.ignored = true;
                    }
                    gix::dir::entry::Status::Tracked => {
                        file_status.tracked = true;
                    }
                    gix::dir::entry::Status::Untracked | gix::dir::entry::Status::Pruned => {}
                }
            }
            Item::Rewrite { .. } => {}
        }
    }

    fn process_tree_index(
        cache: &mut HashMap<String, FileGitStatus>,
        change: &gix::diff::index::Change,
    ) {
        let path = change.location().to_string();
        let file_status = cache.entry(path).or_default();
        file_status.staged = true;
    }

    fn get_status(&mut self, rela: &str) -> FileGitStatus {
        self.ensure_status_cache();
        self.status_cache
            .as_ref()
            .and_then(|c| c.get(rela))
            .cloned()
            .unwrap_or_default()
    }

    fn is_tracked(&mut self, rela: &str) -> bool {
        self.get_status(rela).tracked
    }

    fn is_untracked(&mut self, rela: &str) -> bool {
        let status = self.get_status(rela);
        !status.tracked && !status.ignored
    }

    fn is_modified(&mut self, rela: &str) -> bool {
        self.get_status(rela).modified
    }

    fn is_staged(&mut self, rela: &str) -> bool {
        self.get_status(rela).staged
    }

    fn is_ignored(&mut self, rela: &str) -> bool {
        self.get_status(rela).ignored
    }

    /// Ensure commit info is cached for a file by walking the log.
    fn ensure_commit_info(&mut self, rela: &str) {
        if self.commit_cache.contains_key(rela) {
            return;
        }

        let info = self.find_last_commit_time(rela);
        self.commit_cache.insert(rela.to_string(), info);
    }

    fn find_last_commit_time(&mut self, rela: &str) -> CommitInfo {
        // Set cache for performance (must be before head_id borrow)
        self.repo.object_cache_size_if_unset(32 * 1024);

        let Ok(head_id) = self.repo.head_id() else {
            return CommitInfo {
                last_commit_time: None,
            };
        };

        let Ok(walk) = head_id.ancestors().all() else {
            return CommitInfo {
                last_commit_time: None,
            };
        };

        for info in walk.filter_map(Result::ok) {
            let Ok(obj) = info.id().object() else {
                continue;
            };
            let Ok(commit) = obj.try_into_commit() else {
                continue;
            };
            let Ok(tree) = commit.tree() else {
                continue;
            };

            if tree.lookup_entry_by_path(rela).ok().flatten().is_some() {
                let time = commit.time().map_or(0, |t| t.seconds);
                return CommitInfo {
                    last_commit_time: Some(time),
                };
            }
        }

        CommitInfo {
            last_commit_time: None,
        }
    }

    fn is_committed_today(&mut self, rela: &str) -> bool {
        self.ensure_commit_info(rela);
        let Some(info) = self.commit_cache.get(rela) else {
            return false;
        };
        let Some(commit_time) = info.last_commit_time else {
            return false;
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));

        (now - commit_time).unsigned_abs() < 86_400
    }

    fn is_never_committed(&mut self, rela: &str) -> bool {
        self.ensure_commit_info(rela);
        self.commit_cache
            .get(rela)
            .is_some_and(|info| info.last_commit_time.is_none())
    }

    fn is_stale(&mut self, rela: &str, stale_days: u32) -> bool {
        self.ensure_commit_info(rela);
        let Some(info) = self.commit_cache.get(rela) else {
            return false;
        };
        let Some(commit_time) = info.last_commit_time else {
            return false;
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));

        let threshold_secs = i64::from(stale_days) * 86_400;
        (now - commit_time) > threshold_secs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    /// Create a temporary git repo for testing.
    fn setup_git_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("git init failed");

        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(path)
            .output()
            .expect("git config email failed");

        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(path)
            .output()
            .expect("git config name failed");

        dir
    }

    #[test]
    fn test_tracked_and_untracked() {
        let dir = setup_git_repo();
        let tracked = dir.path().join("tracked.txt");
        let untracked = dir.path().join("untracked.txt");

        fs::write(&tracked, "hello").unwrap();
        fs::write(&untracked, "world").unwrap();

        Command::new("git")
            .args(["add", "tracked.txt"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let mut checker = GitStatusChecker::discover(&tracked).unwrap();

        assert!(checker.matches(&tracked, &GitCondition::Tracked, 90));
        assert!(!checker.matches(&untracked, &GitCondition::Tracked, 90));
        assert!(checker.matches(&untracked, &GitCondition::Untracked, 90));
        assert!(!checker.matches(&tracked, &GitCondition::Untracked, 90));
    }

    #[test]
    fn test_modified() {
        let dir = setup_git_repo();
        let file = dir.path().join("file.txt");

        fs::write(&file, "original").unwrap();

        Command::new("git")
            .args(["add", "file.txt"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Not modified yet
        let mut checker = GitStatusChecker::discover(&file).unwrap();
        assert!(!checker.matches(&file, &GitCondition::Modified, 90));

        // Modify it
        fs::write(&file, "changed").unwrap();
        // Need fresh checker since status is cached
        let mut checker = GitStatusChecker::discover(&file).unwrap();
        assert!(checker.matches(&file, &GitCondition::Modified, 90));
    }

    #[test]
    fn test_staged() {
        let dir = setup_git_repo();
        let file = dir.path().join("file.txt");

        fs::write(&file, "content").unwrap();

        Command::new("git")
            .args(["add", "file.txt"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Should be staged (in index but not yet committed = diff vs HEAD)
        let mut checker = GitStatusChecker::discover(&file).unwrap();
        assert!(checker.matches(&file, &GitCondition::Staged, 90));
    }

    #[test]
    fn test_ignored() {
        let dir = setup_git_repo();
        let ignored = dir.path().join("build.o");

        fs::write(dir.path().join(".gitignore"), "*.o\n").unwrap();
        fs::write(&ignored, "binary").unwrap();

        Command::new("git")
            .args(["add", ".gitignore"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        Command::new("git")
            .args(["commit", "-m", "add gitignore"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let mut checker = GitStatusChecker::discover(&ignored).unwrap();
        assert!(checker.matches(&ignored, &GitCondition::Ignored, 90));
        assert!(!checker.matches(&ignored, &GitCondition::Tracked, 90));
    }

    #[test]
    fn test_committed_today() {
        let dir = setup_git_repo();
        let file = dir.path().join("file.txt");

        fs::write(&file, "content").unwrap();

        Command::new("git")
            .args(["add", "file.txt"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        Command::new("git")
            .args(["commit", "-m", "today"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let mut checker = GitStatusChecker::discover(&file).unwrap();
        assert!(checker.matches(&file, &GitCondition::CommittedToday, 90));
    }

    #[test]
    fn test_never_committed() {
        let dir = setup_git_repo();
        let committed = dir.path().join("committed.txt");
        let uncommitted = dir.path().join("uncommitted.txt");

        fs::write(&committed, "content").unwrap();

        Command::new("git")
            .args(["add", "committed.txt"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        fs::write(&uncommitted, "not yet").unwrap();

        let mut checker = GitStatusChecker::discover(&committed).unwrap();
        assert!(!checker.matches(&committed, &GitCondition::NeverCommitted, 90));
        assert!(checker.matches(&uncommitted, &GitCondition::NeverCommitted, 90));
    }

    #[test]
    fn test_not_in_git_repo() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file.txt");
        fs::write(&file, "content").unwrap();

        // Should return None when not in a git repo
        assert!(GitStatusChecker::discover(&file).is_none());
    }
}
