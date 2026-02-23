//! Access control enforcement for CLI tool invocations.
//!
//! Implements deny-first access guard with allow/deny prefix lists and
//! per-subcommand write-access opt-in. Deny overrides allow (highest priority).

use std::collections::HashMap;
use std::fmt;

use crate::config::CliServerConfig;

/// Type alias for the read-only checker function stored in AccessGuard.
type ReadOnlyFn = Box<dyn Fn(&[&str]) -> bool + Send + Sync>;

/// Access control decision error for a CLI invocation.
///
/// This is a local enum (not PorterError) that gets mapped to
/// `PorterError::AccessDenied` at the harness layer.
#[derive(Debug, Clone)]
pub enum AccessDenied {
    /// Subcommand matched an explicit deny prefix.
    ExplicitDeny { subcommand: String },
    /// Subcommand is a write operation not opted-in via write_access.
    WriteBlocked { subcommand: String, hint: String },
    /// Allow list is non-empty and subcommand doesn't match any entry.
    NotInAllowList { subcommand: String },
}

impl fmt::Display for AccessDenied {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AccessDenied::ExplicitDeny { subcommand } => {
                write!(f, "Command blocked: {} is explicitly denied", subcommand)
            }
            AccessDenied::WriteBlocked {
                subcommand: _,
                hint,
            } => {
                write!(f, "{}", hint)
            }
            AccessDenied::NotInAllowList { subcommand } => {
                write!(
                    f,
                    "Command blocked: {} is not in the allow list",
                    subcommand
                )
            }
        }
    }
}

/// Access guard enforcing deny-first allow/deny lists and write-access controls.
///
/// Evaluation order (strict priority):
/// 1. Deny list — explicit deny prefix matches always block (highest priority)
/// 2. Write-only check — if profile marks as write, must have write_access opt-in
/// 3. Allow list — if non-empty, subcommand must match at least one prefix
/// 4. Pass — no restrictions matched
pub struct AccessGuard {
    allow: Vec<String>,
    deny: Vec<String>,
    write_access: HashMap<String, bool>,
    is_read_only_fn: Option<ReadOnlyFn>,
}

impl AccessGuard {
    /// Build an AccessGuard from a CLI server config.
    pub fn new(config: &CliServerConfig) -> Self {
        AccessGuard {
            allow: config.allow.clone(),
            deny: config.deny.clone(),
            write_access: config.write_access.clone(),
            is_read_only_fn: None,
        }
    }

    /// Attach a read-only checker function (typically from a built-in profile).
    ///
    /// The function receives the full args slice and returns `true` if the
    /// operation is read-only. Called in Step 2 of access evaluation.
    pub fn with_read_only_checker(
        mut self,
        f: impl Fn(&[&str]) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.is_read_only_fn = Some(Box::new(f));
        self
    }

    /// Check whether the given command invocation is allowed.
    ///
    /// Returns `Ok(())` if allowed, or an `AccessDenied` variant describing
    /// why the invocation was blocked.
    ///
    /// # Arguments
    /// * `command` - The base command name (e.g., "aws")
    /// * `args` - All args passed to the command (subcommand path + flags)
    pub fn check(&self, command: &str, args: &[&str]) -> std::result::Result<(), AccessDenied> {
        // Join args to form the subcommand path for prefix matching
        let subcommand_path = args.join(" ");

        // Step 1: Deny list — highest priority, always blocks
        for deny_prefix in &self.deny {
            if subcommand_path.starts_with(deny_prefix.as_str()) {
                return Err(AccessDenied::ExplicitDeny {
                    subcommand: subcommand_path.clone(),
                });
            }
        }

        // Step 2: Write-only check (from profile read-only checker)
        if let Some(is_read_only) = &self.is_read_only_fn {
            if !is_read_only(args) {
                // Operation is a write — check for explicit opt-in
                let allowed_write = self.write_access.iter().any(|(prefix, &allowed)| {
                    allowed && subcommand_path.starts_with(prefix.as_str())
                });
                if !allowed_write {
                    let hint = format!(
                        "Command blocked: {} {} is a write operation. Enable write_access in config to allow.",
                        command,
                        subcommand_path
                    );
                    return Err(AccessDenied::WriteBlocked {
                        subcommand: subcommand_path.clone(),
                        hint,
                    });
                }
            }
        }

        // Step 3: Allow list — if non-empty, subcommand must match at least one prefix
        if !self.allow.is_empty() {
            let matched = self
                .allow
                .iter()
                .any(|prefix| subcommand_path.starts_with(prefix.as_str()));
            if !matched {
                return Err(AccessDenied::NotInAllowList {
                    subcommand: subcommand_path,
                });
            }
        }

        // Step 4: Pass — no restrictions matched
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CliServerConfig, TransportKind};
    use std::collections::HashMap;

