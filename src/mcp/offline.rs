//! Offline MCP server: serves local-only tools (semantic search, file skeleton)
//! without any cloud or database connection.

use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::time::Instant;

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// Session statistics - tracks every tool call for ROI measurement.
#[derive(Default)]
struct SessionStats {
    start_time: Option<Instant>,
    total_calls: u32,
    calls_by_tool: std::collections::HashMap<String, u32>,
    total_tokens_returned: u64,
    total_duration_ms: u64,
    searches_performed: u32,
    files_skeletonized: u32,
    callers_found: u32,
    usages_found: u32,
}

impl SessionStats {
    fn record_call(&mut self, tool: &str, result_len: usize, duration_ms: u64) {
        if self.start_time.is_none() {
            self.start_time = Some(Instant::now());
        }
        self.total_calls += 1;
        *self.calls_by_tool.entry(tool.to_string()).or_insert(0) += 1;
        // Estimate tokens: ~4 chars per token
        self.total_tokens_returned += (result_len / 4) as u64;
        self.total_duration_ms += duration_ms;

        match tool {
            "semantic_search" => self.searches_performed += 1,
            "file_skeleton" => self.files_skeletonized += 1,
            "callers" => self.callers_found += 1,
            "where_used" => self.usages_found += 1,
            _ => {}
        }
    }

    fn to_json(&self) -> Value {
        let session_seconds = self.start_time.map(|s| s.elapsed().as_secs()).unwrap_or(0);

        // Estimate what grep/read would have cost
        let estimated_grep_calls =
            self.searches_performed * 8 + self.callers_found * 5 + self.usages_found * 5;
        let estimated_grep_tokens =
            self.searches_performed as u64 * 3000 + self.files_skeletonized as u64 * 2400;
        let tokens_saved = estimated_grep_tokens.saturating_sub(self.total_tokens_returned);
        let time_saved_seconds = estimated_grep_calls as u64 * 5; // ~5s per grep+read cycle

        // ── Savants Efficiency Quotient (SEQ) ──
        //
        // Inspired by DORA + SPACE frameworks, adapted for AI agent context efficiency.
        //
        // What research says matters:
        //   DORA: outcomes over activity (don't measure lines of code)
        //   SPACE: multiple dimensions, never single metric alone
        //   Anthropic SWE-bench: cost per successful task is the real metric
        //   Anti-pattern: measuring tokens saved (activity) instead of problems solved (outcome)
        //
        // SEQ measures three things that actually matter:
        //
        //   1. PRECISION (0-40): Did savants return the RIGHT result?
        //      Measured by: ratio of savants calls to total search attempts.
        //      If you called semantic_search once and didn't need grep after = perfect precision.
        //      If you called savants then fell back to grep = low precision.
        //
        //   2. COST EFFICIENCY (0-35): How much context did the LLM consume?
        //      Measured by: tokens returned by savants vs estimated grep/read alternative.
        //      28x fewer tokens = high efficiency. Same tokens = no benefit.
        //
        //   3. VELOCITY (0-25): How fast did the AI get the answer?
        //      Measured by: avg response time and total session active time.
        //      Sub-second responses = maximum velocity.
        //
        // Total: 0-100 scale (intuitive, like a percentage)
        // NOT 0-1000 (inflated numbers feel fake)

        // Precision: 0-40
        // Were savants results good enough that no grep fallback was needed?
        let precision = if self.total_calls > 0 {
            // Perfect: all searches via savants, no grep needed
            // In practice: we only see savants calls, so precision = quality of results
            // Proxy: more diverse tool usage = finding answers from different angles = good
            let unique_tools = self.calls_by_tool.len() as f64;
            let diversity = (unique_tools / 4.0).min(1.0); // 4 tools = max diversity
            let first_call_hit = if self.searches_performed > 0 {
                0.7
            } else {
                0.3
            };
            ((first_call_hit + diversity * 0.3) * 40.0) as u32
        } else {
            0
        };

        // Cost efficiency: 0-35
        let cost_efficiency = if estimated_grep_tokens > 0 && self.total_tokens_returned > 0 {
            let ratio = tokens_saved as f64 / estimated_grep_tokens as f64;
            (ratio * 35.0).min(35.0) as u32
        } else {
            0
        };

        // Velocity: 0-25
        let velocity = if self.total_calls > 0 {
            let avg_ms = self.total_duration_ms / self.total_calls as u64;
            if avg_ms < 300 {
                25
            } else if avg_ms < 500 {
                22
            } else if avg_ms < 1000 {
                18
            } else if avg_ms < 2000 {
                12
            } else {
                5
            }
        } else {
            0
        };

        let seq = precision + cost_efficiency + velocity;

        let seq_label = if seq >= 90 {
            "Exceptional"
        } else if seq >= 75 {
            "Excellent"
        } else if seq >= 60 {
            "Good"
        } else if seq >= 40 {
            "Moderate"
        } else if seq > 0 {
            "Getting started"
        } else {
            "No data yet"
        };

        json!({
            "seq": {
                "score": seq,
                "max": 100,
                "label": seq_label,
                "breakdown": {
                    "precision": { "score": precision, "max": 40, "what": "Did savants return the right result without needing grep fallback?" },
                    "cost_efficiency": { "score": cost_efficiency, "max": 35, "what": "How many tokens saved vs grep/read approach?" },
                    "velocity": { "score": velocity, "max": 25, "what": "How fast were the responses?" },
                },
            },
            "session": {
                "duration_seconds": session_seconds,
                "total_tool_calls": self.total_calls,
                "total_tokens_returned": self.total_tokens_returned,
                "total_duration_ms": self.total_duration_ms,
                "avg_response_ms": if self.total_calls > 0 { self.total_duration_ms / self.total_calls as u64 } else { 0 },
            },
            "by_tool": self.calls_by_tool,
            "savings": {
                "tokens_returned": self.total_tokens_returned,
                "estimated_without_savants_tokens": estimated_grep_tokens,
                "tokens_saved": tokens_saved,
                "token_reduction_percent": if estimated_grep_tokens > 0 {
                    ((tokens_saved as f64 / estimated_grep_tokens as f64) * 100.0) as u32
                } else { 0 },
                "estimated_grep_calls_avoided": estimated_grep_calls,
                "estimated_time_saved_seconds": time_saved_seconds,
            },
            "summary": format!(
                "SEQ: {}/100 ({}). {} savants calls, ~{} tokens. Without savants: ~{} grep/read calls, ~{} tokens. Saved {}% tokens, ~{}s.",
                seq, seq_label,
                self.total_calls,
                self.total_tokens_returned,
                estimated_grep_calls,
                estimated_grep_tokens,
                if estimated_grep_tokens > 0 { ((tokens_saved as f64 / estimated_grep_tokens as f64) * 100.0) as u32 } else { 0 },
                time_saved_seconds
            )
        })
    }
}

