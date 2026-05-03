fn main() {
    // Version from git tag at build time. No manual Cargo.toml bumps needed.
    // CI tags (v0.17.3) -> binary reports 0.17.3
    // Local dev (no tag reachable) -> falls back to Cargo.toml version
    let version = std::process::Command::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().trim_start_matches('v').to_string())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

    println!("cargo:rustc-env=SAVANTS_VERSION={}", version);
    println!("cargo:rerun-if-changed=.git/refs/tags");
}
