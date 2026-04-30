use crate::config::State;
use colored::*;

const CLOUD_ENDPOINT: &str = "https://api.savants.cloud";

pub async fn run() {
    println!("{}", "Connecting to savants.cloud...".bold());
    println!();

    let state = State::load();
    if state.is_cloud_authenticated() {
        println!("  {} Already connected", "●".green());
        println!("  Run {} to see usage.", "savants usage".cyan());
        return;
    }

    let client = reqwest::Client::new();
    let code_response = match client
        .post(&format!("{}/auth/device/code", CLOUD_ENDPOINT))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            resp.json::<serde_json::Value>().await.unwrap_or_default()
        }
        Ok(resp) => {
            eprintln!("{}: cloud returned status {}", "Error".red(), resp.status());
            return;
        }
        Err(e) => {
            eprintln!("{}: could not reach savants.cloud: {}", "Error".red(), e);
            return;
        }
    };

    let device_code = code_response
        .get("device_code")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let user_code = code_response
        .get("user_code")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let default_uri = format!("{}/activate", CLOUD_ENDPOINT);
    let verification_uri = code_response
        .get("verification_uri")
        .and_then(|v| v.as_str())
        .unwrap_or(&default_uri);
    let interval = code_response
        .get("interval")
        .and_then(|v| v.as_u64())
        .unwrap_or(5);

    println!("To authenticate, visit:");
    println!();
    println!("    {}", verification_uri.cyan().bold().underline());
    println!();
    println!("And enter code: {}", user_code.yellow().bold());
    println!();
    println!("Waiting for authentication...");

    for _ in 0..180 {
        tokio::time::sleep(std::time::Duration::from_secs(interval)).await;

        let poll_response = match client
            .post(&format!("{}/auth/device/token", CLOUD_ENDPOINT))
            .json(&serde_json::json!({"device_code": device_code}))
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(_) => continue,
        };

        let status = poll_response.status();
        let body = poll_response
            .json::<serde_json::Value>()
            .await
            .unwrap_or_default();

        if status.is_success() {
            let access_token = body
                .get("access_token")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let org_id = body.get("org_id").and_then(|v| v.as_str()).unwrap_or("");

            let mut state = State::load();
            state.cloud_token = Some(access_token.to_string());
            state.cloud_org = Some(org_id.to_string());
            if let Err(e) = state.save() {
                eprintln!("{}: {}", "Error".red(), e);
                return;
            }

            println!();
            println!("  {} Connected to savants.cloud", "●".green());

            // Auto-update all .mcp.json files with cloud URL
            crate::commands::mcp::install("user", "auto");

            println!();
            println!("  Restart Claude Code to use cloud tools.");
            println!("  You now have: diagnose_error, pr_risk, radar, and more.");
            return;
        }

        let error = body.get("error").and_then(|v| v.as_str()).unwrap_or("");
        match error {
            "authorization_pending" => continue,
            "slow_down" => {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
            "expired_token" => {
                eprintln!("Code expired. Run {} again.", "savants connect".cyan());
                return;
            }
            "access_denied" => {
                eprintln!("Denied.");
                return;
            }
            _ => continue,
        }
    }
    eprintln!("Timed out. Run {} again.", "savants connect".cyan());
}

pub fn disconnect() {
    let mut state = State::load();
    state.cloud_token = None;
    state.cloud_org = None;
    let _ = state.save();
    println!("Disconnected from savants.cloud.");
}
