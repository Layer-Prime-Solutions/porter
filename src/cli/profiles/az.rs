//! Azure CLI (`az`) built-in profile for Porter.

use std::collections::HashSet;
use std::sync::OnceLock;

use super::BuiltinProfile;

/// Built-in profile for the Azure CLI (`az`).
pub struct AzProfile;

/// Static set of read-only "group action" pairs for az.
fn read_only_set() -> &'static HashSet<String> {
    static SET: OnceLock<HashSet<String>> = OnceLock::new();
    SET.get_or_init(|| {
        let mut s = HashSet::new();

        // account
        for action in &["list", "show", "list-locations", "get-access-token"] {
            s.insert(format!("account {}", action));
        }

        // group (resource groups)
        for action in &["list", "show"] {
            s.insert(format!("group {}", action));
        }

        // vm
        for action in &["list", "show", "get-instance-view", "list-sizes"] {
            s.insert(format!("vm {}", action));
        }

        // vmss
        for action in &["list", "show"] {
            s.insert(format!("vmss {}", action));
        }

        // network
        for action in &["list", "show"] {
            s.insert(format!("network vnet {}", action));
            s.insert(format!("network nsg {}", action));
            s.insert(format!("network nic {}", action));
            s.insert(format!("network public-ip {}", action));
            s.insert(format!("network lb {}", action));
            s.insert(format!("network application-gateway {}", action));
            s.insert(format!("network route-table {}", action));
            s.insert(format!("network dns zone {}", action));
            s.insert(format!("network dns record-set {}", action));
        }

        // storage
        for action in &["list", "show"] {
            s.insert(format!("storage account {}", action));
        }
        s.insert("storage blob list".to_string());
        s.insert("storage container list".to_string());

        // aks
        for action in &["list", "show", "get-credentials"] {
            s.insert(format!("aks {}", action));
        }

        // acr (container registry)
        for action in &["list", "show"] {
            s.insert(format!("acr {}", action));
        }
        s.insert("acr repository list".to_string());
        s.insert("acr repository show-tags".to_string());

        // webapp / functionapp
        for action in &["list", "show"] {
            s.insert(format!("webapp {}", action));
            s.insert(format!("functionapp {}", action));
        }

        // ad (Azure AD / Entra ID)
        for action in &["list", "show"] {
            s.insert(format!("ad user {}", action));
            s.insert(format!("ad group {}", action));
            s.insert(format!("ad sp {}", action));
            s.insert(format!("ad app {}", action));
        }

        // role
        s.insert("role definition list".to_string());
        s.insert("role assignment list".to_string());

        // keyvault
        for action in &["list", "show"] {
            s.insert(format!("keyvault {}", action));
        }
        s.insert("keyvault secret list".to_string());
        s.insert("keyvault key list".to_string());
        s.insert("keyvault certificate list".to_string());

        // monitor
        s.insert("monitor metrics list".to_string());
        s.insert("monitor activity-log list".to_string());
        s.insert("monitor log-analytics workspace list".to_string());

        // resource
        for action in &["list", "show"] {
            s.insert(format!("resource {}", action));
        }

        // sql
        for action in &["list", "show"] {
            s.insert(format!("sql server {}", action));
            s.insert(format!("sql db {}", action));
        }

        // cosmosdb
        for action in &["list", "show"] {
            s.insert(format!("cosmosdb {}", action));
        }

        s
    })
}

impl BuiltinProfile for AzProfile {
    fn name(&self) -> &'static str {
        "az"
    }

    fn default_inject_flags(&self) -> Vec<String> {
        vec!["--output".to_string(), "json".to_string()]
    }

    fn is_read_only(&self, args: &[&str]) -> bool {
        if args.len() < 2 {
            return false;
        }
        // az uses multi-level groups: "vm list", "network vnet list", "ad user list"
        // Try 3-token key first, then 2-token
        if args.len() >= 3 {
            let key3 = format!("{} {} {}", args[0], args[1], args[2]);
            if read_only_set().contains(&key3) {
                return true;
            }
        }
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
