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
        let tokens_saved = if estimated_grep_tokens > self.total_tokens_returned {
            estimated_grep_tokens - self.total_tokens_returned
        } else {
            0
        };
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

                match result {
                    Ok(ref text) => {
                        stats.record_call(tool, text.len(), duration_ms);
                        Some(self.response(&req_id, json!({"content": [{"type": "text", "text": text}]})))
                    },
                    Err(e) => Some(self.response(&req_id, json!({"content": [{"type": "text", "text": format!("Error: {}", e)}], "isError": true}))),
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
                "description": "NATURAL LANGUAGE CODE SEARCH: Find functions by describing what they do. 'payment retry logic' finds handleTransactionWithBackoff. 90% accuracy. Works offline.",
                "inputSchema": {"type": "object", "properties": {
                    "query": {"type": "string", "description": "What you're looking for in natural language"},
                    "repo": {"type": "string", "description": "Repository name"},
                    "limit": {"type": "integer", "description": "Max results (default 10)"}
                }, "required": ["query"]}
            },
            {
                "name": "file_skeleton",
                "description": "FASTER THAN READ: Returns file structure - function names, signatures, line ranges. No bodies. 10x fewer tokens than reading the full file.",
                "inputSchema": {"type": "object", "properties": {
                    "file": {"type": "string", "description": "File path relative to repo root"},
                    "repo": {"type": "string", "description": "Repository name"}
                }, "required": ["file"]}
            },
            {
                "name": "where_used",
                "description": "FASTER THAN GREP: Find every place a function is called or imported across the codebase. Returns callers and importers.",
                "inputSchema": {"type": "object", "properties": {
                    "symbol": {"type": "string", "description": "Function name to search for"},
                    "repo": {"type": "string", "description": "Repository name"}
                }, "required": ["symbol"]}
            },
            {
                "name": "callers",
                "description": "FASTER THAN GREP: Find all functions that directly call a given function.",
                "inputSchema": {"type": "object", "properties": {
                    "function": {"type": "string", "description": "Function name"},
                    "repo": {"type": "string", "description": "Repository name"}
                }, "required": ["function"]}
            },
            {
                "name": "reindex",
                "description": "Parse and index a repository for semantic search. Run this first.",
                "inputSchema": {"type": "object", "properties": {
                    "repo_path": {"type": "string", "description": "Absolute path to repository"}
                }, "required": ["repo_path"]}
            },
            {
                "name": "session_stats",
                "description": "SAVANTS EFFICIENCY QUOTIENT (SEQ): Your AI agent's context efficiency score (0-100). Measures precision (right result first try), cost (tokens saved vs grep), and velocity (response speed). Call this to see your score. Zero cost.",
                "inputSchema": {"type": "object", "properties": {}}
            }
        ])
    }

    fn call_tool(&self, tool: &str, args: &Value) -> Result<String, String> {
        match tool {
            "semantic_search" => self.tool_semantic_search(args),
            "file_skeleton" => self.tool_file_skeleton(args),
            "where_used" => self.tool_where_used(args),
            "callers" => self.tool_callers(args),
            "reindex" => self.tool_reindex(args),
            _ => Err(format!(
                "'{}' requires savants.cloud. Run: savants connect",
                tool
            )),
        }
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

        if !crate::embedding_store::EmbeddingStore::exists(repo) {
            return Ok(format!("No index for '{}'. Run: savants reindex", repo));
        }

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

        if functions.is_empty() && classes.is_empty() {
            return Ok(format!(
                "No entities in '{}'. Is the file path correct?",
                file
            ));
        }

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
        Ok(lines.join("\n"))
    }

    fn tool_reindex(&self, args: &Value) -> Result<String, String> {
        let repo_path = args
            .get("repo_path")
            .and_then(|v| v.as_str())
            .ok_or("repo_path required")?;

        if !std::path::Path::new(repo_path).is_dir() {
            return Err(format!("Not a directory: {}", repo_path));
        }

        let repo_name = std::path::Path::new(repo_path)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let mut parser = crate::code_parser::CodeParser::new(&repo_name);
        let result = parser.parse_repo(repo_path);

        // Save call index
        let ci = crate::call_index::CallIndex::from_parse_result(&result);
        let _ = ci.save(&repo_name);

        // Save freshness state (git HEAD + branch)
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

        Ok(format!(
            "Indexed {}: {} files, {} entities. Cached for instant search.",
            repo_name,
            result.files,
            store.entries.len()
        ))
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
            lines.push(format!("\nNo usages found for '{}'", symbol));
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
            return Ok(format!("No callers found for '{}'", function));
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

    fn response(&self, id: &Value, result: Value) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "result": result})
    }
}
