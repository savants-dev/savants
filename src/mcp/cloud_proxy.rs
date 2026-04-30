//! Cloud proxy MCP server: forwards all tool calls to api.savants.cloud
//!
//! When SAVANTS_CLOUD_URL is set, `savants serve` uses this instead of

use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

pub struct CloudProxyServer {
    cloud_url: String,
    api_key: String,
}

impl CloudProxyServer {
    pub fn new(cloud_url: &str, api_key: &str) -> Self {
        Self {
            cloud_url: cloud_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
        }
    }

    pub fn run(&self) {
        eprintln!(
            "Savants MCP server started (cloud proxy -> {})",
            self.cloud_url
        );
        let stdin = io::stdin();
        let stdout = io::stdout();
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

        if req_id.is_none() || req_id == Some(&Value::Null) {
            return None;
        }
        let req_id = req_id.unwrap().clone();

        match method {
            "initialize" => Some(self.response(
                &req_id,
                json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {
                        "tools": {"listChanged": false},
                        "resources": {},
                        "prompts": {}
                    },
                    "serverInfo": {"name": "savants", "version": "0.1.0-cloud"}
                }),
            )),

            "ping" => Some(self.response(&req_id, json!({}))),

            "tools/list" => {
                // Fetch tool list from cloud API
                match self.cloud_get("/api/v1/tools") {
                    Ok(tools_response) => {
                        let tools = tools_response.get("tools").cloned().unwrap_or(json!([]));
                        // Convert cloud format to MCP format
                        let mcp_tools: Vec<Value> = tools.as_array()
                            .unwrap_or(&vec![])
                            .iter()
                            .map(|t| {
                                json!({
                                    "name": t.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                                    "description": t.get("description").and_then(|v| v.as_str()).unwrap_or(""),
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {}
                                    }
                                })
                            })
                            .collect();
                        Some(self.response(&req_id, json!({"tools": mcp_tools})))
                    }
                    Err(e) => Some(self.error(&req_id, -32000, &format!("Cloud error: {}", e))),
                }
            }

            "tools/call" => {
                let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

                // LOCAL tools run on user's machine - never proxy to cloud
                let local_tools = [
                    "semantic_search",
                    "file_skeleton",
                    "where_used",
                    "callers",
                    "session_stats",
                ];
                if local_tools.contains(&tool_name) {
                    let offline = super::offline::OfflineServer::new();
                    let result = offline.call_tool_direct(tool_name, &arguments);
                    return match result {
                        Ok(text) => Some(self.response(&req_id, json!({"content": [{"type": "text", "text": text}]}))),
                        Err(e) => Some(self.response(&req_id, json!({"content": [{"type": "text", "text": format!("Error: {}", e)}], "isError": true}))),
                    };
                }

                // Reindex runs LOCALLY (parses code on user's machine),
                // then uploads parsed entities to cloud for indexing.
                if tool_name == "reindex" {
                    let repo_path = arguments
                        .get("repo_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or(".");
                    let repo_name = std::path::Path::new(repo_path)
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| "unknown".to_string());

                    if !std::path::Path::new(repo_path).is_dir() {
                        return Some(self.response(&req_id, json!({
                            "content": [{"type": "text", "text": format!("Not a directory: {}", repo_path)}],
                            "isError": true
                        })));
                    }

                    // Parse locally
                    let mut parser = crate::code_parser::CodeParser::new(&repo_name);
                    let result = parser.parse_repo(repo_path);
                    let entity_count = result.entities.len();
                    let file_count = result.files;

                    // Save local caches (embeddings + call index)
                    let ci = crate::call_index::CallIndex::from_parse_result(&result);
                    let _ = ci.save(&repo_name);

                    if let Ok(mut engine) = crate::embeddings::EmbeddingEngine::new() {
                        if let Ok(index) = crate::semantic_search::SemanticIndex::from_parse_result(
                            &result,
                            &mut engine,
                        ) {
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
                            let _ = store.save(&repo_name);
                        }
                    }

                    // Cloud upload mode:
                    //   Default (metadata-only): names, files, lines, params, call sites, flags.
                    //     No source code leaves your machine.
                    //   SAVANTS_INDEX_MODE=source-context: includes function body snippets.
                    //     Enables full-text search and validation detection on cloud.
                    //     Your source code is sent to savants.cloud (encrypted in transit).
                    let index_mode = std::env::var("SAVANTS_INDEX_MODE").unwrap_or_default();
                    let full_index = index_mode == "source-context";

                    let upload_data = if full_index {
                        // Full mode: send everything including bodies (user opted in)
                        result.clone()
                    } else {
                        // Safe mode (default): strip bodies, send only structural metadata
                        let mut safe = result.clone();
                        for entity in &mut safe.entities {
                            let has_validation = entity.body.contains("validate")
                                || entity.body.contains("check")
                                || entity.body.contains("guard")
                                || entity.body.contains("try");
                            let has_error_handling = entity.body.contains("catch")
                                || entity.body.contains("throw")
                                || entity.body.contains("Error");
                            // Replace body with analysis flags only
                            let mut flags = vec![];
                            if has_validation {
                                flags.push("validation");
                            }
                            if has_error_handling {
                                flags.push("error_handling");
                            }
                            entity.body = if flags.is_empty() {
                                String::new()
                            } else {
                                format!("[{}]", flags.join(","))
                            };
                        }
                        safe
                    };

                    let mode = if full_index {
                        "source-context (includes code snippets)"
                    } else {
                        "metadata-only (no source code sent)"
                    };
                    let body_str = serde_json::to_string(&upload_data).unwrap_or_default();
                    let upload_result = std::process::Command::new("curl")
                        .args([
                            "-sf",
                            "--max-time",
                            "60",
                            "-X",
                            "POST",
                            "-H",
                            &format!("Authorization: Bearer {}", self.api_key),
                            "-H",
                            "Content-Type: application/json",
                            "-d",
                            &body_str,
                            &format!("{}/api/v1/ingest", self.cloud_url),
                        ])
                        .output();

                    let cloud_status = match upload_result {
                        Ok(o) if o.status.success() => format!("uploaded to cloud ({})", mode),
                        _ => "cloud upload failed (local index still works)".to_string(),
                    };

                    return Some(self.response(
                        &req_id,
                        json!({
                            "content": [{"type": "text", "text": format!(
                                "Indexed {}: {} files, {} entities. Local cache saved. {}",
                                repo_name, file_count, entity_count, cloud_status
                            )}]
                        }),
                    ));
                }

                // All other tools: forward to cloud API
                let body = json!({
                    "tool": tool_name,
                    "input": arguments,
                });

                match self.cloud_post("/api/v1/tools/call", &body) {
                    Ok(cloud_response) => {
                        let result_text = cloud_response.get("result")
                            .and_then(|v| v.as_str())
                            .unwrap_or_else(|| {
                                cloud_response.get("result")
                                    .map(|v| v.to_string())
                                    .unwrap_or_default()
                                    .leak()
                            });

                        Some(self.response(&req_id, json!({
                            "content": [{
                                "type": "text",
                                "text": result_text
                            }]
                        })))
                    }
                    Err(e) if e.contains("402") || e.contains("Payment") => {
                        Some(self.response(&req_id, json!({
                            "content": [{
                                "type": "text",
                                "text": "Free tier limit reached (10 calls/month).\n\nUpgrade to pay-as-you-go: https://savants.cloud/billing\nOr run: savants usage"
                            }],
                            "isError": true
                        })))
                    }
                    Err(e) => Some(self.error(&req_id, -32000, &format!("Cloud error: {}", e))),
                }
            }

            "resources/list" => Some(self.response(&req_id, json!({"resources": []}))),
            "prompts/list" => Some(self.response(&req_id, json!({"prompts": []}))),
            _ => Some(self.error(&req_id, -32601, &format!("Unknown method: {}", method))),
        }
    }

    fn cloud_get(&self, path: &str) -> Result<Value, String> {
        let url = format!("{}{}", self.cloud_url, path);
        let output = std::process::Command::new("curl")
            .args([
                "-sf",
                "--max-time",
                "60",
                "-H",
                &format!("Authorization: Bearer {}", self.api_key),
                &url,
            ])
            .output()
            .map_err(|e| format!("curl failed: {}", e))?;
        if !output.status.success() {
            return Err(format!("HTTP error from {}", url));
        }
        serde_json::from_slice(&output.stdout).map_err(|e| format!("parse failed: {}", e))
    }

    fn cloud_post(&self, path: &str, body: &Value) -> Result<Value, String> {
        let url = format!("{}{}", self.cloud_url, path);
        let body_str = serde_json::to_string(body).unwrap();
        let output = std::process::Command::new("curl")
            .args([
                "-sf",
                "--max-time",
                "60",
                "-X",
                "POST",
                "-H",
                &format!("Authorization: Bearer {}", self.api_key),
                "-H",
                "Content-Type: application/json",
                "-d",
                &body_str,
                &url,
            ])
            .output()
            .map_err(|e| format!("curl failed: {}", e))?;
        if !output.status.success() {
            return Err(format!("HTTP error from {}", url));
        }
        serde_json::from_slice(&output.stdout).map_err(|e| format!("parse failed: {}", e))
    }

    fn response(&self, id: &Value, result: Value) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "result": result})
    }

    fn error(&self, id: &Value, code: i32, message: &str) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
    }
}
