//! Ripgrep (`rg`) built-in profile for Porter.
//!
//! Ripgrep is a purely read-only search tool. `is_read_only` always returns
//! true and `expand_by_default` returns false (rg is a single tool, not
//! expanded into subcommands).

use super::BuiltinProfile;

/// Built-in profile for Ripgrep (`rg`).
pub struct RgProfile;

impl BuiltinProfile for RgProfile {
    fn name(&self) -> &'static str {
        "rg"
    }

    fn default_inject_flags(&self) -> Vec<String> {
        vec!["--json".to_string()]
    }

    /// Ripgrep is search-only — all operations are read-only.
    fn is_read_only(&self, _args: &[&str]) -> bool {
        true
    }

    /// No meaningful subcommand structure.
    fn read_only_subcommands(&self) -> Vec<Vec<String>> {
        vec![]
    }

    /// rg is a single tool — do not expand into per-subcommand MCP tools.
    fn expand_by_default(&self) -> bool {
        false
    }
}
