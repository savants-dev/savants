/// Opt-in anonymous telemetry.
///
/// What we collect: tool name, duration_ms, repo language breakdown, OS/arch.
/// What we never collect: code, file paths, queries, function names, errors.
///
/// Enable: savants telemetry on
/// Disable: savants telemetry off
use crate::config::State;

const TELEMETRY_URL: &str = "https://api.savants.cloud/api/v1/telemetry";

pub fn send(tool: &str, duration_ms: u64) {
    let state = State::load();
    if !state.telemetry_enabled {
        return;
    }

    let device_id = match &state.telemetry_id {
        Some(id) => id.clone(),
        None => return,
    };

    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let body = serde_json::json!({
        "d": device_id,
        "t": tool,
        "ms": duration_ms,
        "os": os,
        "arch": arch,
        "v": env!("CARGO_PKG_VERSION"),
    });

    // Fire and forget in a background thread - never block tool calls
    let body_str = body.to_string();
    std::thread::spawn(move || {
        let _ = std::process::Command::new("curl")
            .args([
                "-sf",
                "--max-time",
                "2",
                "-X",
                "POST",
                "-H",
                "Content-Type: application/json",
                "-d",
                &body_str,
                TELEMETRY_URL,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    });
}

pub fn enable() {
    let mut state = State::load();
    state.telemetry_enabled = true;
    if state.telemetry_id.is_none() {
        state.telemetry_id = Some(generate_device_id());
    }
    let _ = state.save();
    eprintln!("Telemetry enabled. We collect: tool name, duration, OS/arch, version.");
    eprintln!("We never collect: code, file paths, queries, or function names.");
    eprintln!("Disable anytime: savants telemetry off");
}

pub fn disable() {
    let mut state = State::load();
    state.telemetry_enabled = false;
    let _ = state.save();
    eprintln!("Telemetry disabled. No data will be sent.");
}

pub fn status() {
    let state = State::load();
    if state.telemetry_enabled {
        eprintln!(
            "Telemetry: enabled (device: {})",
            state.telemetry_id.as_deref().unwrap_or("?")
        );
    } else {
        eprintln!("Telemetry: disabled. Enable with: savants telemetry on");
    }
}

fn generate_device_id() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    // Hash hostname + home dir for a stable anonymous ID
    if let Ok(hostname) = std::env::var("HOSTNAME").or_else(|_| std::env::var("HOST")) {
        hostname.hash(&mut hasher);
    }
    if let Some(home) = dirs::home_dir() {
        home.to_string_lossy().hash(&mut hasher);
    }
    // Add some randomness so two users on same machine get different IDs
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .hash(&mut hasher);

    format!("sv_{:016x}", hasher.finish())
}
