//! GitHub CLI (`gh`) built-in profile for Porter.

use std::collections::HashSet;
use std::sync::OnceLock;

use super::BuiltinProfile;

/// Built-in profile for the GitHub CLI (`gh`).
pub struct GhProfile;

/// Static set of read-only "group action" pairs for gh.
fn read_only_set() -> &'static HashSet<String> {
    static SET: OnceLock<HashSet<String>> = OnceLock::new();
    SET.get_or_init(|| {
        let mut s = HashSet::new();

        // repo
        for action in &["list", "view", "clone"] {
            s.insert(format!("repo {}", action));
        }

        // issue
        for action in &["list", "view", "status"] {
            s.insert(format!("issue {}", action));
        }

        // pr (pull request)
        for action in &["list", "view", "status", "checks", "diff"] {
            s.insert(format!("pr {}", action));
        }

        // release
        for action in &["list", "view"] {
            s.insert(format!("release {}", action));
        }

        // workflow / run
        for action in &["list", "view"] {
            s.insert(format!("workflow {}", action));
            s.insert(format!("run {}", action));
        }
        s.insert("run watch".to_string());

        // gist
        for action in &["list", "view"] {
            s.insert(format!("gist {}", action));
        }

        // label
        s.insert("label list".to_string());

        // milestone
        s.insert("milestone list".to_string());

        // variable / secret (list only — not values)
        s.insert("variable list".to_string());
        s.insert("secret list".to_string());

        // api (GET requests are read-only — we mark "api" as read-only and
        // rely on users being responsible; gh api is an escape hatch)
        s.insert("api".to_string());

        s
    })
}

impl BuiltinProfile for GhProfile {
    fn name(&self) -> &'static str {
        "gh"
    }

    fn default_inject_flags(&self) -> Vec<String> {
        // gh --json only works on some subcommands, not globally.
        // We don't inject it by default; users can add it per-invocation.
        vec![]
    }

    fn is_read_only(&self, args: &[&str]) -> bool {
        if args.is_empty() {
            return false;
        }
        // Try 2-token key first (e.g., "pr list"), then 1-token (e.g., "api")
        if args.len() >= 2 {
            let key2 = format!("{} {}", args[0], args[1]);
            if read_only_set().contains(&key2) {
                return true;
            }
        }
        // Single token (e.g., "api")
        read_only_set().contains(args[0])
    }

    fn read_only_subcommands(&self) -> Vec<Vec<String>> {
        read_only_set()
            .iter()
            .map(|entry| entry.split(' ').map(String::from).collect::<Vec<_>>())
            .collect()
    }
}