    fn make_config(
        allow: Vec<&str>,
        deny: Vec<&str>,
        write_access: HashMap<String, bool>,
    ) -> CliServerConfig {
        CliServerConfig {
            slug: "test".to_string(),
            enabled: true,
            transport: TransportKind::Cli,
            command: "aws".to_string(),
            profile: None,
            args: vec![],
            env: HashMap::new(),
            allow: allow.into_iter().map(String::from).collect(),
            deny: deny.into_iter().map(String::from).collect(),
            write_access,
            timeout_secs: 30,
            inject_flags: vec![],
            expand_subcommands: None,
            schema_override: None,
            help_depth: None,
            discovery_budget_secs: 60,
        }
    }

    #[test]
    fn test_deny_overrides_allow() {
        // Even when the subcommand is in the allow list, deny wins
        let config = make_config(
            vec!["s3"],        // allow s3 operations
            vec!["s3 delete"], // deny s3 delete
            HashMap::new(),
        );
        let guard = AccessGuard::new(&config);

        // s3 ls should be allowed (in allow list, not in deny)
        assert!(guard.check("aws", &["s3", "ls"]).is_ok());

        // s3 delete should be denied even though s3 is in allow list
        let result = guard.check("aws", &["s3", "delete"]);
        assert!(
            matches!(result, Err(AccessDenied::ExplicitDeny { .. })),
            "deny must override allow"
        );
    }

    #[test]
    fn test_write_operation_blocked_by_default() {
        let config = make_config(vec![], vec![], HashMap::new());
        let guard = AccessGuard::new(&config).with_read_only_checker(|_args| {
            // ec2 describe-* is read-only, everything else is write
            false
        });

        // Write operation without write_access should be blocked
        let result = guard.check("aws", &["ec2", "run-instances"]);
        assert!(
            matches!(result, Err(AccessDenied::WriteBlocked { .. })),
            "write operations must be blocked by default"
        );
    }

    #[test]
    fn test_write_operation_allowed_with_explicit_write_access() {
        let mut write_access = HashMap::new();
        write_access.insert("ec2 run-instances".to_string(), true);

        let config = make_config(vec![], vec![], write_access);
        let guard = AccessGuard::new(&config).with_read_only_checker(|_args| {
            // Nothing is read-only in this test
            false
        });

        // Write operation with write_access opt-in should pass
        assert!(guard.check("aws", &["ec2", "run-instances"]).is_ok());
    }

    #[test]
    fn test_empty_allow_list_means_allow_all() {
        // Empty allow list = no restriction, not "deny all"
        let config = make_config(vec![], vec![], HashMap::new());
        let guard = AccessGuard::new(&config);

        assert!(guard.check("aws", &["s3", "ls"]).is_ok());
        assert!(guard.check("aws", &["ec2", "describe-instances"]).is_ok());
        assert!(guard.check("aws", &["iam", "list-users"]).is_ok());
    }

    #[test]
    fn test_not_in_allow_list_blocked() {
        let config = make_config(vec!["s3", "ec2 describe"], vec![], HashMap::new());
        let guard = AccessGuard::new(&config);

        // s3 is in allow list
        assert!(guard.check("aws", &["s3", "ls"]).is_ok());

        // iam is NOT in allow list
        let result = guard.check("aws", &["iam", "list-users"]);
        assert!(
            matches!(result, Err(AccessDenied::NotInAllowList { .. })),
            "operations not in allow list should be blocked"
        );
    }

    #[test]
    fn test_write_blocked_error_message_format() {
        let config = make_config(vec![], vec![], HashMap::new());
        let guard = AccessGuard::new(&config).with_read_only_checker(|_| false);

        let result = guard.check("aws", &["ec2", "terminate-instances"]);
        if let Err(AccessDenied::WriteBlocked { hint, .. }) = result {
            assert!(
                hint.contains("is a write operation"),
                "error message should mention write operation: {}",
                hint
            );
            assert!(
                hint.contains("Enable write_access in config"),
                "error message should guide user: {}",
                hint
            );
        } else {
            panic!("expected WriteBlocked error");
        }
    }
}
