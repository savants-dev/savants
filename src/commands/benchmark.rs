//! Session benchmark - analyze Claude Code session for token usage and tool call stats.
//! Reads the Claude Code session log and produces a comprehensive report.

use colored::*;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Default)]
struct SessionReport {
    total_input_tokens: u64,
    total_output_tokens: u64,
    total_cache_read: u64,
    total_tool_calls: u32,
    tool_calls_by_name: HashMap<String, u32>,
    savants_calls: u32,
    grep_calls: u32,
    read_calls: u32,
    bash_calls: u32,
    glob_calls: u32,
    write_calls: u32,
    edit_calls: u32,
    agent_calls: u32,
    other_calls: u32,
    message_count: u32,
}

pub fn run(session_id: Option<String>) {
    println!("{}", "Session Benchmark".bold());
    println!();

    // Find the session log
    let home = dirs::home_dir().unwrap_or_default();
    let claude_dir = home.join(".claude").join("projects");

    let log_path = if let Some(ref id) = session_id {
        // Direct session ID
        find_session_by_id(&claude_dir, id)
    } else {
        // Find the most recent session
        find_latest_session(&claude_dir)
    };

    let log_path = match log_path {
        Some(p) => p,
        None => {
            eprintln!("{}: No Claude Code session found.", "Error".red());
            eprintln!("  Run this from a directory where you've used Claude Code.");
            return;
        }
    };

    println!("  Session: {}", log_path.display().to_string().dimmed());

    // Parse the log
    let content = match std::fs::read_to_string(&log_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: Failed to read session log: {}", "Error".red(), e);
            return;
        }
    };

    let mut report = SessionReport::default();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }

        let parsed: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let msg_type = parsed.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if msg_type != "assistant" { continue; }

        let msg = match parsed.get("message") {
            Some(m) => m,
            None => continue,
        };

        // Count tokens
        if let Some(usage) = msg.get("usage") {
            report.total_input_tokens += usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            report.total_output_tokens += usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            report.total_cache_read += usage.get("cache_read_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
        }

        report.message_count += 1;

        // Count tool calls
        if let Some(content) = msg.get("content").and_then(|v| v.as_array()) {
            for item in content {
                if item.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                    let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                    report.total_tool_calls += 1;
                    *report.tool_calls_by_name.entry(name.to_string()).or_insert(0) += 1;

                    // Categorize
                    if name.starts_with("mcp__savants__") {
                        report.savants_calls += 1;
                    } else {
                        match name {
                            "Grep" => report.grep_calls += 1,
                            "Read" => report.read_calls += 1,
                            "Bash" => report.bash_calls += 1,
                            "Glob" => report.glob_calls += 1,
                            "Write" => report.write_calls += 1,
                            "Edit" => report.edit_calls += 1,
                            "Agent" => report.agent_calls += 1,
                            _ => report.other_calls += 1,
                        }
                    }
                }
            }
        }
    }

    // Print report
    println!();
    println!("{}", "  Token Usage".bold());
    println!("  ├── Input:      {:>12}", format_tokens(report.total_input_tokens));
    println!("  ├── Output:     {:>12}", format_tokens(report.total_output_tokens));
    println!("  ├── Cache read: {:>12}", format_tokens(report.total_cache_read));
    println!("  └── Total:      {:>12}", format_tokens(report.total_input_tokens + report.total_output_tokens).bold());

    let est_cost = estimate_cost(report.total_input_tokens, report.total_output_tokens, report.total_cache_read);
    println!();
    println!("  {} Estimated cost: {}", "●".cyan(), format!("${:.2}", est_cost).bold());

    println!();
    println!("{}", "  Tool Calls".bold());
    println!("  ├── Total:     {:>6}", report.total_tool_calls.to_string().bold());

    if report.savants_calls > 0 {
        println!("  ├── {} {:>6}  {}", "Savants:".cyan(), report.savants_calls, "★".yellow());
    }
    if report.grep_calls > 0 {
        println!("  ├── Grep:      {:>6}", report.grep_calls);
    }
    if report.read_calls > 0 {
        println!("  ├── Read:      {:>6}", report.read_calls);
    }
    if report.bash_calls > 0 {
        println!("  ├── Bash:      {:>6}", report.bash_calls);
    }
    if report.glob_calls > 0 {
        println!("  ├── Glob:      {:>6}", report.glob_calls);
    }
    if report.write_calls > 0 {
        println!("  ├── Write:     {:>6}", report.write_calls);
    }
    if report.edit_calls > 0 {
        println!("  ├── Edit:      {:>6}", report.edit_calls);
    }
    if report.agent_calls > 0 {
        println!("  ├── Agent:     {:>6}", report.agent_calls);
    }
    if report.other_calls > 0 {
        println!("  └── Other:     {:>6}", report.other_calls);
    }

    // Savants impact
    if report.savants_calls > 0 {
        println!();
        println!("{}", "  Savants Impact".bold());

        // Each savants call replaces ~8 grep/read calls and ~3000 tokens
        let search_calls = report.tool_calls_by_name.iter()
            .filter(|(k, _)| k.contains("semantic_search") || k.contains("search_code"))
            .map(|(_, v)| v)
            .sum::<u32>();
        let caller_calls = report.tool_calls_by_name.iter()
            .filter(|(k, _)| k.contains("callers") || k.contains("where_used"))
            .map(|(_, v)| v)
            .sum::<u32>();
        let skeleton_calls = report.tool_calls_by_name.iter()
            .filter(|(k, _)| k.contains("file_skeleton"))
            .map(|(_, v)| v)
            .sum::<u32>();

        let estimated_replaced_calls = search_calls * 8 + caller_calls * 5 + skeleton_calls * 3;
        let estimated_saved_tokens = search_calls as u64 * 3000 + caller_calls as u64 * 2000 + skeleton_calls as u64 * 2400;

        println!("  ├── Searches:       {:>4} (replaced ~{} grep+read cycles)", search_calls, search_calls * 8);
        println!("  ├── Caller lookups: {:>4} (replaced ~{} grep cycles)", caller_calls, caller_calls * 5);
        println!("  ├── Skeletons:      {:>4} (replaced ~{} full file reads)", skeleton_calls, skeleton_calls * 3);
        println!("  ├── Est. calls saved: {}", estimated_replaced_calls.to_string().green().bold());
        println!("  └── Est. tokens saved: {}", format_tokens(estimated_saved_tokens).green().bold());

        if estimated_saved_tokens > 0 {
            let pct = (estimated_saved_tokens as f64 / (estimated_saved_tokens + report.total_output_tokens) as f64 * 100.0) as u32;
            println!();
            println!("  {} Token reduction: {}%", "★".yellow(), pct.to_string().green().bold());
        }
    }

    // Top tools
    if !report.tool_calls_by_name.is_empty() {
        println!();
        println!("{}", "  Top Tools".bold());
        let mut sorted: Vec<_> = report.tool_calls_by_name.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (i, (name, count)) in sorted.iter().take(10).enumerate() {
            let bar_len = (**count as f32 / *sorted[0].1 as f32 * 20.0) as usize;
            let bar: String = "█".repeat(bar_len);
            println!("  {:>2}. {:<30} {:>4}  {}", i + 1, name, count, bar.cyan());
        }
    }
}

