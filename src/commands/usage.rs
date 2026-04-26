use crate::config::State;
use colored::*;

const CLOUD_ENDPOINT: &str = "https://api.savants.cloud";

pub async fn run() {
    let state = State::load();
    let token = match &state.cloud_token {
        Some(t) if !t.is_empty() => t.clone(),
        _ => {
            println!("{}", "Not connected to savants.cloud".yellow());
            println!("  Run {} to connect.", "savants connect".cyan());
            return;
        }
    };

    println!("{}", "Savants Usage".bold());
    println!();

    let client = reqwest::Client::new();
    let resp = match client
        .get(&format!("{}/api/v1/usage", CLOUD_ENDPOINT))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r.json::<serde_json::Value>().await.unwrap_or_default(),
        Ok(r) if r.status().as_u16() == 401 => {
            println!("  Session expired. Run {} again.", "savants connect".cyan());
            return;
        }
        _ => {
            println!("  Could not reach savants.cloud");
            return;
        }
    };

    let period = resp.get("period").and_then(|v| v.as_str()).unwrap_or("?");
    let total_calls = resp
        .get("total_calls")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let total_cost = resp
        .get("total_cost_cents")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let plan = resp.get("plan").and_then(|v| v.as_str()).unwrap_or("?");

    println!("  Period:  {}", period.cyan());
    println!("  Plan:    {}", plan.green());
    println!("  Calls:   {}", total_calls);
    println!("  Cost:    ${:.2}", total_cost as f64 / 100.0);

    if let Some(tools) = resp.get("by_tool").and_then(|v| v.as_array()) {
        if !tools.is_empty() {
            println!();
            for tool in tools {
                let name = tool.get("tool").and_then(|v| v.as_str()).unwrap_or("?");
                let calls = tool.get("calls").and_then(|v| v.as_i64()).unwrap_or(0);
                println!("  {:<25} {} calls", name, calls);
            }
        }
    }
}
