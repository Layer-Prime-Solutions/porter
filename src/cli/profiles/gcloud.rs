//! Google Cloud CLI (`gcloud`) built-in profile for Porter.

use std::collections::HashSet;
use std::sync::OnceLock;

use super::BuiltinProfile;

/// Built-in profile for the Google Cloud CLI (`gcloud`).
pub struct GcloudProfile;

/// Static set of read-only "group action" pairs for gcloud.
///
/// gcloud subcommands follow the pattern: `gcloud <group> <resource> <action>`
/// We store the first two tokens as the key (e.g., "compute instances list").
fn read_only_set() -> &'static HashSet<String> {
    static SET: OnceLock<HashSet<String>> = OnceLock::new();
    SET.get_or_init(|| {
        let mut s = HashSet::new();

        // compute instances
        for action in &[
            "list",
            "describe",
            "get-serial-port-output",
            "get-shielded-identity",
        ] {
            s.insert(format!("compute instances {}", action));
        }
        // compute disks
        for action in &["list", "describe"] {
            s.insert(format!("compute disks {}", action));
        }
        // compute networks
        for action in &["list", "describe"] {
            s.insert(format!("compute networks {}", action));
        }
        // compute firewall-rules
        for action in &["list", "describe"] {
            s.insert(format!("compute firewall-rules {}", action));
        }
        // compute backend-services
        for action in &["list", "describe"] {
            s.insert(format!("compute backend-services {}", action));
        }
        // compute forwarding-rules
        for action in &["list", "describe"] {
            s.insert(format!("compute forwarding-rules {}", action));
        }
        // compute target-http-proxies
        for action in &["list", "describe"] {
            s.insert(format!("compute target-http-proxies {}", action));
        }
        // compute url-maps
        for action in &["list", "describe"] {
            s.insert(format!("compute url-maps {}", action));
        }
        // compute health-checks
        for action in &["list", "describe"] {
            s.insert(format!("compute health-checks {}", action));
        }
        // compute regions / zones
        for action in &["list", "describe"] {
            s.insert(format!("compute regions {}", action));
            s.insert(format!("compute zones {}", action));
        }
        // compute addresses
        for action in &["list", "describe"] {
            s.insert(format!("compute addresses {}", action));
        }
        // compute routers
        for action in &["list", "describe"] {
            s.insert(format!("compute routers {}", action));
        }
        // compute images
        for action in &["list", "describe"] {
            s.insert(format!("compute images {}", action));
        }

        // iam roles / service-accounts / policies
        for action in &["list", "describe"] {
            s.insert(format!("iam roles {}", action));
            s.insert(format!("iam service-accounts {}", action));
        }
        s.insert("iam service-accounts get-iam-policy".to_string());
        s.insert("iam roles get".to_string());

        // projects
        for action in &["list", "describe", "get-iam-policy"] {
            s.insert(format!("projects {}", action));
        }

        // storage
        s.insert("storage ls".to_string());
        s.insert("storage buckets list".to_string());
        s.insert("storage buckets describe".to_string());
        s.insert("storage objects list".to_string());

        // container (GKE)
        for action in &["list", "describe"] {
            s.insert(format!("container clusters {}", action));
            s.insert(format!("container node-pools {}", action));
        }

        // dns
        for action in &["list", "describe"] {
            s.insert(format!("dns managed-zones {}", action));
            s.insert(format!("dns record-sets {}", action));
        }

        // sql
        for action in &["list", "describe"] {
            s.insert(format!("sql instances {}", action));
            s.insert(format!("sql databases {}", action));
            s.insert(format!("sql users {}", action));
            s.insert(format!("sql backups {}", action));
        }

        // pubsub
        for action in &["list", "describe"] {
            s.insert(format!("pubsub topics {}", action));
            s.insert(format!("pubsub subscriptions {}", action));
        }

        // functions
        for action in &["list", "describe"] {
            s.insert(format!("functions {}", action));
        }

        // run
        for action in &["list", "describe"] {
            s.insert(format!("run services {}", action));
            s.insert(format!("run revisions {}", action));
        }

        // logging
        for action in &["list", "read"] {
            s.insert(format!("logging logs {}", action));
        }
        s.insert("logging sinks list".to_string());

        s
    })
}

impl BuiltinProfile for GcloudProfile {
    fn name(&self) -> &'static str {
        "gcloud"
    }

    fn default_inject_flags(&self) -> Vec<String> {
        vec!["--format=json".to_string()]
    }

    fn is_read_only(&self, args: &[&str]) -> bool {
        // gcloud pattern: args[0]=group, args[1]=resource-or-action, args[2]=action (sometimes)
        // We check up to 3 tokens: "group resource action" and "group action"
        if args.len() < 2 {
            return false;
        }
        // Try 3-token key first (e.g., "compute instances list")
        if args.len() >= 3 {
            let key3 = format!("{} {} {}", args[0], args[1], args[2]);
            if read_only_set().contains(&key3) {
                return true;
            }
        }
        // Try 2-token key (e.g., "projects list", "storage ls")
        let key2 = format!("{} {}", args[0], args[1]);
        read_only_set().contains(&key2)
    }

    fn read_only_subcommands(&self) -> Vec<Vec<String>> {
        read_only_set()
            .iter()
            .map(|entry| entry.split(' ').map(String::from).collect::<Vec<_>>())
            .collect()
    }
}