pub struct OfflineServer;

impl OfflineServer {
    pub fn new() -> Self {
        Self
    }

    pub fn run(&self) {
        eprintln!("Savants MCP server started (offline mode)");
        eprintln!("Connect to savants.cloud for full intelligence tools: savants connect");
        let stdin = io::stdin();
        let stdout = io::stdout();
        let mut stats = SessionStats::default();
        let reader = stdin.lock();
        let mut writer = stdout.lock();

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let message: Value = match serde_json::from_str(trimmed) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if let Some(response) = self.handle_message(&message, &mut stats) {
                let body = serde_json::to_string(&response).unwrap();
                let _ = writeln!(writer, "{}", body);
                let _ = writer.flush();
            }
        }
    }

    fn handle_message(&self, message: &Value, stats: &mut SessionStats) -> Option<Value> {
        let method = message.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let params = message.get("params").cloned().unwrap_or(json!({}));
        let req_id = message.get("id");
        if req_id.is_none() || req_id == Some(&Value::Null) {
            return None;
        }
        let req_id = req_id.unwrap().clone();

        match method {
            "initialize" => Some(self.response(&req_id, json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {"tools": {"listChanged": false}, "resources": {}, "prompts": {}},
                "serverInfo": {"name": "savants", "version": "0.1.0-offline"}
            }))),
            "ping" => Some(self.response(&req_id, json!({}))),
            "tools/list" => Some(self.response(&req_id, json!({"tools": self.list_tools()}))),
            "tools/call" => {
                let tool = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));

                // Handle session_stats specially
                if tool == "session_stats" {
                    let stats_json = stats.to_json();
                    let text = serde_json::to_string_pretty(&stats_json).unwrap_or_default();
                    return Some(self.response(&req_id, json!({"content": [{"type": "text", "text": text}]})));
                }

                let start = Instant::now();
                let result = self.call_tool(tool, &args);
                let duration_ms = start.elapsed().as_millis() as u64;

                // Opt-in telemetry: tool name + duration only
                crate::telemetry::send(tool, duration_ms);

                match result {
                    Ok(ref text) => {
                        stats.record_call(tool, text.len(), duration_ms);
                        Some(self.response(&req_id, json!({"content": [{"type": "text", "text": text}]})))
                    },
                    Err(e) => {
                        // Return as content, not isError - errors make Claude avoid the tool
                        let msg = format!("Could not complete: {}. Try a different search term or use Grep as fallback.", e);
                        Some(self.response(&req_id, json!({"content": [{"type": "text", "text": msg}]})))
                    },
                }
            }
            "resources/list" => Some(self.response(&req_id, json!({"resources": []}))),
            "prompts/list" => Some(self.response(&req_id, json!({"prompts": []}))),
            _ => Some(json!({"jsonrpc": "2.0", "id": req_id, "error": {"code": -32601, "message": format!("Unknown: {}", method)}})),
        }
    }

    fn list_tools(&self) -> Value {
        json!([
            {
                "name": "semantic_search",
                "description": "Use BEFORE Grep. Finds code by meaning: 'payment retry logic' returns handleTransactionWithBackoff even though those words don't appear in it. Grep can't do concept search. Returns function name, file, line, relevance score. Auto-indexes on first use.",
                "inputSchema": {"type": "object", "properties": {
                    "query": {"type": "string", "description": "Natural language: what the code does, not what it's named"},
                    "repo": {"type": "string", "description": "Repository name (auto-detected from cwd if omitted)"},
                    "limit": {"type": "integer", "description": "Max results (default 10)"}
                }, "required": ["query"]}
            },
            {
                "name": "file_skeleton",
                "description": "Use BEFORE Read. Returns every function name, class, and type in a file with line numbers - no bodies. Use this to decide WHICH functions to read instead of reading the entire file. 10x fewer tokens. Works without indexing.",
                "inputSchema": {"type": "object", "properties": {
                    "file": {"type": "string", "description": "File path relative to repo root"},
                    "repo": {"type": "string", "description": "Repository name (auto-detected from cwd if omitted)"}
                }, "required": ["file"]}
            },
            {
                "name": "where_used",
                "description": "Use BEFORE Grep for usage search. Returns every caller and importer of a function across the entire codebase from the pre-built call index. Grep matches text; this returns verified structural references with file and function context.",
                "inputSchema": {"type": "object", "properties": {
                    "symbol": {"type": "string", "description": "Function or symbol name"},
                    "repo": {"type": "string", "description": "Repository name (auto-detected from cwd if omitted)"}
                }, "required": ["symbol"]}
            },
            {
                "name": "callers",
                "description": "Use BEFORE Grep for caller search. Returns the exact functions that call a given function, from the pre-built call index. No false positives from variable names or comments matching.",
                "inputSchema": {"type": "object", "properties": {
                    "function": {"type": "string", "description": "Function name"},
                    "repo": {"type": "string", "description": "Repository name (auto-detected from cwd if omitted)"}
                }, "required": ["function"]}
            },
            {
                "name": "blast_radius",
                "description": "Use BEFORE changing a function. Shows every function that directly or transitively depends on it - what breaks if you change this. Uses the local call index. Essential for safe refactoring.",
                "inputSchema": {"type": "object", "properties": {
                    "function": {"type": "string", "description": "Function name to analyze"},
                    "repo": {"type": "string", "description": "Repository name (auto-detected from cwd if omitted)"},
                    "depth": {"type": "integer", "description": "Max traversal depth (default 5)"}
                }, "required": ["function"]}
            },
            {
                "name": "dead_code",
                "description": "Find functions with zero callers in the codebase - candidates for removal. Uses the local call index. Essential for cleanup during refactoring.",
                "inputSchema": {"type": "object", "properties": {
                    "repo": {"type": "string", "description": "Repository name (auto-detected from cwd if omitted)"},
                    "file": {"type": "string", "description": "Limit to a specific file (optional)"}
                }}
            },
            {
                "name": "reindex",
                "description": "Rebuild the code index for a repository. Usually not needed - indexing happens automatically on first tool use. Use this to force a refresh after major changes.",
                "inputSchema": {"type": "object", "properties": {
                    "repo_path": {"type": "string", "description": "Absolute path to repository"}
                }, "required": ["repo_path"]}
            },
            {
                "name": "git_blame",
                "description": "Use BEFORE running Bash git blame. Returns who wrote specific lines, when, the commit hash, and commit message. Use after finding a bug to trace when and why it was introduced.",
                "inputSchema": {"type": "object", "properties": {
                    "file": {"type": "string", "description": "File path relative to repo root"},
                    "line_start": {"type": "integer", "description": "Start line number"},
                    "line_end": {"type": "integer", "description": "End line number (defaults to line_start)"},
                    "repo_path": {"type": "string", "description": "Path to repository (defaults to cwd)"}
                }, "required": ["file", "line_start"]}
            },
            {
                "name": "git_log",
                "description": "Use BEFORE running Bash git log. Shows commit history for a file or tracks a specific function's evolution over time using git log -L. Returns author, date, message per commit.",
                "inputSchema": {"type": "object", "properties": {
                    "file": {"type": "string", "description": "File path (optional - omit for full repo history)"},
                    "function_name": {"type": "string", "description": "Track a specific function's history (uses git log -L for precise function-level tracking)"},
                    "limit": {"type": "integer", "description": "Max commits (default 10)"},
                    "repo_path": {"type": "string", "description": "Path to repository (defaults to cwd)"}
                }}
            },
            {
                "name": "session_stats",
                "description": "Shows how efficiently savants tools were used this session: tokens saved vs grep/read approach, response speed, and precision score (0-100).",
                "inputSchema": {"type": "object", "properties": {}}
            }
        ])
    }

    /// Public entry point for cloud proxy to call local tools directly.
    pub fn call_tool_direct(&self, tool: &str, args: &Value) -> Result<String, String> {
        self.call_tool(tool, args)
    }

    fn call_tool(&self, tool: &str, args: &Value) -> Result<String, String> {
        // Auto-index: if a tool needs the index and it doesn't exist, build it now
        let needs_index = matches!(
            tool,
            "semantic_search"
                | "file_skeleton"
                | "where_used"
                | "callers"
                | "blast_radius"
                | "dead_code"
        );
        if needs_index {
            let repo = args
                .get("repo")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            if !crate::embedding_store::EmbeddingStore::exists(repo) {
                eprintln!("[savants] No index for '{}', auto-indexing...", repo);
                if let Some(path) = self.detect_repo_path(repo) {
                    let _ = self.do_reindex(&path);
                }
            }
        }

        match tool {
            "semantic_search" => self.tool_semantic_search(args),
            "file_skeleton" => self.tool_file_skeleton(args),
            "where_used" => self.tool_where_used(args),
            "callers" => self.tool_callers(args),
            "reindex" => self.tool_reindex(args),
            "blast_radius" => self.tool_blast_radius(args),
            "dead_code" => self.tool_dead_code(args),
            "git_blame" => self.tool_git_blame(args),
            "git_log" => self.tool_git_log(args),
            _ => Err(format!(
                "'{}' requires savants.cloud. Run: savants connect",
                tool
            )),
        }
    }

    /// Detect repo path: check cwd and parent dirs for a git repo matching the name
    fn detect_repo_path(&self, repo: &str) -> Option<String> {
        let cwd = std::env::current_dir().ok()?;

        // If cwd is the repo or its name matches
        if cwd.join(".git").exists() {
            let cwd_name = cwd.file_name()?.to_string_lossy();
            if cwd_name == repo || repo == "unknown" {
                return Some(cwd.to_string_lossy().to_string());
            }
        }

        // Check if repo is a subdirectory
        let sub = cwd.join(repo);
        if sub.join(".git").exists() {
            return Some(sub.to_string_lossy().to_string());
        }

        // Fallback: use cwd if it's a git repo
        if cwd.join(".git").exists() {
            return Some(cwd.to_string_lossy().to_string());
        }

        None
    }

    /// Core reindex logic shared by auto-index and explicit reindex tool
    fn do_reindex(&self, repo_path: &str) -> Result<String, String> {
        let repo_name = std::path::Path::new(repo_path)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let mut parser = crate::code_parser::CodeParser::new(&repo_name);
        let result = parser.parse_repo(repo_path);

        let ci = crate::call_index::CallIndex::from_parse_result(&result);
        let _ = ci.save(&repo_name);

        if let Some(head) = crate::freshness::get_git_head(repo_path) {
            let branch = crate::freshness::get_git_branch(repo_path)
                .unwrap_or_else(|| "unknown".to_string());
            crate::freshness::save_state(&repo_name, &head, &branch);
        }

        let mut engine = crate::embeddings::EmbeddingEngine::new()?;
        let index = crate::semantic_search::SemanticIndex::from_parse_result(&result, &mut engine)?;

        let dim = engine
            .embed_one("test")
            .map(|v| v.len() as u32)
            .unwrap_or(128);
        let mut store = crate::embedding_store::EmbeddingStore::new(dim);
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
        store.save(&repo_name)?;

        let msg = format!(
            "Indexed {}: {} files, {} entities",
            repo_name,
            result.files,
            store.entries.len()
        );
        eprintln!("[savants] {}", msg);
        Ok(msg)
    }

    fn tool_semantic_search(&self, args: &Value) -> Result<String, String> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or("query required")?;
        let repo = args
            .get("repo")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(10) as usize;

        if !crate::embedding_store::EmbeddingStore::exists(repo) {
            return Ok(format!(
                "No index for '{}'. Run: savants reindex --repo-path /path/to/{}",
                repo, repo
            ));
        }

        // Check if index is stale - if so, auto-reindex in the background
        let cwd = std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if let Some(_warning) = crate::freshness::check_freshness(repo, &cwd) {
            // Auto-reindex: the user shouldn't have to think about this
            eprintln!("[savants] Index stale, re-indexing {}...", repo);
            let mut parser = crate::code_parser::CodeParser::new(repo);
            let result = parser.parse_repo(&cwd);

            let ci = crate::call_index::CallIndex::from_parse_result(&result);
            let _ = ci.save(repo);

            if let Some(head) = crate::freshness::get_git_head(&cwd) {
                let branch =
                    crate::freshness::get_git_branch(&cwd).unwrap_or_else(|| "unknown".to_string());
                crate::freshness::save_state(repo, &head, &branch);
            }

            if let Ok(mut engine) = crate::embeddings::EmbeddingEngine::new() {
                if let Ok(index) =
                    crate::semantic_search::SemanticIndex::from_parse_result(&result, &mut engine)
                {
                    let dim = engine
                        .embed_one("test")
                        .map(|v| v.len() as u32)
                        .unwrap_or(128);
                    let mut store = crate::embedding_store::EmbeddingStore::new(dim);
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
                    let _ = store.save(repo);
                }
            }
            eprintln!(
                "[savants] Re-indexed {} ({} entities)",
                repo,
                result.entities.len()
            );
            // Fall through to search with the fresh index
        }

        let store = crate::embedding_store::EmbeddingStore::load(repo)?;
        let mut engine = crate::embeddings::EmbeddingEngine::new()?;
        let query_vec = engine.embed_one(query)?;
        let results = store.search(&query_vec, limit);

        if results.is_empty() {
            return Ok(format!("No results for '{}' in {}", query, repo));
        }

        let mut lines = vec![format!(
            "=== Semantic search: '{}' ({} results) ===",
            query,
            results.len()
        )];
        for (idx, score) in &results {
            let entry = &store.entries[*idx];
            lines.push(format!(
                "  {}:{} {}() [{:.3}]",
                entry.file, entry.line, entry.name, score
            ));
        }
        Ok(lines.join("\n"))
    }

    fn tool_file_skeleton(&self, args: &Value) -> Result<String, String> {
        let file = args
            .get("file")
            .and_then(|v| v.as_str())
            .ok_or("file required")?;
        let repo = args
            .get("repo")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        // Try index first
        if crate::embedding_store::EmbeddingStore::exists(repo) {
            let store = crate::embedding_store::EmbeddingStore::load(repo)?;
            let mut functions = vec![];
            let mut classes = vec![];

            for entry in &store.entries {
                if entry.file == file {
                    match entry.kind {
                        0 => functions.push(entry),
                        1 => classes.push(entry),
                        _ => {}
                    }
                }
            }

            if !functions.is_empty() || !classes.is_empty() {
                let mut lines = vec![format!("=== {} ===", file)];
                if !classes.is_empty() {
                    lines.push("Classes:".to_string());
                    for c in &classes {
                        lines.push(format!("  {} (line {})", c.name, c.line));
                    }
                }
                if !functions.is_empty() {
                    lines.push("Functions:".to_string());
                    for f in &functions {
                        lines.push(format!("  {}() (line {})", f.name, f.line));
                    }
                }
                return Ok(lines.join("\n"));
            }
        }

        // Fallback: parse the file directly from filesystem
        let file_path = self.resolve_file_path(file);
        if !std::path::Path::new(&file_path).exists() {
            return Err(format!("File not found: {}", file));
        }

        let content = std::fs::read_to_string(&file_path)
            .map_err(|e| format!("Cannot read {}: {}", file, e))?;

        let mut lines = vec![format!("=== {} (live parse) ===", file)];
        let mut line_num = 0;
        for line in content.lines() {
            line_num += 1;
            let trimmed = line.trim();
            // Match function/class/interface/type definitions
            if trimmed.starts_with("export ")
                || trimmed.starts_with("pub ")
                || trimmed.starts_with("function ")
                || trimmed.starts_with("class ")
                || trimmed.starts_with("interface ")
                || trimmed.starts_with("type ")
                || trimmed.starts_with("async function ")
                || trimmed.starts_with("def ")
                || trimmed.starts_with("fn ")
                || trimmed.starts_with("const ") && trimmed.contains("=>")
                || trimmed.starts_with("async ") && trimmed.contains("(")
            {
                // Extract just the signature, not the body
                let sig = if let Some(brace) = trimmed.find('{') {
                    &trimmed[..brace]
                } else {
                    trimmed
                };
                lines.push(format!("  L{}: {}", line_num, sig.trim()));
            }
        }

        if lines.len() == 1 {
            lines.push("  (no functions/classes detected)".to_string());
        }

        Ok(lines.join("\n"))
    }

    /// Resolve a relative file path to an absolute path
    fn resolve_file_path(&self, file: &str) -> String {
        let cwd = std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if std::path::Path::new(file).is_absolute() {
            file.to_string()
        } else {
            format!("{}/{}", cwd, file)
        }
    }

    fn tool_reindex(&self, args: &Value) -> Result<String, String> {
        let repo_path = args
            .get("repo_path")
            .and_then(|v| v.as_str())
            .ok_or("repo_path required")?;

        if !std::path::Path::new(repo_path).is_dir() {
            return Err(format!("Not a directory: {}", repo_path));
        }

        self.do_reindex(repo_path)
    }

    fn tool_where_used(&self, args: &Value) -> Result<String, String> {
        let symbol = args
            .get("symbol")
            .and_then(|v| v.as_str())
            .ok_or("symbol required")?;
        let repo = args
            .get("repo")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        if !crate::call_index::CallIndex::exists(repo) {
            return Ok(format!("No index for '{}'. Run: savants reindex", repo));
        }

        let ci = crate::call_index::CallIndex::load(repo)?;
        let (callers, importers) = ci.find_where_used(symbol);

        let mut lines = vec![format!("=== Where '{}' is used ===", symbol)];

        if !callers.is_empty() {
            lines.push(format!("\nCallers ({}):", callers.len()));
            for c in &callers {
                lines.push(format!("  {} ({})", c.name, c.file));
            }
        }

        if !importers.is_empty() {
            lines.push(format!("\nImported by ({} files):", importers.len()));
            for f in &importers {
                lines.push(format!("  {}", f));
            }
        }

        if callers.is_empty() && importers.is_empty() {
            lines.push(format!("\nNo usages found for '{}' in the indexed codebase. It may be an entry point, exported API, or referenced dynamically.", symbol));
        }

        Ok(lines.join("\n"))
    }

    fn tool_callers(&self, args: &Value) -> Result<String, String> {
        let function = args
            .get("function")
            .and_then(|v| v.as_str())
            .ok_or("function required")?;
        let repo = args
            .get("repo")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        if !crate::call_index::CallIndex::exists(repo) {
            return Ok(format!("No index for '{}'. Run: savants reindex", repo));
        }

        let ci = crate::call_index::CallIndex::load(repo)?;
        let callers = ci.find_callers(function);

        if callers.is_empty() {
            return Ok(format!("No callers found for '{}' in the indexed codebase. This function may be an entry point, exported API, or called dynamically.", function));
        }

        let mut lines = vec![format!(
            "=== Callers of {} ({}) ===",
            function,
            callers.len()
        )];
        for c in &callers {
            lines.push(format!("  {} ({})", c.name, c.file));
        }
        Ok(lines.join("\n"))
    }

    fn tool_blast_radius(&self, args: &Value) -> Result<String, String> {
        let function = args
            .get("function")
            .and_then(|v| v.as_str())
            .ok_or("function required")?;
        let repo = args
            .get("repo")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let max_depth = args.get("depth").and_then(|v| v.as_i64()).unwrap_or(5) as usize;

        if !crate::call_index::CallIndex::exists(repo) {
            return Ok(format!("No index for '{}'. Auto-indexing should have run - try calling semantic_search first.", repo));
        }

        let ci = crate::call_index::CallIndex::load(repo)?;

        // BFS: find all transitive callers
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        let mut results: Vec<(String, String, usize)> = Vec::new(); // (name, file, depth)

        visited.insert(function.to_string());
        queue.push_back((function.to_string(), 0usize));

        while let Some((current, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            if let Some(callers) = ci.callers.get(&current) {
                for caller in callers {
                    if !visited.contains(&caller.name) {
                        visited.insert(caller.name.clone());
                        results.push((caller.name.clone(), caller.file.clone(), depth + 1));
                        queue.push_back((caller.name.clone(), depth + 1));
                    }
                }
            }
        }

        let affected_files: std::collections::HashSet<&str> =
            results.iter().map(|(_, f, _)| f.as_str()).collect();

        let mut lines = vec![format!(
            "=== Blast radius: {} ===\n{} functions affected across {} files (max depth {})",
            function,
            results.len(),
            affected_files.len(),
            max_depth
        )];

        if results.is_empty() {
            lines.push(format!(
                "\nNo callers found for '{}'. Safe to modify - nothing depends on it.",
                function
            ));
        } else {
            let risk = if results.len() > 20 {
                "HIGH"
            } else if results.len() > 5 {
                "MEDIUM"
            } else {
                "LOW"
            };
            lines.push(format!("Risk: {}\n", risk));

            // Group by depth
            for d in 1..=max_depth {
                let at_depth: Vec<&(String, String, usize)> =
                    results.iter().filter(|(_, _, depth)| *depth == d).collect();
                if !at_depth.is_empty() {
                    lines.push(format!("Depth {} ({} functions):", d, at_depth.len()));
                    for (name, file, _) in &at_depth {
                        lines.push(format!("  {}() in {}", name, file));
                    }
                }
            }
        }

        Ok(lines.join("\n"))
    }

    fn tool_dead_code(&self, args: &Value) -> Result<String, String> {
        let repo = args
            .get("repo")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let file_filter = args.get("file").and_then(|v| v.as_str());

        if !crate::call_index::CallIndex::exists(repo) {
            return Ok(format!("No index for '{}'. Auto-indexing should have run - try calling semantic_search first.", repo));
        }

        let ci = crate::call_index::CallIndex::load(repo)?;

        let mut dead: Vec<&crate::call_index::FuncRef> = Vec::new();

        for func in &ci.functions {
            // Skip if file filter doesn't match
            if let Some(filter) = file_filter {
                if !func.file.contains(filter) {
                    continue;
                }
            }

            // Skip common entry points / lifecycle functions
            let skip_names = [
                "main",
                "default",
                "setup",
                "teardown",
                "init",
                "constructor",
                "render",
                "mount",
                "unmount",
                "componentDidMount",
                "useEffect",
            ];
            if skip_names.contains(&func.name.as_str()) {
                continue;
            }

            // Check if anything calls this function
            let has_callers = ci
                .callers
                .get(&func.name)
                .map(|c| !c.is_empty())
                .unwrap_or(false);
            let has_importers = ci
                .importers
                .get(&func.name)
                .map(|i| !i.is_empty())
                .unwrap_or(false);

            if !has_callers && !has_importers {
                dead.push(func);
            }
        }

        let mut lines = vec![format!(
            "=== Dead code candidates{} ===\n{} functions with zero callers",
            file_filter
                .map(|f| format!(" in {}", f))
                .unwrap_or_default(),
            dead.len()
        )];

        if dead.is_empty() {
            lines.push(
                "\nNo dead code found. All functions have at least one caller or importer."
                    .to_string(),
            );
        } else {
            // Group by file
            let mut by_file: std::collections::HashMap<&str, Vec<&crate::call_index::FuncRef>> =
                std::collections::HashMap::new();
            for func in &dead {
                by_file.entry(&func.file).or_default().push(func);
            }

            let mut files: Vec<&&str> = by_file.keys().collect();
            files.sort();
            for file in files {
                let funcs = &by_file[file];
                lines.push(format!("\n{}:", file));
                for f in funcs {
                    lines.push(format!("  {}() (line {})", f.name, f.line));
                }
            }
            lines.push(
                "\nNote: exported functions may be used externally. Review before removing."
                    .to_string(),
            );
        }

        Ok(lines.join("\n"))
    }

    fn tool_git_blame(&self, args: &Value) -> Result<String, String> {
        let file = args
            .get("file")
            .and_then(|v| v.as_str())
            .ok_or("file required")?;
        let line_start = args
            .get("line_start")
            .and_then(|v| v.as_i64())
            .ok_or("line_start required")?;
        let line_end = args
            .get("line_end")
            .and_then(|v| v.as_i64())
            .unwrap_or(line_start);
        let repo_path = args
            .get("repo_path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            });

        let output = std::process::Command::new("git")
            .args([
                "-C",
                &repo_path,
                "blame",
                "--porcelain",
                &format!("-L{},{}", line_start, line_end),
                file,
            ])
            .output()
            .map_err(|e| format!("git blame failed: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git blame error: {}", stderr.trim()));
        }

        let raw = String::from_utf8_lossy(&output.stdout);
        let mut results: Vec<String> = Vec::new();
        let mut current_hash = String::new();
        let mut author = String::new();
        let mut date = String::new();
        let mut summary = String::new();
        let mut line_num = 0u64;

        for line in raw.lines() {
            if line.len() >= 40 && line.chars().take(40).all(|c| c.is_ascii_hexdigit()) {
                // Commit line: hash orig_line final_line
                let parts: Vec<&str> = line.split_whitespace().collect();
                current_hash = parts[0][..12].to_string();
                if parts.len() >= 3 {
                    line_num = parts[2].parse().unwrap_or(0);
                }
            } else if let Some(val) = line.strip_prefix("author ") {
                author = val.to_string();
            } else if let Some(val) = line.strip_prefix("author-time ") {
                if let Ok(ts) = val.parse::<i64>() {
                    let dt = chrono_format(ts);
                    date = dt;
                }
            } else if let Some(val) = line.strip_prefix("summary ") {
                summary = val.to_string();
            } else if line.starts_with('\t') {
                // Content line - emit the result
                let code = &line[1..];
                results.push(format!(
                    "L{}: {} | {} ({}) | {}",
                    line_num, current_hash, author, date, summary
                ));
                results.push(format!("    {}", code));
            }
        }

        if results.is_empty() {
            return Ok(format!(
                "No blame data for {}:{}-{}",
                file, line_start, line_end
            ));
        }

        let mut output_lines = vec![format!(
            "=== git blame: {}:{}-{} ===",
            file, line_start, line_end
        )];
        // Deduplicate consecutive same-commit lines
        let mut seen_commits: std::collections::HashSet<String> = std::collections::HashSet::new();
        for line in &results {
            if line.starts_with('L') {
                let commit = line.split(' ').nth(1).unwrap_or("").to_string();
                if !seen_commits.contains(&commit) {
                    seen_commits.insert(commit);
                    output_lines.push(line.clone());
                }
            } else {
                output_lines.push(line.clone());
            }
        }

        // Add commit details for deeper investigation
        if let Some(first_hash) = results.first().and_then(|l| l.split(' ').nth(1)) {
            let show = std::process::Command::new("git")
                .args(["-C", &repo_path, "show", "--stat", "--oneline", first_hash])
                .output();
            if let Ok(show_out) = show {
                if show_out.status.success() {
                    let show_text = String::from_utf8_lossy(&show_out.stdout);
                    output_lines.push(String::new());
                    output_lines.push("=== Commit details ===".to_string());
                    for l in show_text.lines().take(15) {
                        output_lines.push(format!("  {}", l));
                    }
                }
            }
        }

        Ok(output_lines.join("\n"))
    }

    fn tool_git_log(&self, args: &Value) -> Result<String, String> {
        let file = args.get("file").and_then(|v| v.as_str());
        let function_name = args.get("function_name").and_then(|v| v.as_str());
        let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(10);
        let repo_path = args
            .get("repo_path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            });

        let mut cmd_args = vec![
            "-C".to_string(),
            repo_path,
            "log".to_string(),
            format!("-{}", limit),
            "--format=%h %ad %an | %s".to_string(),
            "--date=short".to_string(),
        ];

        if let Some(fn_name) = function_name {
            if let Some(f) = file {
                // git log -L :funcname:file - shows function history
                cmd_args.push(format!("-L:{}:{}", fn_name, f));
            } else {
                // Search for commits mentioning the function
                cmd_args.push("-S".to_string());
                cmd_args.push(fn_name.to_string());
            }
        } else if let Some(f) = file {
            cmd_args.push("--".to_string());
            cmd_args.push(f.to_string());
        }

        let output = std::process::Command::new("git")
            .args(&cmd_args)
            .output()
            .map_err(|e| format!("git log failed: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git log error: {}", stderr.trim()));
        }

        let raw = String::from_utf8_lossy(&output.stdout);
        let log_lines: Vec<&str> = raw.lines().take(limit as usize * 3).collect();

        if log_lines.is_empty() {
            let target = file.or(function_name).unwrap_or("repo");
            return Ok(format!("No git history for '{}'", target));
        }

        let header = if let Some(fn_name) = function_name {
            format!("=== git log: {} ===", fn_name)
        } else if let Some(f) = file {
            format!("=== git log: {} ===", f)
        } else {
            "=== git log ===".to_string()
        };

        let mut output_lines = vec![header];
        for l in &log_lines {
            output_lines.push(format!("  {}", l));
        }
        Ok(output_lines.join("\n"))
    }

    fn response(&self, id: &Value, result: Value) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "result": result})
    }
}

fn chrono_format(ts: i64) -> String {
    // Simple unix timestamp to YYYY-MM-DD
    let days = ts / 86400;
    let y = 1970 + (days * 4 + 2) / 1461;
    let doy = days - (365 * (y - 1970) + (y - 1969) / 4);
    let months = [
        31,
        28 + if y % 4 == 0 { 1 } else { 0 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut m = 0;
    let mut d = doy;
    for days_in_month in &months {
        if d < *days_in_month {
            break;
        }
        d -= days_in_month;
        m += 1;
    }
    format!("{}-{:02}-{:02}", y, m + 1, d + 1)
}
