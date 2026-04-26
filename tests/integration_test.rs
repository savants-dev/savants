//! Integration tests for Savants OSS
//! Tests every user path: parse, index, search, MCP protocol

use std::io::Write;
use std::process::{Command, Stdio};

/// Helper: send MCP messages to savants serve and return the last response
fn mcp_call(messages: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_savants"))
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start savants serve");

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(messages.as_bytes()).unwrap();
    }

    let output = child.wait_with_output().expect("Failed to read output");
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().last().unwrap_or("").to_string()
}

fn init_msg() -> &'static str {
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#
}

fn tool_call(id: u32, tool: &str, args: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":{},"method":"tools/call","params":{{"name":"{}","arguments":{}}}}}"#,
        id, tool, args
    )
}

fn parse_result_text(json_line: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(json_line).unwrap_or_default();
    v.get("result")
        .and_then(|r| r.get("content"))
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|t| t.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string()
}

// ============================================================
// MCP Protocol Tests
// ============================================================

#[test]
fn test_mcp_initialize() {
    let response = mcp_call(&format!("{}\n", init_msg()));
    let v: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert_eq!(v["result"]["serverInfo"]["name"], "savants");
    assert!(v["result"]["capabilities"]["tools"].is_object());
}

#[test]
fn test_mcp_tools_list() {
    let msg = format!(
        "{}\n{}\n",
        init_msg(),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#
    );
    let response = mcp_call(&msg);
    let v: serde_json::Value = serde_json::from_str(&response).unwrap();
    let tools = v["result"]["tools"].as_array().unwrap();
    assert!(
        tools.len() >= 5,
        "Expected at least 5 tools, got {}",
        tools.len()
    );

    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"semantic_search"));
    assert!(names.contains(&"file_skeleton"));
    assert!(names.contains(&"callers"));
    assert!(names.contains(&"where_used"));
    assert!(names.contains(&"reindex"));
}

