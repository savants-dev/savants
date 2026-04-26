use colored::*;

pub fn run() {
    println!("{}", "Savants Status".bold());
    println!();

    // Cloud connection
    if let Ok(url) = std::env::var("SAVANTS_CLOUD_URL") {
        println!("  {} Cloud: {} (connected)", "●".green(), url.cyan());
    } else {
        println!("  {} Cloud: {}", "●".dimmed(), "not connected".dimmed());
        println!("    Run {} for team features.", "savants connect".cyan());
    }

    // Local embeddings
    let cwd = std::env::current_dir().unwrap_or_default();
    let repo = cwd.file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();

    if !repo.is_empty() && crate::embedding_store::EmbeddingStore::exists(&repo) {
        if let Ok(store) = crate::embedding_store::EmbeddingStore::load(&repo) {
            println!("  {} Search: {} functions indexed", "●".green(), store.entries.len());
        }
    } else {
        println!("  {} Search: not indexed", "●".dimmed());
    }
}
