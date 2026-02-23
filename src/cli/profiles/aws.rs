//! AWS CLI built-in profile for Porter.
//!
//! Provides read-only subcommand classification and JSON output flag injection
//! for the `aws` CLI. Read-only determination uses a HashSet of "service action"
//! pairs for O(1) lookup.

use std::collections::HashSet;
use std::sync::OnceLock;

use super::BuiltinProfile;

/// Built-in profile for the AWS CLI (`aws`).
pub struct AwsProfile;

/// Static set of read-only "service action" pairs for O(1) lookup.
///
/// Format: "service action" (space-separated), e.g., "ec2 describe-instances".
/// Populated once on first use via OnceLock.
fn read_only_set() -> &'static HashSet<String> {
    static SET: OnceLock<HashSet<String>> = OnceLock::new();
    SET.get_or_init(|| {
        let mut s = HashSet::new();

        // EC2
        for action in &[
            "describe-instances",
            "describe-instance-types",
            "describe-instance-status",
            "describe-vpcs",
            "describe-subnets",
            "describe-security-groups",
            "describe-key-pairs",
            "describe-images",
            "describe-snapshots",
            "describe-volumes",
            "describe-addresses",
            "describe-availability-zones",
            "describe-regions",
            "describe-route-tables",
            "describe-internet-gateways",
            "describe-nat-gateways",
            "describe-network-interfaces",
            "describe-network-acls",
            "describe-load-balancers",
            "describe-auto-scaling-groups",
            "describe-launch-templates",
            "describe-tags",
            "describe-instance-attribute",
            "describe-spot-instance-requests",
            "describe-reserved-instances",
            "describe-dhcp-options",
            "describe-vpc-endpoints",
            "describe-vpc-peering-connections",
            "describe-transit-gateways",
            "describe-flow-logs",
        ] {
            s.insert(format!("ec2 {}", action));
        }

        // S3
        for action in &["ls", "cp --dryrun"] {
            s.insert(format!("s3 {}", action));
        }

        // S3API
        for action in &[
            "list-buckets",
            "list-objects",
            "list-objects-v2",
            "list-object-versions",
            "list-multipart-uploads",
            "get-bucket-acl",
            "get-bucket-cors",
            "get-bucket-encryption",
            "get-bucket-lifecycle",
            "get-bucket-location",
            "get-bucket-logging",
            "get-bucket-notification-configuration",
            "get-bucket-policy",
            "get-bucket-replication",
            "get-bucket-tagging",
            "get-bucket-versioning",
            "get-bucket-website",
            "get-object-acl",
            "get-object-tagging",
            "head-bucket",
            "head-object",
        ] {
            s.insert(format!("s3api {}", action));
        }

        // IAM
        for action in &[
            "list-users",
            "list-groups",
            "list-roles",
            "list-policies",
            "list-attached-user-policies",
            "list-attached-group-policies",
            "list-attached-role-policies",
            "list-user-policies",
            "list-group-policies",
            "list-role-policies",
            "list-groups-for-user",
            "list-access-keys",
            "list-mfa-devices",
            "list-virtual-mfa-devices",
            "list-instance-profiles",
            "list-account-aliases",
            "get-user",
            "get-group",
            "get-role",
            "get-policy",
            "get-policy-version",
            "get-account-summary",
            "get-account-password-policy",
            "get-account-authorization-details",
        ] {
            s.insert(format!("iam {}", action));
        }

        // STS
        for action in &[
            "get-caller-identity",
            "get-session-token",
            "decode-authorization-message",
        ] {
            s.insert(format!("sts {}", action));
        }

        // RDS
        for action in &[
            "describe-db-instances",
            "describe-db-clusters",
            "describe-db-snapshots",
            "describe-db-cluster-snapshots",
            "describe-db-subnet-groups",
            "describe-db-parameter-groups",
            "describe-db-security-groups",
            "describe-db-engine-versions",
            "describe-db-log-files",
            "describe-events",
            "describe-option-groups",
        ] {
            s.insert(format!("rds {}", action));
        }

        // Lambda
        for action in &[
            "list-functions",
            "list-aliases",
            "list-event-source-mappings",
            "list-layers",
            "list-layer-versions",
            "list-tags",
            "list-versions-by-function",
            "get-function",
            "get-function-configuration",
            "get-function-event-invoke-config",
            "get-policy",
            "get-account-settings",
        ] {
            s.insert(format!("lambda {}", action));
        }

        // CloudFormation
        for action in &[
            "list-stacks",
            "list-stack-resources",
            "list-stack-sets",
            "list-exports",
            "describe-stacks",
            "describe-stack-events",
            "describe-stack-resource",
            "describe-stack-resources",
            "describe-stack-set",
            "get-template",
            "get-template-summary",
            "validate-template",
        ] {
            s.insert(format!("cloudformation {}", action));
        }

        // Route53
        for action in &[
            "list-hosted-zones",
            "list-hosted-zones-by-name",
            "list-resource-record-sets",
            "list-traffic-policies",
            "list-health-checks",
            "get-hosted-zone",
            "get-health-check",
            "get-account-limit",
        ] {
            s.insert(format!("route53 {}", action));
        }

        // CloudWatch
        for action in &[
            "list-metrics",
            "list-dashboards",
            "list-alarms",
            "list-alarms-for-metric",
            "describe-alarms",
            "describe-alarm-history",
            "get-metric-data",
            "get-metric-statistics",
            "get-metric-widget-image",
            "get-dashboard",
        ] {
            s.insert(format!("cloudwatch {}", action));
        }

        // CloudWatch Logs
        for action in &[
            "describe-log-groups",
            "describe-log-streams",
            "describe-subscription-filters",
            "describe-metric-filters",
            "filter-log-events",
            "get-log-events",
            "get-log-group-fields",
            "get-log-record",
            "get-query-results",
            "list-tags-log-group",
            "start-query",
            "stop-query",
        ] {
            s.insert(format!("logs {}", action));
        }

        // SNS
        for action in &[
            "list-topics",
            "list-subscriptions",
            "list-subscriptions-by-topic",
            "list-tags-for-resource",
            "get-topic-attributes",
            "get-subscription-attributes",
        ] {
            s.insert(format!("sns {}", action));
        }

        // SQS
        for action in &[
            "list-queues",
            "list-queue-tags",
            "get-queue-attributes",
            "get-queue-url",
        ] {
            s.insert(format!("sqs {}", action));
        }

        // DynamoDB
        for action in &[
            "list-tables",
            "list-tags-of-resource",
            "list-backups",
            "list-global-tables",
            "describe-table",
            "describe-backup",
            "describe-continuous-backups",
            "describe-global-table",
            "describe-limits",
            "describe-time-to-live",
            "scan",
            "query",
            "get-item",
            "batch-get-item",
        ] {
            s.insert(format!("dynamodb {}", action));
        }

        // ECS
        for action in &[
            "list-clusters",
            "list-services",
            "list-tasks",
            "list-task-definitions",
            "list-container-instances",
            "list-account-settings",
            "list-attributes",
            "list-tags-for-resource",
            "describe-clusters",
            "describe-services",
            "describe-tasks",
            "describe-task-definition",
            "describe-container-instances",
        ] {
            s.insert(format!("ecs {}", action));
        }

        // EKS
        for action in &[
            "list-clusters",
            "list-nodegroups",
            "list-fargate-profiles",
            "list-addons",
            "list-identity-provider-configs",
            "list-tags-for-resource",
            "list-updates",
            "describe-cluster",
            "describe-nodegroup",
            "describe-fargate-profile",
            "describe-addon",
            "describe-addon-versions",
            "describe-update",
            "describe-identity-provider-config",
        ] {
            s.insert(format!("eks {}", action));
        }

        // ElastiCache
        for action in &[
            "describe-cache-clusters",
            "describe-cache-engine-versions",
            "describe-cache-parameter-groups",
            "describe-cache-parameters",
            "describe-cache-security-groups",
            "describe-cache-subnet-groups",
            "describe-events",
            "describe-replication-groups",
            "describe-reserved-cache-nodes",
            "describe-snapshots",
            "list-tags-for-resource",
        ] {
            s.insert(format!("elasticache {}", action));
        }

        // ELB (Classic)
        for action in &[
            "describe-load-balancers",
            "describe-load-balancer-attributes",
            "describe-load-balancer-policies",
            "describe-instance-health",
            "describe-tags",
        ] {
            s.insert(format!("elb {}", action));
        }

        // ELBv2 (ALB/NLB)
        for action in &[
            "describe-load-balancers",
            "describe-load-balancer-attributes",
            "describe-listeners",
            "describe-listener-certificates",
            "describe-rules",
            "describe-target-groups",
            "describe-target-group-attributes",
            "describe-target-health",
            "describe-tags",
            "describe-ssl-policies",
            "describe-account-limits",
        ] {
            s.insert(format!("elbv2 {}", action));
        }

        // ECR
        for action in &[
            "describe-repositories",
            "describe-images",
            "describe-image-scan-findings",
            "list-images",
            "list-tags-for-resource",
            "get-authorization-token",
            "get-repository-policy",
            "get-lifecycle-policy",
            "get-registry-scanning-configuration",
        ] {
            s.insert(format!("ecr {}", action));
        }

        // SecretsManager
        for action in &[
            "list-secrets",
            "list-secret-version-ids",
            "describe-secret",
            "get-secret-value",
            "get-resource-policy",
        ] {
            s.insert(format!("secretsmanager {}", action));
        }

        // SSM
        for action in &[
            "list-associations",
            "list-commands",
            "list-command-invocations",
            "list-documents",
            "list-inventory-entries",
            "list-ops-items",
            "list-parameters",
            "list-tags-for-resource",
            "describe-instance-information",
            "describe-parameters",
            "describe-document",
            "get-parameter",
            "get-parameters",
            "get-parameters-by-path",
            "get-parameter-history",
        ] {
            s.insert(format!("ssm {}", action));
        }

        s
    })
}

impl BuiltinProfile for AwsProfile {
    fn name(&self) -> &'static str {
        "aws"
    }

    fn default_inject_flags(&self) -> Vec<String> {
        vec!["--output".to_string(), "json".to_string()]
    }

    fn is_read_only(&self, args: &[&str]) -> bool {
        // args[0] = service (e.g., "ec2"), args[1] = action (e.g., "describe-instances")
        if args.len() < 2 {
            return false;
        }
        let key = format!("{} {}", args[0], args[1]);
        read_only_set().contains(&key)
    }

    fn read_only_subcommands(&self) -> Vec<Vec<String>> {
        read_only_set()
            .iter()
            .map(|entry| entry.splitn(2, ' ').map(String::from).collect::<Vec<_>>())
            .collect()
    }
}
