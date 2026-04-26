//! Offline MCP server: serves local-only tools (semantic search, file skeleton)
//! without any cloud or database connection.

use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

pub struct OfflineServer;

impl OfflineServer {
    pub fn new() -> Self { Self }

    pub fn run(&self) {
        eprintln!("Savants MCP server started (offline mode)");
        eprintln!("Connect to savants.cloud for full intelligence tools: savants connect");
        let stdin = io::stdin();
        let stdout = io::stdout();
        let reader = stdin.lock();
        let mut writer = stdout.lock();

        for line in reader.lines() {
            let line = match line { Ok(l) => l, Err(_) => break };
            let trimmed = line.trim();
            if trimmed.is_empty() { continue; }
            let message: Value = match serde_json::from_str(trimmed) {
                Ok(v) => v, Err(_) => continue,
            };
            if let Some(response) = self.handle_message(&message) {
                let body = serde_json::to_string(&response).unwrap();
                let _ = writeln!(writer, "{}", body);
                let _ = writer.flush();
            }
        }
    }

    fn handle_message(&self, message: &Value) -> Option<Value> {
        let method = message.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let params = message.get("params").cloned().unwrap_or(json!({}));
        let req_id = message.get("id");
        if req_id.is_none() || req_id == Some(&Value::Null) { return None; }
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
                let result = self.call_tool(tool, &args);
                match result {
                    Ok(text) => Some(self.response(&req_id, json!({"content": [{"type": "text", "text": text}]}))),
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
            _ => Err(format!("'{}' requires savants.cloud. Run: savants connect", tool)),
        }
    }

    fn tool_semantic_search(&self, args: &Value) -> Result<String, String> {
        let query = args.get("query").and_then(|v| v.as_str()).ok_or("query required")?;
        let repo = args.get("repo").and_then(|v| v.as_str()).unwrap_or("unknown");
        let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(10) as usize;

        if !crate::embedding_store::EmbeddingStore::exists(repo) {
            return Ok(format!("No index for '{}'. Run: savants reindex --repo-path /path/to/{}", repo, repo));
        }

        // Check if index is stale - if so, auto-reindex in the background
        let cwd = std::env::current_dir().unwrap_or_default().to_string_lossy().to_string();
        if let Some(_warning) = crate::freshness::check_freshness(repo, &cwd) {
            // Auto-reindex: the user shouldn't have to think about this
            eprintln!("[savants] Index stale, re-indexing {}...", repo);
            let mut parser = crate::code_parser::CodeParser::new(repo);
            let result = parser.parse_repo(&cwd);

            let ci = crate::call_index::CallIndex::from_parse_result(&result);
            let _ = ci.save(repo);

            if let Some(head) = crate::freshness::get_git_head(&cwd) {
                let branch = crate::freshness::get_git_branch(&cwd).unwrap_or_else(|| "unknown".to_string());
                crate::freshness::save_state(repo, &head, &branch);
            }

            if let Ok(mut engine) = crate::embeddings::EmbeddingEngine::new() {
                if let Ok(index) = crate::semantic_search::SemanticIndex::from_parse_result(&result, &mut engine) {
                    let dim = engine.embed_one("test").map(|v| v.len() as u32).unwrap_or(128);
                    let mut store = crate::embedding_store::EmbeddingStore::new(dim);
                    for (entry, emb) in index.entries_with_embeddings() {
                        let kind = match entry.kind.as_str() { "class" => 1, "interface" => 2, _ => 0 };
                        store.add(&entry.name, &entry.file, entry.line as u32, kind, emb.clone());
                    }
                    let _ = store.save(repo);
                }
            }
            eprintln!("[savants] Re-indexed {} ({} entities)", repo, result.entities.len());
            // Fall through to search with the fresh index
        }

        let store = crate::embedding_store::EmbeddingStore::load(repo)?;
        let mut engine = crate::embeddings::EmbeddingEngine::new()?;
        let query_vec = engine.embed_one(query)?;
        let results = store.search(&query_vec, limit);

        if results.is_empty() {
            return Ok(format!("No results for '{}' in {}", query, repo));
        }

        let mut lines = vec![format!("=== Semantic search: '{}' ({} results) ===", query, results.len())];
        for (idx, score) in &results {
            let entry = &store.entries[*idx];
            lines.push(format!("  {}:{} {}() [{:.3}]", entry.file, entry.line, entry.name, score));
        }
        Ok(lines.join("\n"))
    }

    fn tool_file_skeleton(&self, args: &Value) -> Result<String, String> {
        let file = args.get("file").and_then(|v| v.as_str()).ok_or("file required")?;
        let repo = args.get("repo").and_then(|v| v.as_str()).unwrap_or("unknown");

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
            return Ok(format!("No entities in '{}'. Is the file path correct?", file));
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
        let repo_path = args.get("repo_path").and_then(|v| v.as_str()).ok_or("repo_path required")?;

        if !std::path::Path::new(repo_path).is_dir() {
            return Err(format!("Not a directory: {}", repo_path));
        }

        let repo_name = std::path::Path::new(repo_path)
            .file_name().map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let mut parser = crate::code_parser::CodeParser::new(&repo_name);
        let result = parser.parse_repo(repo_path);

        // Save call index
        let ci = crate::call_index::CallIndex::from_parse_result(&result);
        let _ = ci.save(&repo_name);

        // Save freshness state (git HEAD + branch)
        if let Some(head) = crate::freshness::get_git_head(repo_path) {
            let branch = crate::freshness::get_git_branch(repo_path).unwrap_or_else(|| "unknown".to_string());
            crate::freshness::save_state(&repo_name, &head, &branch);
        }

        let mut engine = crate::embeddings::EmbeddingEngine::new()?;
        let index = crate::semantic_search::SemanticIndex::from_parse_result(&result, &mut engine)?;

        let dim = engine.embed_one("test").map(|v| v.len() as u32).unwrap_or(128);
        let mut store = crate::embedding_store::EmbeddingStore::new(dim);
        for (entry, emb) in index.entries_with_embeddings() {
            let kind = match entry.kind.as_str() { "class" => 1, "interface" => 2, _ => 0 };
            store.add(&entry.name, &entry.file, entry.line as u32, kind, emb.clone());
        }
        store.save(&repo_name)?;

        Ok(format!("Indexed {}: {} files, {} entities. Cached for instant search.", repo_name, result.files, store.entries.len()))
    }

    fn tool_where_used(&self, args: &Value) -> Result<String, String> {
        let symbol = args.get("symbol").and_then(|v| v.as_str()).ok_or("symbol required")?;
        let repo = args.get("repo").and_then(|v| v.as_str()).unwrap_or("unknown");

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
        let function = args.get("function").and_then(|v| v.as_str()).ok_or("function required")?;
        let repo = args.get("repo").and_then(|v| v.as_str()).unwrap_or("unknown");

        if !crate::call_index::CallIndex::exists(repo) {
            return Ok(format!("No index for '{}'. Run: savants reindex", repo));
        }

        let ci = crate::call_index::CallIndex::load(repo)?;
        let callers = ci.find_callers(function);

        if callers.is_empty() {
            return Ok(format!("No callers found for '{}'", function));
        }

        let mut lines = vec![format!("=== Callers of {} ({}) ===", function, callers.len())];
        for c in &callers {
            lines.push(format!("  {} ({})", c.name, c.file));
        }
        Ok(lines.join("\n"))
    }

    fn response(&self, id: &Value, result: Value) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "result": result})
    }
}
