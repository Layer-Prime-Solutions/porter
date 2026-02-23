//! CLI harness modules for Porter — wraps arbitrary CLI tools as MCP tools.
//!
//! The CLI transport allows external command-line tools (e.g., `aws`, `kubectl`)
//! to be registered as namespaced MCP tools within Porter. Each CLI tool is
//! executed via structured args (never shell), with access control enforcement
//! and timeout-kill semantics.

pub mod access_guard;
pub mod discovery;
pub mod harness;
pub mod help_parser;
pub mod profiles;
pub mod read_only_heuristic;
pub mod subcommand_parser;

pub use access_guard::{AccessDenied, AccessGuard};
pub use discovery::{discover_subcommands, DiscoveryConfig, DiscoveryResult};
pub use harness::{CliHandle, CliHarness};
pub use help_parser::{parse_help_output, ArgumentSchema};
pub use profiles::{available_profiles, get_profile, BuiltinProfile};
pub use read_only_heuristic::is_likely_read_only;
pub use subcommand_parser::parse_subcommands;
