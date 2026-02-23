//! Doggo DNS lookup tool built-in profile for Porter.
//!
//! Doggo is a purely read-only DNS lookup tool — all operations are read-only
//! by nature. `is_read_only` always returns true.

use super::BuiltinProfile;

/// Built-in profile for the Doggo DNS lookup tool (`doggo`).
pub struct DoggoProfile;

impl BuiltinProfile for DoggoProfile {
    fn name(&self) -> &'static str {
        "doggo"
    }

    fn default_inject_flags(&self) -> Vec<String> {
        vec!["--json".to_string()]
    }

    /// Doggo is a DNS lookup tool — all operations are read-only.
    fn is_read_only(&self, _args: &[&str]) -> bool {
        true
    }

    /// Doggo has no meaningful subcommand structure to expand.
    fn read_only_subcommands(&self) -> Vec<Vec<String>> {
        vec![]
    }
}
