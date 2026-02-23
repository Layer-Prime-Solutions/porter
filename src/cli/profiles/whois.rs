//! Whois lookup tool built-in profile for Porter.
//!
//! Whois is a purely read-only domain/IP lookup tool. `expand_by_default`
//! returns false as whois is a single tool with no meaningful subcommands.

use super::BuiltinProfile;

/// Built-in profile for the Whois lookup tool (`whois`).
pub struct WhoisProfile;

impl BuiltinProfile for WhoisProfile {
    fn name(&self) -> &'static str {
        "whois"
    }

    fn default_inject_flags(&self) -> Vec<String> {
        vec![]
    }

    /// Whois is a DNS/IP lookup tool — all operations are read-only.
    fn is_read_only(&self, _args: &[&str]) -> bool {
        true
    }

    /// No meaningful subcommand structure.
    fn read_only_subcommands(&self) -> Vec<Vec<String>> {
        vec![]
    }

    /// whois is a single lookup tool — do not expand into subcommands.
    fn expand_by_default(&self) -> bool {
        false
    }
}
