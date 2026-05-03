use colored::*;
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn find_savants_binary() -> String {
    if let Ok(exe) = env::current_exe() {
        return exe.to_string_lossy().to_string();
    }
    std::process::Command::new("which")
        .arg("savants")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "savants".to_string())
}

fn mcp_config_json() -> serde_json::Value {
    let bin = find_savants_binary();

    // Cloud mode is auto-detected from state file - no env vars needed
    json!({
        "command": bin,
        "args": ["serve"]
    })
}

/// All savants MCP tool names (used for allowlist).
const TOOL_NAMES: &[&str] = &[
    "semantic_search",
    "file_skeleton",
    "where_used",
    "callers",
    "blast_radius",
    "dead_code",
    "git_blame",
    "git_log",
    "reindex",
    "session_stats",
];

pub fn install(scope: &str, tool: &str) {
    let has_claude = std::process::Command::new("which")
        .arg("claude")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let has_cursor = dirs::home_dir()
        .map(|h| h.join(".cursor").exists())
        .unwrap_or(false);

    let target = match tool {
        "claude" => "claude",
        "cursor" => "cursor",
        _ => {
            if has_claude {
                "claude"
            } else if has_cursor {
                "cursor"
            } else {
                "claude"
            }
        }
    };

    let config = mcp_config_json();

    // Global install via claude mcp add-json
    if target == "claude" && has_claude && scope == "user" {
        let json_str = serde_json::to_string(&config).unwrap();
        println!("Registering with Claude Code...");
        let result = Command::new("claude")
            .args(["mcp", "add-json", "--scope", "user", "savants", &json_str])
            .output();

        match result {
            Ok(out) if out.status.success() => {
                add_to_claude_allowlist();
                println!();
                println!(
                    "{}",
                    "Savants MCP server registered with Claude Code.".green()
                );
                println!("All savants tools auto-approved (read-only).");
                println!("Restart Claude Code to activate.");
                return;
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                eprintln!("claude mcp add-json failed: {}", stderr.trim());
                eprintln!("Falling back to .mcp.json...");
            }
            Err(e) => {
                eprintln!("Failed to run claude: {}", e);
                eprintln!("Falling back to .mcp.json...");
            }
        }
    }

    // Cursor config
    if target == "cursor" {
        let config_path = PathBuf::from(".cursor/mcp.json");
        write_mcp_json(&config_path, &config);
        return;
    }

    // Write to project root AND home directory
    let config_path = PathBuf::from(".mcp.json");
    write_mcp_json(&config_path, &config);

    // Also update ~/.mcp.json so it's consistent
    if let Some(home) = dirs::home_dir() {
        let home_mcp = home.join(".mcp.json");
        if home_mcp != std::fs::canonicalize(&config_path).unwrap_or_default() {
            write_mcp_json(&home_mcp, &config);
        }
    }

    // Find and fix any other .mcp.json files that have savants without cloud URL
    fix_stale_mcp_configs(&config);

    add_to_claude_allowlist();
}

/// Add all savants MCP tools to Claude Code's allowlist.
/// These are read-only tools, safe to auto-approve.
fn add_to_claude_allowlist() {
    let settings_path = match dirs::home_dir() {
        Some(h) => h.join(".claude").join("settings.json"),
        None => return,
    };

    let mut settings: serde_json::Value = if settings_path.exists() {
        fs::read_to_string(&settings_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| json!({}))
    } else {
        json!({})
    };

    let permissions = settings
        .as_object_mut()
        .unwrap()
        .entry("permissions")
        .or_insert_with(|| json!({}));
    let allow = permissions
        .as_object_mut()
        .unwrap()
        .entry("allow")
        .or_insert_with(|| json!([]));

    let allow_arr = allow.as_array_mut().unwrap();

    // Add wildcard pattern for all savants MCP tools
    let pattern = json!("mcp__savants__*");
    if !allow_arr.contains(&pattern) {
        allow_arr.push(pattern);
    }

    if let Some(parent) = settings_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let content = serde_json::to_string_pretty(&settings).unwrap() + "\n";
    if let Err(e) = fs::write(&settings_path, &content) {
        eprintln!("Warning: could not update Claude settings: {}", e);
    }
}

/// Find .mcp.json files in common locations and ensure savants config is up to date.
/// Prevents the scenario where a project-level .mcp.json overrides the home one
/// with stale config (missing SAVANTS_CLOUD_URL, wrong binary path, etc.)
fn fix_stale_mcp_configs(current_config: &serde_json::Value) {
    let locations = [
        // Home directory
        dirs::home_dir().map(|h| h.join(".mcp.json")),
        // Current directory
        Some(PathBuf::from(".mcp.json")),
        // Common git project roots (scan parent directories)
        std::env::current_dir().ok().and_then(|mut d| {
            for _ in 0..5 {
                let mcp = d.join(".mcp.json");
                if mcp.exists() {
                    return Some(mcp);
                }
                if !d.pop() {
                    break;
                }
            }
            None
        }),
    ];

    for loc in locations.iter().flatten() {
        if !loc.exists() {
            continue;
        }

        let content = match fs::read_to_string(loc) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut config: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Check if savants entry exists but is outdated
        if let Some(servers) = config.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
            if let Some(savants) = servers.get("savants") {
                let needs_update = savants.get("env").is_some() // Remove env vars (cloud auto-detected now)
                    || savants.get("command").and_then(|v| v.as_str())
                        .map(|c| !c.contains(".savants/bin/"))
                        .unwrap_or(false); // Update old binary paths

                if needs_update {
                    servers.insert("savants".to_string(), current_config.clone());
                    let updated = serde_json::to_string_pretty(&config).unwrap() + "\n";
                    let _ = fs::write(loc, &updated);
                    println!(
                        "  Updated {}",
                        loc.display().to_string().cyan()
                    );
                }
            }
        }
    }
}

fn write_mcp_json(path: &Path, server_config: &serde_json::Value) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let mut existing: serde_json::Value = if path.exists() {
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| json!({}))
    } else {
        json!({})
    };

    existing
        .as_object_mut()
        .unwrap()
        .entry("mcpServers")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .unwrap()
        .insert("savants".to_string(), server_config.clone());

    let content = serde_json::to_string_pretty(&existing).unwrap() + "\n";
    fs::write(path, &content).expect("Failed to write MCP config");

    println!("Wrote {}", path.display().to_string().cyan());
    println!();
    println!("{}", "Savants MCP server configured.".green());
    println!("All savants tools auto-approved (read-only).");
    println!("Restart your AI tool to activate.");
    println!();
    println!("Tools:");
    for name in TOOL_NAMES {
        println!("  {}", name.cyan());
    }
}

pub fn status() {
    println!("{}", "MCP Server Status".bold());
    println!();

    let bin = find_savants_binary();
    println!("  Binary: {}", bin.cyan());
    println!("  Command: {} serve", bin);
    println!();

    // Check .mcp.json
    let mcp_path = PathBuf::from(".mcp.json");
    if mcp_path.exists() {
        println!("  {} .mcp.json found", "●".green());
    } else {
        println!("  {} .mcp.json not found", "●".yellow());
        println!("  Run: {}", "savants mcp install".cyan());
    }
}
