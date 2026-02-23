//! Subcommand section parser — extracts subcommand names from CLI help output.
//!
//! Pure text parser (no I/O). Peer to `help_parser.rs` which handles flags.
//! Scans for section headers like "COMMANDS", "Available Commands", "GROUPS",
//! then collects indented entries as subcommand names.

/// A subcommand discovered from parsing --help output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSubcommand {
    pub name: String,
    pub description: Option<String>,
}

/// Noise tokens that should be filtered from subcommand lists.
const NOISE_COMMANDS: &[&str] = &["help", "version", "completion", "completions"];

/// Section headers (case-insensitive) that introduce subcommand lists.
const SECTION_HEADERS: &[&str] = &[
    "commands",
    "available commands",
    "subcommands",
    "groups",
    "core commands",
    "management commands",
    "other commands",
];

/// Parse subcommand names from CLI help text.
///
/// Scans for known section headers, then collects indented entries beneath them.
/// Entry criteria: 2+ leading spaces, first token matches `[a-zA-Z][a-zA-Z0-9_-]*`,
/// not a flag (no leading `-`).
pub fn parse_subcommands(help_text: &str) -> Vec<DiscoveredSubcommand> {
    let mut results = Vec::new();
    let mut in_section = false;

    for line in help_text.lines() {
        let trimmed = line.trim();

        // Empty line doesn't end a section (commands can have blank line gaps)
        if trimmed.is_empty() {
            continue;
        }

        // Check if this is a section header
        if is_section_header(line, trimmed) {
            in_section = true;
            continue;
        }

        // If at zero indent with non-empty content, end the section
        if in_section && !line.starts_with(' ') && !line.starts_with('\t') {
            in_section = false;
            continue;
        }

        if !in_section {
            continue;
        }

        // Must have 2+ leading spaces (indented entry)
        let leading_spaces = line.len() - line.trim_start().len();
        if leading_spaces < 2 {
            continue;
        }

        // Extract first token
        let tokens: Vec<&str> = trimmed.splitn(2, |c: char| c.is_whitespace()).collect();
        if tokens.is_empty() {
            continue;
        }

        let mut name = tokens[0].to_string();

        // Skip flags (lines starting with -)
        if name.starts_with('-') {
            continue;
        }

        // Must match [a-zA-Z][a-zA-Z0-9_-]*
        if !is_valid_subcommand_name(&name) {
            continue;
        }

        // Strip trailing colon (gh style: "repo:" → "repo")
        if name.ends_with(':') {
            name.pop();
        }

        // Filter noise commands
        if NOISE_COMMANDS.contains(&name.as_str()) {
            continue;
        }

        let description = tokens
            .get(1)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        // Deduplicate
        if !results.iter().any(|r: &DiscoveredSubcommand| r.name == name) {
            results.push(DiscoveredSubcommand { name, description });
        }
    }

    results
}

/// Check if a line is a section header.
fn is_section_header(raw_line: &str, trimmed: &str) -> bool {
    // Must be at zero indent (or close to it — some tools indent headers slightly)
    let leading = raw_line.len() - raw_line.trim_start().len();
    if leading > 1 {
        return false;
    }

    // Strip trailing colon for matching (e.g., "Commands:" → "Commands")
    let header = trimmed.trim_end_matches(':');

    // Case-insensitive match against known section headers
    let lower = header.to_lowercase();
    SECTION_HEADERS.iter().any(|h| lower == *h)
}

