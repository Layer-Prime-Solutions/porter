//! tldr documentation tool built-in profile for Porter.
//!
//! tldr shows simplified command documentation pages — all operations are
//! read-only. `expand_by_default` returns false as tldr is a single tool.

use super::BuiltinProfile;

/// Built-in profile for the tldr documentation tool (`tldr`).
pub struct TldrProfile;

impl BuiltinProfile for TldrProfile {
    fn name(&self) -> &'static str {
        "tldr"
    }

    fn default_inject_flags(&self) -> Vec<String> {
        vec![]
    }

    /// tldr shows documentation — all operations are read-only.
    fn is_read_only(&self, _args: &[&str]) -> bool {
        true
    }

    /// No meaningful subcommand structure.
    fn read_only_subcommands(&self) -> Vec<Vec<String>> {
        vec![]
    }

    /// tldr is a single documentation tool — do not expand into subcommands.
    fn expand_by_default(&self) -> bool {
        false
    }
}
