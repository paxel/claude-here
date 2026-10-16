//! The seccomp profile handed to `docker run` when a toolchain needs syscalls
//! that Docker's default profile reserves for `CAP_SYS_ADMIN` (ADR 0010).
//! Rootless podman needs `unshare`, `mount`, `pivot_root` and friends; the
//! sandbox does not have that capability and must not get it, so the default
//! profile is shipped verbatim and one allow rule is appended.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

/// Docker's default profile, verbatim. Source:
/// <https://raw.githubusercontent.com/moby/profiles/main/seccomp/default.json>,
/// copied 2026-10-16. Re-copy the file when upstream changes it; nothing in
/// it is edited here.
pub const DOCKER_DEFAULT: &str = include_str!("../images/seccomp-default.json");

/// The default profile plus one unconditional allow rule for `syscalls`.
/// With no syscalls the default is returned as it is.
pub fn profile(syscalls: &[&str]) -> Result<String> {
    if syscalls.is_empty() {
        return Ok(DOCKER_DEFAULT.to_string());
    }
    let mut doc: Value =
        serde_json::from_str(DOCKER_DEFAULT).context("parsing the default seccomp profile")?;
    let Some(rules) = doc.get_mut("syscalls").and_then(Value::as_array_mut) else {
        bail!("default seccomp profile has no syscalls array");
    };
    rules.push(json!({
        "names": syscalls,
        "action": "SCMP_ACT_ALLOW",
        "comment": "claude_here: needed by rootless podman inside the sandbox (ADR 0010)"
    }));
    serde_json::to_string_pretty(&doc).context("serializing the seccomp profile")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_returned_untouched_without_syscalls() -> Result<()> {
        assert_eq!(profile(&[])?, DOCKER_DEFAULT);
        Ok(())
    }

    #[test]
    fn rule_is_appended_and_the_default_action_kept() -> Result<()> {
        let before: Value = serde_json::from_str(DOCKER_DEFAULT)?;
        let after: Value = serde_json::from_str(&profile(&["unshare", "mount"])?)?;
        assert_eq!(after["defaultAction"], before["defaultAction"]);
        assert_eq!(after["defaultAction"], "SCMP_ACT_ERRNO");
        let n = before["syscalls"].as_array().map_or(0, Vec::len);
        let rules = after["syscalls"].as_array().cloned().unwrap_or_default();
        assert_eq!(rules.len(), n + 1);
        let last = &rules[n];
        assert_eq!(last["action"], "SCMP_ACT_ALLOW");
        assert_eq!(last["names"], json!(["unshare", "mount"]));
        assert!(
            last.get("includes").is_none(),
            "the rule must be unconditional"
        );
        Ok(())
    }

    /// The syscalls podman needs are exactly the ones the default profile
    /// gates behind CAP_SYS_ADMIN (or does not mention at all); nothing in the
    /// list is already allowed unconditionally, or the list is stale.
    #[test]
    fn podman_syscalls_are_not_already_allowed() -> Result<()> {
        let doc: Value = serde_json::from_str(DOCKER_DEFAULT)?;
        let rules = doc["syscalls"].as_array().cloned().unwrap_or_default();
        for name in crate::toolchain::PODMAN_SYSCALLS {
            let unconditional = rules.iter().any(|r| {
                r["action"] == "SCMP_ACT_ALLOW"
                    && r.get("includes").is_none_or(|i| i == &json!({}))
                    && r.get("args").is_none()
                    && r["names"]
                        .as_array()
                        .is_some_and(|ns| ns.iter().any(|n| n == name))
            });
            assert!(!unconditional, "{name} is already allowed by the default");
        }
        Ok(())
    }
}
