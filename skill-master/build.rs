//! Embed deploy identity at compile time.
//!
//! Prefer `GIT_COMMIT` / `IMAGE_TAG` from the environment (Docker/CI set these).
//! Fall back to `git rev-parse HEAD` for local builds so MCP/health still expose a SHA.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=GIT_COMMIT");
    println!("cargo:rerun-if-env-changed=IMAGE_TAG");
    println!("cargo:rerun-if-changed=../.git/HEAD");

    let git_commit = std::env::var("GIT_COMMIT")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(git_rev_parse_head)
        .unwrap_or_else(|| "unknown".to_string());

    // Never embed secrets — only the commit SHA (or "unknown").
    println!("cargo:rustc-env=GIT_COMMIT={git_commit}");

    if let Ok(tag) = std::env::var("IMAGE_TAG") {
        let tag = tag.trim();
        if !tag.is_empty() {
            println!("cargo:rustc-env=IMAGE_TAG={tag}");
        }
    }
}

fn git_rev_parse_head() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8(output.stdout).ok()?;
    let sha = sha.trim().to_string();
    if sha.is_empty() { None } else { Some(sha) }
}
