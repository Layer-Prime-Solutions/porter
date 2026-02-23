//! Built-in CLI profiles for Porter.
//!
//! Each profile provides compile-time read-only subcommand lists, default
//! inject flags, and read-only classification logic for a well-known CLI tool.
//! Profiles enable subcommand expansion (one MCP tool per read-only subcommand)
//! and automatic output format flag injection without manual TOML schema config.

mod ansible;
mod aws;
mod az;
mod doggo;
mod gcloud;
mod gh;
mod gitlab;
mod kubectl;
mod rg;
mod tldr;
mod whois;

pub use ansible::AnsibleProfile;
pub use aws::AwsProfile;
pub use az::AzProfile;
pub use doggo::DoggoProfile;
pub use gcloud::GcloudProfile;
pub use gh::GhProfile;
pub use gitlab::GitlabProfile;
pub use kubectl::KubectlProfile;
pub use rg::RgProfile;
pub use tldr::TldrProfile;
pub use whois::WhoisProfile;

/// A built-in CLI profile providing compile-time read-only subcommand lists
/// and default inject flags for a well-known CLI tool.
///
/// Implementations provide:
/// - `name()` — the profile identifier (e.g., "aws")
/// - `default_inject_flags()` — flags injected on every invocation (e.g., `["--output", "json"]`)
/// - `is_read_only(args)` — checks if the given argument slice is a read-only operation
/// - `read_only_subcommands()` — all known read-only subcommand paths (compile-time lists)
/// - `expand_by_default()` — whether to create one MCP tool per read-only subcommand (default: true)
pub trait BuiltinProfile: Send + Sync {
    /// The profile identifier used in config (e.g., "aws", "kubectl").
    fn name(&self) -> &'static str;

    /// Flags injected on every invocation (e.g., `["--output", "json"]` for aws).
    /// Used when user config does not override inject_flags.
    fn default_inject_flags(&self) -> Vec<String>;

    /// Returns true if the given args represent a read-only operation.
    ///
    /// `args` is the full argument slice (subcommand path + flags), e.g.,
    /// `["ec2", "describe-instances", "--instance-ids", "i-123"]`.
    fn is_read_only(&self, args: &[&str]) -> bool;

    /// All known read-only subcommand paths as compile-time lists.
    ///
    /// Each entry is a Vec representing the subcommand path, e.g.:
    /// - `["ec2", "describe-instances"]` for `aws ec2 describe-instances`
    /// - `["get"]` for `kubectl get`
    ///
    /// Used for subcommand expansion to create one MCP tool per read-only subcommand.
    fn read_only_subcommands(&self) -> Vec<Vec<String>>;

    /// Whether to create one MCP tool per read-only subcommand (default: true).
    ///
    /// Set to false for single-purpose tools (rg, tldr, whois) that don't
    /// have meaningful subcommands to expand.
    fn expand_by_default(&self) -> bool {
        true
    }
}

/// Resolve a built-in profile by name.
///
/// Returns `Some(Box<dyn BuiltinProfile>)` for known profiles,
/// or `None` if the name is not a recognized built-in.
///
/// Supported profiles: "aws", "gcloud", "kubectl", "gh", "az",
/// "ansible", "gitlab", "doggo", "rg", "tldr", "whois".
pub fn get_profile(name: &str) -> Option<Box<dyn BuiltinProfile>> {
    match name {
        "aws" => Some(Box::new(AwsProfile)),
        "gcloud" => Some(Box::new(GcloudProfile)),
        "kubectl" => Some(Box::new(KubectlProfile)),
        "gh" => Some(Box::new(GhProfile)),
        "az" => Some(Box::new(AzProfile)),
        "ansible" => Some(Box::new(AnsibleProfile)),
        "gitlab" => Some(Box::new(GitlabProfile)),
        "doggo" => Some(Box::new(DoggoProfile)),
        "rg" => Some(Box::new(RgProfile)),
        "tldr" => Some(Box::new(TldrProfile)),
        "whois" => Some(Box::new(WhoisProfile)),
        _ => None,
    }
}

