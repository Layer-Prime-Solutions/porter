//! GitLab CLI (`glab`) built-in profile for Porter.

use std::collections::HashSet;
use std::sync::OnceLock;

use super::BuiltinProfile;

/// Built-in profile for the GitLab CLI (`glab`).
pub struct GitlabProfile;

/// Static set of read-only "group action" pairs for glab.
fn read_only_set() -> &'static HashSet<String> {
    static SET: OnceLock<HashSet<String>> = OnceLock::new();
    SET.get_or_init(|| {
        let mut s = HashSet::new();

        // mr (merge requests)
        for action in &["list", "view", "diff", "approvers", "issues"] {
            s.insert(format!("mr {}", action));
        }

        // issue
        for action in &["list", "view"] {
            s.insert(format!("issue {}", action));
        }
        s.insert("issue board view".to_string());

        // project
        for action in &["list", "view", "search"] {
            s.insert(format!("project {}", action));
        }

        // pipeline / ci
        for action in &["list", "view", "status"] {
            s.insert(format!("pipeline {}", action));
        }
        for action in &["get", "list", "status", "trace", "view", "lint"] {
            s.insert(format!("ci {}", action));
        }

        // incident
        for action in &["list", "view"] {
            s.insert(format!("incident {}", action));
        }

        // iteration
        s.insert("iteration list".to_string());

        // job
        s.insert("job artifact".to_string());

        // release
        for action in &["list", "view", "download"] {
            s.insert(format!("release {}", action));
        }

        // snippet
        for action in &["list", "view"] {
            s.insert(format!("snippet {}", action));
        }

        // label
        for action in &["list", "get"] {
            s.insert(format!("label {}", action));
        }

        // milestone
        for action in &["list", "get"] {
            s.insert(format!("milestone {}", action));
        }

        // deploy-key
        for action in &["list", "get"] {
            s.insert(format!("deploy-key {}", action));
        }

        // gpg-key
        for action in &["list", "get"] {
            s.insert(format!("gpg-key {}", action));
        }

        // ssh-key
        for action in &["list", "get"] {
            s.insert(format!("ssh-key {}", action));
        }

        // schedule
        s.insert("schedule list".to_string());

        // securefile
        for action in &["list", "get"] {
            s.insert(format!("securefile {}", action));
        }

        // token
        s.insert("token list".to_string());

        // user
        s.insert("user events".to_string());

        // variable
        for action in &["list", "get", "export"] {
            s.insert(format!("variable {}", action));
        }

        // repo
        for action in &["list", "view", "search", "archive", "contributors"] {
            s.insert(format!("repo {}", action));
        }

        // auth
        s.insert("auth status".to_string());

        // config
        s.insert("config get".to_string());

        // cluster agent
        s.insert("cluster agent list".to_string());

        // stack
        s.insert("stack list".to_string());

        // runner-controller
        s.insert("runner-controller list".to_string());

        // version (top-level)
        s.insert("version".to_string());

        s
    })
}

impl BuiltinProfile for GitlabProfile {
    fn name(&self) -> &'static str {
        "gitlab"
    }

    fn default_inject_flags(&self) -> Vec<String> {
        vec!["-o".to_string(), "json".to_string()]
    }

    fn is_read_only(&self, args: &[&str]) -> bool {
        if args.is_empty() {
            return false;
        }
        // Try progressively shorter matches (3-token, 2-token, 1-token)
        for len in (1..=args.len().min(3)).rev() {
            let key = args[..len].join(" ");
            if read_only_set().contains(&key) {
                return true;
            }
        }
        false
    }

    fn read_only_subcommands(&self) -> Vec<Vec<String>> {
        read_only_set()
            .iter()
            .map(|entry| entry.split(' ').map(String::from).collect::<Vec<_>>())
            .collect()
    }
}
