//! Build-time version identity for MCP `initialize` and unauthenticated `/health`.

use finance::AlertEvaluatorDig;
use serde::Serialize;

/// Cargo package SemVer from `skill-master/Cargo.toml`.
pub const CARGO_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Full git commit SHA embedded at build (or `"unknown"`).
pub const GIT_SHA: &str = env!("GIT_COMMIT");

/// Optional image / release tag when `IMAGE_TAG` was set at build.
pub fn image_tag() -> Option<&'static str> {
    option_env!("IMAGE_TAG")
}

/// Short SHA (up to 8 hex chars) for SemVer build metadata.
pub fn short_git_sha() -> &'static str {
    let sha = GIT_SHA;
    if sha == "unknown" {
        return sha;
    }
    let end = sha.len().min(8);
    &sha[..end]
}

/// MCP `serverInfo.version`: `{cargo_version}+{short_git_sha}` (SemVer build metadata).
pub fn mcp_server_version() -> String {
    format!("{}+{}", CARGO_VERSION, short_git_sha())
}

/// JSON body for `GET /health` (probes without MCP auth).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HealthInfo {
    pub version: String,
    pub git_sha: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_tag: Option<&'static str>,
    /// Structural alert-evaluator dig (no magnitudes/secrets).
    pub alert_evaluator: AlertEvaluatorDig,
}

pub fn health_info(alert_evaluator: AlertEvaluatorDig) -> HealthInfo {
    HealthInfo {
        version: mcp_server_version(),
        git_sha: GIT_SHA,
        image_tag: image_tag(),
        alert_evaluator,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use finance::AlertEvaluatorStatus;

    #[test]
    fn mcp_server_version_uses_semver_build_metadata() {
        let v = mcp_server_version();
        let (semver, meta) =
            v.split_once('+').expect("SemVer build metadata (+)");
        assert_eq!(semver, CARGO_VERSION);
        assert!(!meta.is_empty());
        assert!(meta.len() <= 8);
        if meta != "unknown" {
            assert!(
                meta.chars().all(|c| c.is_ascii_hexdigit()),
                "expected hex short sha, got {meta}"
            );
        }
        // Example shape from architect: `0.1.45+258dcfad`
        assert_eq!(v, format!("{CARGO_VERSION}+{}", short_git_sha()));
    }

    #[test]
    fn health_info_includes_version_git_sha_and_evaluator_dig() {
        let status = AlertEvaluatorStatus::new();
        let info = health_info(status.dig());
        assert_eq!(info.version, mcp_server_version());
        assert_eq!(info.git_sha, GIT_SHA);
        assert!(!info.alert_evaluator.running);
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["version"], info.version);
        assert_eq!(json["git_sha"], GIT_SHA);
        assert_eq!(json["alert_evaluator"]["running"], false);
        if image_tag().is_none() {
            assert!(json.get("image_tag").is_none());
        } else {
            assert_eq!(json["image_tag"], image_tag().unwrap());
        }
    }

    #[test]
    fn health_info_evaluator_dig_no_cash_magnitude() {
        let status = AlertEvaluatorStatus::new();
        status.set_running(true);
        status.record_tick(Some(99_999.5), 3, 0, chrono::Utc::now());
        let json = serde_json::to_string(&health_info(status.dig())).unwrap();
        assert!(json.contains("\"Some\""));
        assert!(!json.contains("99999"));
        assert!(json.contains("\"enabled_watches\":3"));
    }

    #[test]
    fn short_git_sha_truncates_to_eight() {
        let short = short_git_sha();
        if GIT_SHA != "unknown" {
            assert_eq!(short, &GIT_SHA[..GIT_SHA.len().min(8)]);
            assert!(short.len() <= 8);
        }
    }
}
