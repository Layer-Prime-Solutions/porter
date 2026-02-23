//! Verb-based read-only classifier for CLI tools without a built-in profile.
//!
//! Provides best-effort read/write classification by matching common CLI verb
//! patterns in the subcommand path. Conservative default: unknown verbs are
//! classified as write (blocked).

/// Read verbs — match means the operation is likely read-only.
const READ_VERBS: &[&str] = &[
    "list", "get", "describe", "show", "view", "inspect", "status", "info", "ls", "cat", "log",
    "logs", "top", "explain", "check", "verify", "whoami", "version", "help", "search", "find",
    "count", "exists", "diff", "compare", "history", "print", "dump", "export",
];

/// Write verbs — match means the operation is definitely NOT read-only.
const WRITE_VERBS: &[&str] = &[
    "create", "delete", "remove", "rm", "update", "set", "put", "apply", "patch", "edit",
    "modify", "replace", "destroy", "kill", "stop", "start", "restart", "terminate", "drain",
    "cordon", "taint", "push", "deploy", "rollback", "scale", "resize", "move", "mv", "cp",
    "copy", "migrate", "import", "exec", "run",
];

/// Classify a subcommand path as likely read-only based on verb patterns.
///
/// Checks the **last token** in the path first (most specific), then walks
/// backwards. First match wins. If no verb matches, returns `false`
/// (conservative — unknown = write = blocked).
///
/// # Examples
///
/// ```
/// use nimbus_porter::cli::read_only_heuristic::is_likely_read_only;
///
/// assert!(is_likely_read_only(&["s3", "ls"]));          // "ls" is a read verb
/// assert!(!is_likely_read_only(&["ec2", "terminate"])); // unknown but no match → write
/// assert!(!is_likely_read_only(&["s3", "rm"]));         // "rm" is a write verb
/// ```
pub fn is_likely_read_only(subcommand_path: &[&str]) -> bool {
    // Walk backwards — last token is most specific
    for token in subcommand_path.iter().rev() {
        let lower = token.to_lowercase();

        // Check read verbs
        if READ_VERBS.contains(&lower.as_str()) {
            return true;
        }

        // Check write verbs
        if WRITE_VERBS.contains(&lower.as_str()) {
            return false;
        }

        // Also check for compound verbs with hyphens (e.g., "describe-instances")
        // Extract the first segment before the first hyphen
        if let Some(prefix) = lower.split('-').next() {
            if READ_VERBS.contains(&prefix) {
                return true;
            }
            if WRITE_VERBS.contains(&prefix) {
                return false;
            }
        }
    }

    // No verb matched — conservative default: classify as write (blocked)
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_verbs() {
        assert!(is_likely_read_only(&["list"]));
        assert!(is_likely_read_only(&["get"]));
        assert!(is_likely_read_only(&["describe"]));
        assert!(is_likely_read_only(&["show"]));
        assert!(is_likely_read_only(&["view"]));
        assert!(is_likely_read_only(&["inspect"]));
        assert!(is_likely_read_only(&["status"]));
        assert!(is_likely_read_only(&["info"]));
        assert!(is_likely_read_only(&["ls"]));
        assert!(is_likely_read_only(&["cat"]));
        assert!(is_likely_read_only(&["log"]));
        assert!(is_likely_read_only(&["logs"]));
        assert!(is_likely_read_only(&["search"]));
        assert!(is_likely_read_only(&["find"]));
        assert!(is_likely_read_only(&["diff"]));
        assert!(is_likely_read_only(&["export"]));
    }

    #[test]
    fn test_write_verbs() {
        assert!(!is_likely_read_only(&["create"]));
        assert!(!is_likely_read_only(&["delete"]));
        assert!(!is_likely_read_only(&["remove"]));
        assert!(!is_likely_read_only(&["rm"]));
        assert!(!is_likely_read_only(&["update"]));
        assert!(!is_likely_read_only(&["apply"]));
        assert!(!is_likely_read_only(&["destroy"]));
        assert!(!is_likely_read_only(&["kill"]));
        assert!(!is_likely_read_only(&["exec"]));
        assert!(!is_likely_read_only(&["run"]));
        assert!(!is_likely_read_only(&["deploy"]));
        assert!(!is_likely_read_only(&["push"]));
    }

    #[test]
    fn test_unknown_defaults_to_write() {
        assert!(!is_likely_read_only(&["frobnicate"]));
        assert!(!is_likely_read_only(&["widget"]));
        assert!(!is_likely_read_only(&["something-weird"]));
    }

    #[test]
    fn test_multi_token_last_wins() {
        // "aws s3 ls" — "ls" is the last token, is read
        assert!(is_likely_read_only(&["s3", "ls"]));

        // "kubectl get pods" — "pods" doesn't match any verb, but "get" does (walk backwards)
        assert!(is_likely_read_only(&["get", "pods"]));

        // "ec2 describe-instances" — "describe-instances" starts with "describe" (read)
        assert!(is_likely_read_only(&["ec2", "describe-instances"]));

        // "s3 rm" — "rm" is write
        assert!(!is_likely_read_only(&["s3", "rm"]));
    }

    #[test]
    fn test_compound_verbs_with_hyphens() {
        assert!(is_likely_read_only(&["describe-instances"]));
        assert!(is_likely_read_only(&["list-buckets"]));
        assert!(is_likely_read_only(&["get-object"]));
        assert!(!is_likely_read_only(&["create-bucket"]));
        assert!(!is_likely_read_only(&["delete-objects"]));
        assert!(!is_likely_read_only(&["run-task"]));
    }

    #[test]
    fn test_case_insensitive() {
        assert!(is_likely_read_only(&["LIST"]));
        assert!(is_likely_read_only(&["Get"]));
        assert!(!is_likely_read_only(&["DELETE"]));
        assert!(!is_likely_read_only(&["Create"]));
    }

    #[test]
    fn test_empty_path() {
        assert!(!is_likely_read_only(&[]));
    }
}
