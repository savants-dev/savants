use colored::*;

pub async fn run() {
    println!("{}", "Starting Savants...".bold());
    println!();

    // Detect environment
    let cwd = std::env::current_dir().unwrap_or_default();
    let has_git = cwd.join(".git").exists();
    let repo_name = cwd.file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    if has_git {
        println!("  {} Git repo: {}", "●".green(), repo_name.cyan());
    } else {
        println!("  {} No git repo detected", "●".yellow());
    }

    // Check cloud connection
    if std::env::var("SAVANTS_CLOUD_URL").is_ok() {
        println!("  {} Context: {} (cloud mode)", "●".green(), "api.savants.cloud".cyan());
    } else {
        println!("  {} Context: {} (offline)", "●".yellow(), "local".dimmed());
        println!("    Run {} for team features.", "savants connect".cyan());
    }

    // Check if embeddings are cached
    if crate::embedding_store::EmbeddingStore::exists(&repo_name) {
        let store = crate::embedding_store::EmbeddingStore::load(&repo_name).ok();
        if let Some(s) = store {
            println!("  {} Search index: {} functions cached", "●".green(), s.entries.len());
        }
    } else if has_git {
        println!("  {} Search index: not built yet", "●".yellow());
        println!("    Run {} to enable semantic search.", "savants reindex".cyan());
    }

    println!();
}
