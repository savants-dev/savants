//! Freshness detection: warns when the index is stale.
//! Stores git HEAD hash at reindex time, checks on every search.

use std::path::{Path, PathBuf};

/// Get the current git HEAD hash for a repo path.
pub fn get_git_head(repo_path: &str) -> Option<String> {
    let head_file = Path::new(repo_path).join(".git/HEAD");
    let content = std::fs::read_to_string(&head_file).ok()?;
    let trimmed = content.trim();

    if trimmed.starts_with("ref: ") {
        // Symbolic ref - read the actual hash
        let ref_path = Path::new(repo_path).join(".git").join(&trimmed[5..]);
        std::fs::read_to_string(&ref_path).ok().map(|s| s.trim().to_string())
    } else {
        // Detached HEAD - hash directly
        Some(trimmed.to_string())
    }
}

/// Get the current branch name.
pub fn get_git_branch(repo_path: &str) -> Option<String> {
    let head_file = Path::new(repo_path).join(".git/HEAD");
    let content = std::fs::read_to_string(&head_file).ok()?;
    let trimmed = content.trim();

    if trimmed.starts_with("ref: refs/heads/") {
        Some(trimmed[16..].to_string())
    } else {
        Some("detached".to_string())
    }
}

fn state_path(repo: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".savants")
        .join("freshness")
        .join(format!("{}.txt", repo))
}

/// Save the current git state after reindex.
pub fn save_state(repo: &str, head: &str, branch: &str) {
    let path = state_path(repo);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, format!("{}\n{}", head, branch));
}

/// Check how many files changed since last index.
pub fn count_changed_files(repo: &str, repo_path: &str) -> usize {
    let path = state_path(repo);
    let index_modified = match std::fs::metadata(&path) {
        Ok(m) => m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
        Err(_) => return 0,
    };

    let skip = ["node_modules", ".git", "dist", "build", "target", ".next", "__pycache__"];
    let mut changed = 0;

    for entry in walkdir::WalkDir::new(repo_path)
        .max_depth(10)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !skip.iter().any(|s| name == *s)
        })
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() { continue; }
        let ext = entry.path().extension().and_then(|e| e.to_str()).unwrap_or("");
        if !["ts", "tsx", "js", "jsx", "py", "rs"].contains(&ext) { continue; }

        if let Ok(m) = entry.metadata() {
            if let Ok(modified) = m.modified() {
                if modified > index_modified {
                    changed += 1;
                }
            }
        }
    }
    changed
}

/// Check if the index is fresh. Returns None if fresh, Some(warning) if stale.
pub fn check_freshness(repo: &str, repo_path: &str) -> Option<String> {
    let path = state_path(repo);
    let saved = std::fs::read_to_string(&path).ok()?;
    let mut lines = saved.lines();
    let saved_head = lines.next()?;
    let saved_branch = lines.next().unwrap_or("?");

    // Check file modifications first (catches unsaved work)
    let changed_files = count_changed_files(repo, repo_path);
    if changed_files > 0 {
        return Some(format!(
            "{} file{} modified since last index. Results may be incomplete. Run: savants reindex",
            changed_files, if changed_files == 1 { "" } else { "s" }
        ));
    }

    let current_head = get_git_head(repo_path)?;
    let current_branch = get_git_branch(repo_path).unwrap_or_else(|| "?".to_string());

    if saved_head == current_head {
        return None; // Fresh
    }

    if saved_branch != current_branch {
        Some(format!(
            "Index is from branch '{}', you're now on '{}'. Run: savants reindex",
            saved_branch, current_branch
        ))
    } else {
        Some("New commits since last index. Run: savants reindex".to_string())
    }
}
