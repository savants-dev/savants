use crate::config::State;
use colored::*;
use std::path::Path;

const API_BASE: &str = "https://api.savants.cloud/api/v1/docs";

pub async fn list() {
    let client = reqwest::Client::new();
    let resp = match client.get(API_BASE).send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}: could not reach savants.cloud: {}", "Error".red(), e);
            return;
        }
    };

    if !resp.status().is_success() {
        eprintln!(
            "{}: server returned status {}",
            "Error".red(),
            resp.status()
        );
        return;
    }

    let body: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}: invalid response: {}", "Error".red(), e);
            return;
        }
    };

    let sources = match body.get("providers").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => {
            eprintln!("{}: unexpected response format", "Error".red());
            return;
        }
    };

    println!("{}", "Available documentation sources:".bold());
    for source in sources {
        let name = source.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
        let description = source
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let status = source
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("planned");
        let versions = source
            .get("versions")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        if status != "planned" {
            println!(
                "  {} {:<14}{}({} versions)",
                "●".green(),
                name.cyan(),
                if description.is_empty() {
                    String::new()
                } else {
                    format!("{} ", description)
                },
                versions
            );
        } else {
            println!(
                "  {} {:<14}{}",
                "○".white(),
                name,
                if description.is_empty() {
                    "(not indexed yet)".to_string()
                } else {
                    format!("{} (not indexed yet)", description)
                }
            );
        }
    }
    println!();
    println!("Usage: {} <provider> <query>", "savants docs search".cyan());
}

pub async fn search(provider: &str, query: &str) {
    println!(
        "Searching {} for \"{}\"...",
        provider.cyan(),
        query.yellow()
    );
    println!();

    let client = reqwest::Client::new();
    let url = format!("{}/{}/search", API_BASE, provider);
    let resp = match client.get(&url).query(&[("q", query)]).send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}: could not reach savants.cloud: {}", "Error".red(), e);
            return;
        }
    };

    if !resp.status().is_success() {
        eprintln!(
            "{}: server returned status {}",
            "Error".red(),
            resp.status()
        );
        return;
    }

    let body: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}: invalid response: {}", "Error".red(), e);
            return;
        }
    };

    let results = match body.get("results").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => {
            println!("No results found.");
            return;
        }
    };

    if results.is_empty() {
        println!("No results found.");
        return;
    }

    for (i, result) in results.iter().enumerate() {
        let title = result
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled");
        let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let snippet = result
            .get("snippet")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        println!("{}. {}", (i + 1).to_string().bold(), title.bold());
        if !url.is_empty() {
            println!("   {}", url.cyan().underline());
        }
        if !snippet.is_empty() {
            println!("   \"{}\"", snippet);
        }
        println!();
    }
}

pub async fn upload(path: &str, project: &str) {
    let state = State::load();
    let token = match &state.cloud_token {
        Some(t) => t.clone(),
        None => {
            eprintln!(
                "{}: not connected to savants.cloud. Run {} first.",
                "Error".red(),
                "savants connect".cyan()
            );
            return;
        }
    };

    let dir = Path::new(path);
    if !dir.exists() || !dir.is_dir() {
        eprintln!("{}: {} is not a valid directory", "Error".red(), path);
        return;
    }

    // Collect markdown files
    let mut files: Vec<(String, String)> = Vec::new();
    collect_markdown_files(dir, &mut files);

    if files.is_empty() {
        eprintln!("{}: no markdown files found in {}", "Error".red(), path);
        return;
    }

    println!(
        "Uploading {} markdown files from {} as project \"{}\"...",
        files.len().to_string().bold(),
        path.cyan(),
        project.yellow()
    );

    let mut docs: Vec<serde_json::Value> = Vec::new();
    for (rel_path, content) in &files {
        docs.push(serde_json::json!({
            "path": rel_path,
            "content": content,
        }));
    }

    let payload = serde_json::json!({
        "project": project,
        "documents": docs,
    });

    let client = reqwest::Client::new();
    let resp = match client
        .post(&format!("{}/upload", API_BASE))
        .header("Authorization", format!("Bearer {}", token))
        .json(&payload)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}: could not reach savants.cloud: {}", "Error".red(), e);
            return;
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        eprintln!(
            "{}: upload failed (status {}): {}",
            "Error".red(),
            status,
            body
        );
        return;
    }

    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    let sections = body
        .get("sections_indexed")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let credits = body
        .get("credits_used")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    println!();
    println!(
        "  {} Uploaded {} sections",
        "●".green(),
        sections.to_string().bold()
    );
    if credits > 0.0 {
        println!("  Credits used: {:.2}", credits);
    }
    println!(
        "  Search with: {} {} <query>",
        "savants docs search".cyan(),
        project
    );
}

fn collect_markdown_files(dir: &Path, files: &mut Vec<(String, String)>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_markdown_files(&path, files);
        } else if let Some(ext) = path.extension() {
            if ext == "md" || ext == "mdx" {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let rel = path.to_string_lossy().to_string();
                    files.push((rel, content));
                }
            }
        }
    }
}