/// Returns a sorted list of all built-in profile names.
pub fn available_profiles() -> Vec<&'static str> {
    let mut names = vec![
        "ansible", "aws", "az", "doggo", "gcloud", "gh", "gitlab", "kubectl", "rg", "tldr", "whois",
    ];
    names.sort_unstable();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_profile_aws_returns_some() {
        let profile = get_profile("aws");
        assert!(profile.is_some());
        let profile = profile.unwrap();
        assert_eq!(profile.name(), "aws");
    }

    #[test]
    fn test_get_profile_unknown_returns_none() {
        assert!(get_profile("unknown-tool").is_none());
        assert!(get_profile("").is_none());
        assert!(get_profile("AWS").is_none()); // case-sensitive
    }

    #[test]
    fn test_available_profiles_returns_all_11() {
        let profiles = available_profiles();
        assert_eq!(profiles.len(), 11);
        // Verify sorted
        let mut sorted = profiles.clone();
        sorted.sort_unstable();
        assert_eq!(profiles, sorted);
    }

    #[test]
    fn test_available_profiles_contains_all_known() {
        let profiles = available_profiles();
        for name in &[
            "aws", "gcloud", "kubectl", "gh", "az", "ansible", "gitlab", "doggo", "rg", "tldr",
            "whois",
        ] {
            assert!(
                profiles.contains(name),
                "available_profiles() missing: {}",
                name
            );
        }
    }

    #[test]
    fn test_aws_is_read_only_describe_instances() {
        let profile = AwsProfile;
        assert!(
            profile.is_read_only(&["ec2", "describe-instances"]),
            "ec2 describe-instances must be read-only"
        );
    }

    #[test]
    fn test_aws_is_read_only_terminate_instances_is_write() {
        let profile = AwsProfile;
        assert!(
            !profile.is_read_only(&["ec2", "terminate-instances"]),
            "ec2 terminate-instances must be write"
        );
    }

    #[test]
    fn test_aws_is_read_only_s3_ls() {
        let profile = AwsProfile;
        assert!(
            profile.is_read_only(&["s3", "ls"]),
            "s3 ls must be read-only"
        );
    }

    #[test]
    fn test_kubectl_is_read_only_get() {
        let profile = KubectlProfile;
        assert!(
            profile.is_read_only(&["get"]),
            "kubectl get must be read-only"
        );
    }

    #[test]
    fn test_kubectl_is_read_only_delete_is_write() {
        let profile = KubectlProfile;
        assert!(
            !profile.is_read_only(&["delete"]),
            "kubectl delete must be write"
        );
    }

    #[test]
    fn test_doggo_is_read_only_anything() {
        let profile = DoggoProfile;
        assert!(
            profile.is_read_only(&["anything"]),
            "doggo is always read-only"
        );
        assert!(
            profile.is_read_only(&["example.com", "A"]),
            "doggo is always read-only"
        );
    }

    #[test]
    fn test_rg_expand_by_default_false() {
        let profile = RgProfile;
        assert!(
            !profile.expand_by_default(),
            "rg should not expand by default"
        );
    }

    #[test]
    fn test_tldr_expand_by_default_false() {
        let profile = TldrProfile;
        assert!(
            !profile.expand_by_default(),
            "tldr should not expand by default"
        );
    }

    #[test]
    fn test_whois_expand_by_default_false() {
        let profile = WhoisProfile;
        assert!(
            !profile.expand_by_default(),
            "whois should not expand by default"
        );
    }

    #[test]
    fn test_all_profiles_resolvable() {
        // Every name in available_profiles() must resolve to Some
        for name in available_profiles() {
            assert!(
                get_profile(name).is_some(),
                "get_profile({}) returned None but is in available_profiles()",
                name
            );
        }
    }

    #[test]
    fn test_aws_inject_flags() {
        let profile = AwsProfile;
        let flags = profile.default_inject_flags();
        assert!(
            flags.contains(&"--output".to_string()),
            "aws should inject --output"
        );
        assert!(
            flags.contains(&"json".to_string()),
            "aws should inject json output format"
        );
    }

    #[test]
    fn test_kubectl_inject_flags() {
        let profile = KubectlProfile;
        let flags = profile.default_inject_flags();
        assert!(
            flags.contains(&"-o".to_string()),
            "kubectl should inject -o"
        );
        assert!(
            flags.contains(&"json".to_string()),
            "kubectl should inject json"
        );
    }

    #[test]
    fn test_aws_read_only_subcommands_nonempty() {
        let profile = AwsProfile;
        let subcommands = profile.read_only_subcommands();
        assert!(
            subcommands.len() > 10,
            "aws should have many read-only subcommands"
        );
    }

    #[test]
    fn test_doggo_has_no_read_only_subcommands_list() {
        // doggo is always read-only, subcommands list is empty (no expansion needed)
        let profile = DoggoProfile;
        // expand_by_default is true for doggo — it's a single tool but all-read
        // The subcommands list can be empty since there's no meaningful expansion
        let _ = profile.read_only_subcommands(); // just ensure it doesn't panic
    }
}
