//! Ansible built-in profile for Porter.
//!
//! Ansible's command structure differs from typical CLIs: read-only operations
//! are specific flags/subcommands of the ansible-* family of tools.
//! We model the read-only subcommands as top-level commands with their key flags.

use std::collections::HashSet;
use std::sync::OnceLock;

use super::BuiltinProfile;

/// Built-in profile for Ansible (`ansible`).
pub struct AnsibleProfile;

/// Static set of read-only subcommand paths for ansible.
fn read_only_set() -> &'static HashSet<String> {
    static SET: OnceLock<HashSet<String>> = OnceLock::new();
    SET.get_or_init(|| {
        let mut s = HashSet::new();

        // ansible-inventory: --list, --graph, --host are read-only
        s.insert("ansible-inventory --list".to_string());
        s.insert("ansible-inventory --graph".to_string());
        s.insert("ansible-inventory --host".to_string());

        // ansible-config: list, dump, view, validate are read-only
        s.insert("ansible-config list".to_string());
        s.insert("ansible-config dump".to_string());
        s.insert("ansible-config view".to_string());
        s.insert("ansible-config validate".to_string());

        // ansible-doc: documentation lookup is read-only (all flag variants)
        s.insert("ansible-doc".to_string());
        s.insert("ansible-doc -l".to_string());
        s.insert("ansible-doc -s".to_string());
        s.insert("ansible-doc -F".to_string());
        s.insert("ansible-doc --metadata-dump".to_string());

        // ansible --version: read-only
        s.insert("ansible --version".to_string());

        // ansible --list-hosts: prints matching hosts, no execution
        s.insert("ansible --list-hosts".to_string());

        // ansible -m setup: fact gathering only
        s.insert("ansible -m setup".to_string());

        // ansible-galaxy: list, search, info, verify are read-only
        s.insert("ansible-galaxy list".to_string());
        s.insert("ansible-galaxy collection list".to_string());
        s.insert("ansible-galaxy collection verify".to_string());
        s.insert("ansible-galaxy role list".to_string());
        s.insert("ansible-galaxy role search".to_string());
        s.insert("ansible-galaxy role info".to_string());

        // ansible-vault: view is read-only
        s.insert("ansible-vault view".to_string());

        s
    })
}

impl BuiltinProfile for AnsibleProfile {
    fn name(&self) -> &'static str {
        "ansible"
    }

    fn default_inject_flags(&self) -> Vec<String> {
        // No standard JSON output flag for ansible
        vec![]
    }

    fn is_read_only(&self, args: &[&str]) -> bool {
        if args.is_empty() {
            return false;
        }

        // Try progressively shorter matches
        // For ansible, args[0] is a subcommand name like "ansible-inventory"
        // We join and check prefixes against the read-only set
        for len in (1..=args.len().min(3)).rev() {
            let key = args[..len].join(" ");
            if read_only_set().contains(&key) {
                return true;
            }
        }

        // Check if any read-only entry is a prefix of the joined args
        let joined = args.join(" ");
        read_only_set()
            .iter()
            .any(|entry| joined.starts_with(entry.as_str()))
    }

    fn read_only_subcommands(&self) -> Vec<Vec<String>> {
        read_only_set()
            .iter()
            .map(|entry| entry.split(' ').map(String::from).collect::<Vec<_>>())
            .collect()
    }
}
