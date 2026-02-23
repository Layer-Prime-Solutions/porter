//! Kubernetes CLI (`kubectl`) built-in profile for Porter.

use std::collections::HashSet;
use std::sync::OnceLock;

use super::BuiltinProfile;

/// Built-in profile for the Kubernetes CLI (`kubectl`).
pub struct KubectlProfile;

/// Static set of read-only top-level subcommands for kubectl.
fn read_only_set() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        let mut s = HashSet::new();
        s.insert("get");
        s.insert("describe");
        s.insert("logs");
        s.insert("top");
        s.insert("api-resources");
        s.insert("api-versions");
        s.insert("cluster-info");
        s.insert("explain");
        s.insert("version");
        s.insert("config"); // config view, get-contexts, current-context are read-only
        s
    })
}

impl BuiltinProfile for KubectlProfile {
    fn name(&self) -> &'static str {
        "kubectl"
    }

    fn default_inject_flags(&self) -> Vec<String> {
        vec!["-o".to_string(), "json".to_string()]
    }

    fn is_read_only(&self, args: &[&str]) -> bool {
        if args.is_empty() {
            return false;
        }
        // Top-level subcommand is args[0]
        let subcmd = args[0];

        // Special case: `config` subcommands — only read-only ones
        if subcmd == "config" {
            if let Some(sub) = args.get(1) {
                return matches!(
                    *sub,
                    "view" | "get-contexts" | "get-clusters" | "get-users" | "current-context"
                );
            }
            return false;
        }

        read_only_set().contains(subcmd)
    }

    fn read_only_subcommands(&self) -> Vec<Vec<String>> {
        let mut cmds: Vec<Vec<String>> = read_only_set()
            .iter()
            .filter(|&&s| s != "config") // config handled separately below
            .map(|&s| vec![s.to_string()])
            .collect();

        // Add config read-only subcommands
        for sub in &[
            "view",
            "get-contexts",
            "get-clusters",
            "get-users",
            "current-context",
        ] {
            cmds.push(vec!["config".to_string(), sub.to_string()]);
        }

        cmds
    }
}