/// Validate subcommand name: starts with alpha, contains only [a-zA-Z0-9_-].
fn is_valid_subcommand_name(name: &str) -> bool {
    let clean = name.trim_end_matches(':');
    if clean.is_empty() {
        return false;
    }
    let mut chars = clean.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aws_style_help() {
        let help = r#"
usage: aws [options] <command> <subcommand> [parameters]

Available Commands:
  ec2                    Amazon Elastic Compute Cloud
  s3                     Amazon Simple Storage Service
  iam                    Identity and Access Management
  lambda                 AWS Lambda
  help                   Show help

To see help text, you can run:
  aws help
"#;
        let subs = parse_subcommands(help);
        let names: Vec<&str> = subs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"ec2"));
        assert!(names.contains(&"s3"));
        assert!(names.contains(&"iam"));
        assert!(names.contains(&"lambda"));
        assert!(!names.contains(&"help"), "help should be filtered");
    }

    #[test]
    fn test_kubectl_style_help() {
        let help = r#"
kubectl controls the Kubernetes cluster manager.

Commands:
  get          Display one or many resources
  describe     Show details of a specific resource
  logs         Print the logs for a container
  exec         Execute a command in a container
  apply        Apply a configuration to a resource
  delete       Delete resources
  version      Print the client and server version information

Usage:
  kubectl [flags] [options]
"#;
        let subs = parse_subcommands(help);
        let names: Vec<&str> = subs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"get"));
        assert!(names.contains(&"describe"));
        assert!(names.contains(&"logs"));
        assert!(names.contains(&"exec"));
        assert!(names.contains(&"apply"));
        assert!(names.contains(&"delete"));
        assert!(!names.contains(&"version"), "version should be filtered");
    }

    #[test]
    fn test_gh_colon_stripping() {
        let help = r#"
Available Commands:
  repo:        Manage repositories
  issue:       Manage issues
  pr:          Manage pull requests
  auth:        Login, logout, and refresh authentication
"#;
        let subs = parse_subcommands(help);
        let names: Vec<&str> = subs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"repo"));
        assert!(names.contains(&"issue"));
        assert!(names.contains(&"pr"));
        assert!(names.contains(&"auth"));
    }

    #[test]
    fn test_gcloud_allcaps_headers() {
        let help = r#"
GROUPS:
  compute          Read and manipulate Compute Engine resources
  iam              Manage IAM service accounts
  storage          Create and manage Cloud Storage

COMMANDS:
  init             Initialize or reinitialize gcloud
  info             Display information about the current gcloud environment
"#;
        let subs = parse_subcommands(help);
        let names: Vec<&str> = subs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"compute"));
        assert!(names.contains(&"iam"));
        assert!(names.contains(&"storage"));
        assert!(names.contains(&"init"));
        assert!(names.contains(&"info"));
    }

    #[test]
    fn test_empty_input() {
        assert!(parse_subcommands("").is_empty());
    }

    #[test]
    fn test_no_section_header() {
        let help = "This is just a description with no commands section.\nfoo bar baz\n";
        assert!(parse_subcommands(help).is_empty());
    }

    #[test]
    fn test_flag_exclusion() {
        let help = r#"
Commands:
  list              List all items
  --verbose         Enable verbose output
  -h                Show help
  get               Get a specific item
"#;
        let subs = parse_subcommands(help);
        let names: Vec<&str> = subs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"list"));
        assert!(names.contains(&"get"));
        assert!(!names.iter().any(|n| n.starts_with('-')), "no flags");
    }

    #[test]
    fn test_noise_filtering() {
        let help = r#"
Commands:
  deploy             Deploy the app
  help               Show help
  version            Show version
  completion         Generate shell completions
  completions        Generate shell completions
  status             Show status
"#;
        let subs = parse_subcommands(help);
        let names: Vec<&str> = subs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"deploy"));
        assert!(names.contains(&"status"));
        assert!(!names.contains(&"help"));
        assert!(!names.contains(&"version"));
        assert!(!names.contains(&"completion"));
        assert!(!names.contains(&"completions"));
    }

    #[test]
    fn test_description_extraction() {
        let help = r#"
Commands:
  list      List all resources
  get       Get a specific resource
"#;
        let subs = parse_subcommands(help);
        let list = subs.iter().find(|s| s.name == "list").unwrap();
        assert_eq!(list.description.as_deref(), Some("List all resources"));
    }

    #[test]
    fn test_deduplication() {
        let help = r#"
Commands:
  list      List items

Other Commands:
  list      List items again
  create    Create items
"#;
        let subs = parse_subcommands(help);
        let list_count = subs.iter().filter(|s| s.name == "list").count();
        assert_eq!(list_count, 1, "list should appear only once");
    }

    #[test]
    fn test_management_commands_section() {
        let help = r#"
Management Commands:
  container   Manage containers
  image       Manage images
  network     Manage networks
"#;
        let subs = parse_subcommands(help);
        let names: Vec<&str> = subs.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"container"));
        assert!(names.contains(&"image"));
        assert!(names.contains(&"network"));
    }
}
