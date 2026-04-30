use crate::config::State;
use colored::*;

pub fn run() {
    println!("{}", "Savants Status".bold());
    println!();

    // Cloud connection - check state file, not env var
    let state = State::load();
    if state.is_cloud_authenticated() {
        let org = state.cloud_org.as_deref().unwrap_or("connected");
        println!(
            "  {} Cloud: {} (org: {})",
            "●".green(),
            "connected".green(),
            org.cyan()
        );
    } else if std::env::var("SAVANTS_CLOUD_URL").is_ok() {
        println!(
            "  {} Cloud: {} (cloud mode)",
            "●".green(),
            "api.savants.cloud".cyan()
        );
    } else {
        println!("  {} Cloud: {}", "●".dimmed(), "not connected".dimmed());
        println!("    Run {} for team features.", "savants connect".cyan());
    }

    // Local embeddings
    let cwd = std::env::current_dir().unwrap_or_default();
    let repo = cwd
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();

    if !repo.is_empty() && crate::embedding_store::EmbeddingStore::exists(&repo) {
        if let Ok(store) = crate::embedding_store::EmbeddingStore::load(&repo) {
            println!(
                "  {} Search: {} functions indexed",
                "●".green(),
                store.entries.len()
            );
        }
    } else {
        println!("  {} Search: not indexed", "●".dimmed());
    }
}
