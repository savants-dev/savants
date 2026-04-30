//! Savants - The context engine for your LLM.
//!
//! Semantic code search, file structure, import trees.
//! Works offline, no API keys. Upgrade to savants.cloud for team features.

use clap::{Parser, Subcommand};
use colored::*;

mod call_index;
mod code_parser;
mod commands;
mod config;
mod embedding_store;
mod embeddings;
mod freshness;
mod mcp;
mod semantic_search;
mod update_check;

#[derive(Parser)]
#[command(name = "savants")]
#[command(
    about = "The context engine for your LLM. Semantic search, code intelligence, MCP tools."
)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Auto-detect your environment and build the context
    Up,
    /// Show context engine status
    Status,
    /// Start the MCP server (for Claude Code / Cursor / Windsurf)
    Serve,
    /// Connect to savants.cloud for team features
    Connect,
    /// Show usage this month
    Usage,
    /// Re-index the current repository
    Reindex {
        #[arg(long)]
        repo_path: Option<String>,
    },
    /// Register savants MCP server with your AI tool
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },
    /// Search and manage documentation sources
    Docs {
        #[command(subcommand)]
        action: DocsAction,
    },

}

#[derive(Subcommand)]
enum McpAction {
    /// Install savants MCP server for Claude Code / Cursor / Windsurf
    Install {
        /// Scope: "user" (global) or "project" (current dir)
        #[arg(long, default_value = "user")]
        scope: String,
        /// Target tool: "claude", "cursor", or auto-detect
        #[arg(long, default_value = "auto")]
        tool: String,
    },
    /// Show MCP server status
    Status,
}

#[derive(Subcommand)]
enum DocsAction {
    /// List available documentation sources
    List,
    /// Search a documentation source
    Search {
        /// Documentation provider (e.g. stripe, cloudflare, react)
        provider: String,
        /// Search query
        query: Vec<String>,
    },
    /// Upload local markdown docs to savants.cloud
    Upload {
        /// Path to directory containing markdown files
        path: String,
        /// Project name for the uploaded docs
        #[arg(long)]
        project: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Background update check (non-blocking, cached 24h)
    update_check::check_background();

    match cli.command {
        Commands::Up => {
            commands::up::run().await;
        }
        Commands::Status => {
            commands::status::run();
        }
        Commands::Serve => {
            let cloud_url = std::env::var("SAVANTS_CLOUD_URL").ok();
            let api_key = std::env::var("SAVANTS_API_KEY")
                .ok()
                .filter(|k| !k.is_empty())
                .or_else(|| {
                    // Fall back to JWT from state file (set by savants connect)
                    let state = config::State::load();
                    state.cloud_token.clone()
                })
                .unwrap_or_default();

            if let Some(url) = cloud_url {
                let proxy = mcp::CloudProxyServer::new(&url, &api_key);
                proxy.run();
            } else {
                // Offline mode: serve local-only tools (semantic search, file skeleton)
                let server = mcp::OfflineServer::new();
                server.run();
            }
        }
        Commands::Connect => {
            commands::connect::run().await;
        }
        Commands::Usage => {
            commands::usage::run().await;
        }
        Commands::Mcp { action } => match action {
            McpAction::Install { scope, tool } => {
                commands::mcp::install(&scope, &tool);
            }
            McpAction::Status => {
                commands::mcp::status();
            }
        },
        Commands::Docs { action } => match action {
            DocsAction::List => {
                commands::docs::list().await;
            }
            DocsAction::Search { provider, query } => {
                let q = query.join(" ");
                commands::docs::search(&provider, &q).await;
            }
            DocsAction::Upload { path, project } => {
                commands::docs::upload(&path, &project).await;
            }
        },
        Commands::Reindex { repo_path } => {
            let path = repo_path.unwrap_or_else(|| {
                std::env::current_dir()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            });
            let repo_name = std::path::Path::new(&path)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());

            println!("{}", "Indexing...".bold());
            let mut parser = code_parser::CodeParser::new(&repo_name);
            let result = parser.parse_repo(&path);
            println!(
                "  Parsed {} files, {} entities",
                result.files,
                result.entities.len()
            );

            // Build and cache call index (callers, importers)
            let ci = call_index::CallIndex::from_parse_result(&result);
            let caller_count: usize = ci.callers.values().map(|v| v.len()).sum();
            match ci.save(&repo_name) {
                Ok(_) => println!("  Cached {} call relationships", caller_count),
                Err(e) => eprintln!("  Warning: call index: {}", e),
            }

            // Build and cache embeddings
            match embeddings::EmbeddingEngine::new() {
                Ok(mut engine) => {
                    match semantic_search::SemanticIndex::from_parse_result(&result, &mut engine) {
                        Ok(index) => {
                            let dim = engine
                                .embed_one("test")
                                .map(|v| v.len() as u32)
                                .unwrap_or(128);
                            let mut store = embedding_store::EmbeddingStore::new(dim);
                            for (entry, emb) in index.entries_with_embeddings() {
                                let kind = match entry.kind.as_str() {
                                    "class" => 1,
                                    "interface" => 2,
                                    _ => 0,
                                };
                                store.add(
                                    &entry.name,
                                    &entry.file,
                                    entry.line as u32,
                                    kind,
                                    emb.clone(),
                                );
                            }
                            match store.save(&repo_name) {
                                Ok(_) => println!(
                                    "  Cached {} embeddings for instant search",
                                    store.entries.len()
                                ),
                                Err(e) => eprintln!("  Warning: {}", e),
                            }
                        }
                        Err(e) => eprintln!("  Embedding index: {}", e),
                    }
                }
                Err(e) => {
                    eprintln!("  Embedding engine unavailable: {}", e);
                    println!("  Semantic search will use keyword fallback");
                }
            }

            // If cloud connected, upload to cloud
            if let Ok(cloud_url) = std::env::var("SAVANTS_CLOUD_URL") {
                let api_key = std::env::var("SAVANTS_API_KEY").unwrap_or_default();
                println!("  Uploading to savants.cloud...");
                let body = serde_json::to_string(&result).unwrap_or_default();
                let output = std::process::Command::new("curl")
                    .args([
                        "-sf",
                        "--max-time",
                        "60",
                        "-X",
                        "POST",
                        "-H",
                        &format!("Authorization: Bearer {}", api_key),
                        "-H",
                        "Content-Type: application/json",
                        "-d",
                        &body,
                        &format!("{}/api/v1/ingest", cloud_url),
                    ])
                    .output();
                match output {
                    Ok(o) if o.status.success() => println!("  {} Uploaded to cloud", "●".green()),
                    _ => println!(
                        "  {} Cloud upload failed (tools work locally)",
                        "●".yellow()
                    ),
                }
            }

            println!("{}", "Done.".green().bold());
        }
    }
}