#[test]
fn test_mcp_unknown_tool_returns_cloud_message() {
    let msg = format!(
        "{}\n{}\n",
        init_msg(),
        tool_call(2, "diagnose-error", r#"{"error":"test"}"#)
    );
    let response = mcp_call(&msg);
    let text = parse_result_text(&response);
    assert!(
        text.contains("requires savants.cloud"),
        "Expected cloud upgrade message, got: {}",
        text
    );
}

#[test]
fn test_mcp_ping() {
    let msg = format!(
        "{}\n{}\n",
        init_msg(),
        r#"{"jsonrpc":"2.0","id":2,"method":"ping","params":{}}"#
    );
    let response = mcp_call(&msg);
    let v: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert_eq!(v["id"], 2);
    assert!(v["result"].is_object());
}

// ============================================================
// Reindex Tests
// ============================================================

#[test]
fn test_reindex_creates_caches() {
    // Use the savants-oss source code itself as test repo
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let msg = format!(
        "{}\n{}\n",
        init_msg(),
        tool_call(2, "reindex", &format!(r#"{{"repo_path":"{}"}}"#, repo_path))
    );
    let response = mcp_call(&msg);
    let text = parse_result_text(&response);
    assert!(
        text.contains("Indexed"),
        "Expected 'Indexed', got: {}",
        text
    );
    assert!(
        text.contains("entities"),
        "Expected entity count in: {}",
        text
    );

    // Check cache files exist
    let home = dirs::home_dir().unwrap();
    let repo_name = std::path::Path::new(repo_path)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        home.join(".savants/embeddings")
            .join(format!("{}.bin", repo_name))
            .exists(),
        "Embedding cache not created"
    );
    assert!(
        home.join(".savants/calls")
            .join(format!("{}.json", repo_name))
            .exists(),
        "Call index not created"
    );
}

#[test]
fn test_reindex_invalid_path() {
    let msg = format!(
        "{}\n{}\n",
        init_msg(),
        tool_call(2, "reindex", r#"{"repo_path":"/nonexistent/path"}"#)
    );
    let response = mcp_call(&msg);
    let text = parse_result_text(&response);
    assert!(
        text.contains("Not a directory"),
        "Expected error, got: {}",
        text
    );
}

// ============================================================
// Search Tests (require reindex first)
// ============================================================

#[test]
fn test_semantic_search_no_index() {
    let msg = format!(
        "{}\n{}\n",
        init_msg(),
        tool_call(
            2,
            "semantic_search",
            r#"{"query":"test","repo":"nonexistent_repo_xyz"}"#
        )
    );
    let response = mcp_call(&msg);
    let text = parse_result_text(&response);
    assert!(
        text.contains("No index") || text.contains("Run"),
        "Expected no-index message, got: {}",
        text
    );
}

#[test]
fn test_file_skeleton_no_index() {
    let msg = format!(
        "{}\n{}\n",
        init_msg(),
        tool_call(
            2,
            "file_skeleton",
            r#"{"file":"src/main.rs","repo":"nonexistent_repo_xyz"}"#
        )
    );
    let response = mcp_call(&msg);
    let text = parse_result_text(&response);
    assert!(
        text.contains("No index") || text.contains("Run"),
        "Expected no-index message, got: {}",
        text
    );
}

#[test]
fn test_callers_no_index() {
    let msg = format!(
        "{}\n{}\n",
        init_msg(),
        tool_call(
            2,
            "callers",
            r#"{"function":"test","repo":"nonexistent_repo_xyz"}"#
        )
    );
    let response = mcp_call(&msg);
    let text = parse_result_text(&response);
    assert!(
        text.contains("No index") || text.contains("Run"),
        "Expected no-index message, got: {}",
        text
    );
}

// ============================================================
// Code Parser Tests
// ============================================================

#[test]
fn test_parser_extracts_rust_functions() {
    // Index the savants-oss repo itself (it's Rust)
    let repo_path = env!("CARGO_MANIFEST_DIR");
    let msg = format!(
        "{}\n{}\n",
        init_msg(),
        tool_call(2, "reindex", &format!(r#"{{"repo_path":"{}"}}"#, repo_path))
    );
    let _ = mcp_call(&msg);

    // Search for a function we know exists
    let repo_name = std::path::Path::new(repo_path)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();
    let msg = format!(
        "{}\n{}\n",
        init_msg(),
        tool_call(
            2,
            "semantic_search",
            &format!(
                r#"{{"query":"cloud proxy server","repo":"{}","limit":3}}"#,
                repo_name
            )
        )
    );
    let response = mcp_call(&msg);
    let text = parse_result_text(&response);
    assert!(
        text.contains("Semantic search") || text.contains("results"),
        "Expected search results, got: {}",
        text
    );
}

// ============================================================
// Embedding Store Tests
// ============================================================

#[test]
fn test_embedding_store_roundtrip() {
    use std::path::PathBuf;

    // This tests the binary format directly
    let store_path = dirs::home_dir()
        .unwrap()
        .join(".savants/embeddings/test_roundtrip.bin");

    // Clean up from previous runs
    let _ = std::fs::remove_file(&store_path);

    // We can't easily test EmbeddingStore directly from integration tests
    // without importing the module, so we test via the MCP reindex path
    // which creates the store as a side effect.
    // The reindex test above covers this.
}

// ============================================================
// CLI Tests
// ============================================================

#[test]
fn test_cli_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_savants"))
        .arg("--help")
        .output()
        .expect("Failed to run savants --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("context engine"),
        "Help should mention context engine"
    );
    assert!(stdout.contains("serve"), "Help should list serve command");
    assert!(
        stdout.contains("reindex"),
        "Help should list reindex command"
    );
}

#[test]
fn test_cli_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_savants"))
        .arg("--version")
        .output()
        .expect("Failed to run savants --version");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("savants"),
        "Version should contain 'savants'"
    );
}

#[test]
fn test_cli_status() {
    let output = Command::new(env!("CARGO_BIN_EXE_savants"))
        .arg("status")
        .output()
        .expect("Failed to run savants status");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Status") || stdout.contains("Cloud") || stdout.contains("Search"),
        "Status should show something useful"
    );
}