fn find_latest_session(claude_dir: &std::path::Path) -> Option<PathBuf> {
    let mut latest: Option<(PathBuf, std::time::SystemTime)> = None;

    if let Ok(entries) = std::fs::read_dir(claude_dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if let Ok(files) = std::fs::read_dir(entry.path()) {
                    for file in files.flatten() {
                        let path = file.path();
                        if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                            if let Ok(meta) = file.metadata() {
                                if let Ok(modified) = meta.modified() {
                                    if latest.as_ref().map(|(_, t)| modified > *t).unwrap_or(true) {
                                        latest = Some((path, modified));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    latest.map(|(p, _)| p)
}

fn find_session_by_id(claude_dir: &std::path::Path, id: &str) -> Option<PathBuf> {
    if let Ok(entries) = std::fs::read_dir(claude_dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let path = entry.path().join(format!("{}.jsonl", id));
                if path.exists() {
                    return Some(path);
                }
            }
        }
    }
    None
}

fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}K", tokens as f64 / 1_000.0)
    } else {
        format!("{}", tokens)
    }
}

fn estimate_cost(input: u64, output: u64, cache_read: u64) -> f64 {
    // Claude Opus pricing (approximate)
    let input_cost = (input - cache_read) as f64 * 15.0 / 1_000_000.0; // $15/M input
    let cache_cost = cache_read as f64 * 1.5 / 1_000_000.0; // $1.50/M cache read
    let output_cost = output as f64 * 75.0 / 1_000_000.0; // $75/M output
    input_cost + cache_cost + output_cost
}
