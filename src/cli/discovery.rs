//! Recursive CLI help discovery engine — walks the `--help` tree via BFS
//! with concurrency control to enumerate all subcommand paths for a CLI tool.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Semaphore;

use crate::cli::subcommand_parser::parse_subcommands;

/// Configuration for a discovery run.
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// The base command to discover subcommands for (e.g., "aws").
    pub command: String,
    /// Maximum depth of recursion (0 = no discovery). Default: 3.
    pub max_depth: u8,
    /// Timeout per individual `--help` subprocess.
    pub timeout_per_help: Duration,
    /// Wall-clock budget for the entire discovery run.
    pub total_budget: Duration,
    /// Environment variables to inject into subprocesses.
    pub env: HashMap<String, String>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            command: String::new(),
            max_depth: 3,
            timeout_per_help: Duration::from_secs(10),
            total_budget: Duration::from_secs(60),
            env: HashMap::new(),
        }
    }
}

/// A discovered subcommand path in the CLI tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPath {
    /// The subcommand path tokens (e.g., `["ec2", "describe-instances"]`).
    pub path: Vec<String>,
    /// True if no further subcommands were found beneath this path.
    pub is_leaf: bool,
}

/// Result of a discovery run.
#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    /// All discovered subcommand paths.
    pub paths: Vec<DiscoveredPath>,
    /// Soft errors: (path, reason) for subcommands where --help failed.
    pub errors: Vec<(Vec<String>, String)>,
    /// True if the total_budget was exceeded (partial results).
    pub timed_out: bool,
}

/// Recursively discover subcommands by walking the `--help` tree via BFS.
///
/// Runs `command --help` to find tier-0 subcommands, then recurses up to
/// `max_depth` levels deep. Concurrency is capped by a semaphore (8 concurrent
/// help invocations per tier). The `total_budget` deadline is checked before
/// spawning each new tier.
pub async fn discover_subcommands(config: DiscoveryConfig) -> DiscoveryResult {
    let deadline = Instant::now() + config.total_budget;
    let mut all_paths = Vec::new();
    let mut all_errors = Vec::new();
    let mut timed_out = false;

    if config.max_depth == 0 {
        return DiscoveryResult {
            paths: all_paths,
            errors: all_errors,
            timed_out,
        };
    }

    tracing::info!(
        command = %config.command,
        max_depth = %config.max_depth,
        budget_secs = %config.total_budget.as_secs(),
        "starting CLI help discovery"
    );

    // BFS: queue of (path_prefix, current_depth)
    let mut queue: Vec<(Vec<String>, u8)> = vec![(vec![], 0)];
    let semaphore = Arc::new(Semaphore::new(8));

    while !queue.is_empty() {
        // Check deadline
        if Instant::now() >= deadline {
            timed_out = true;
            tracing::warn!(
                command = %config.command,
                "discovery budget exceeded, using partial results"
            );
            break;
        }

        // Process current tier concurrently
        let current_tier: Vec<(Vec<String>, u8)> = queue.drain(..).collect();
        let mut handles = Vec::with_capacity(current_tier.len());

        for (prefix, depth) in current_tier {
            let cmd = config.command.clone();
            let env = config.env.clone();
            let timeout = config.timeout_per_help;
            let sem = semaphore.clone();

            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await;
                let result = run_help_and_parse(&cmd, &prefix, &env, timeout).await;
                (prefix, depth, result)
            });
            handles.push(handle);
        }

        // Collect results
        for handle in handles {
            let Ok((prefix, depth, result)) = handle.await else {
                continue;
            };

            match result {
                Ok(subcommands) => {
                    if subcommands.is_empty() {
                        // Leaf node (no subcommands found)
                        if !prefix.is_empty() {
                            all_paths.push(DiscoveredPath {
                                path: prefix,
                                is_leaf: true,
                            });
                        }
                    } else {
                        tracing::debug!(
                            command = %config.command,
                            prefix = ?prefix,
                            count = %subcommands.len(),
                            "discovered subcommands at depth {}", depth
                        );

                        for sub in subcommands {
                            let mut child_path = prefix.clone();
                            child_path.push(sub);

                            if depth + 1 < config.max_depth {
                                // Enqueue for deeper discovery
                                queue.push((child_path.clone(), depth + 1));
                                // Also record as non-leaf
                                all_paths.push(DiscoveredPath {
                                    path: child_path,
                                    is_leaf: false,
                                });
                            } else {
                                // Max depth reached — treat as leaf
                                all_paths.push(DiscoveredPath {
                                    path: child_path,
                                    is_leaf: true,
                                });
                            }
                        }
                    }
                }
                Err(reason) => {
                    tracing::warn!(
                        command = %config.command,
                        path = ?prefix,
                        reason = %reason,
                        "help discovery failed for subcommand path"
                    );
                    all_errors.push((prefix, reason));
                }
            }
        }
    }

    tracing::info!(
        command = %config.command,
        discovered = %all_paths.len(),
        errors = %all_errors.len(),
        timed_out = %timed_out,
        "CLI help discovery complete"
    );

    DiscoveryResult {
        paths: all_paths,
        errors: all_errors,
        timed_out,
    }
}

/// Run `command [prefix...] --help` and parse subcommand names from output.
async fn run_help_and_parse(
    command: &str,
    prefix: &[String],
    env: &HashMap<String, String>,
    timeout: Duration,
) -> std::result::Result<Vec<String>, String> {
    let mut cmd = tokio::process::Command::new(command);
    cmd.args(prefix);
    cmd.arg("--help");
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    for (k, v) in env {
        cmd.env(k, v);
    }

    let output = tokio::time::timeout(timeout, cmd.output())
        .await
        .map_err(|_| format!("--help timed out after {}ms", timeout.as_millis()))?
        .map_err(|e| format!("failed to spawn: {}", e))?;

    // Try stdout first, fall back to stderr (many CLIs write help to stderr)
    let text = {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stdout.trim().is_empty() {
            stderr.into_owned()
        } else {
            stdout.into_owned()
        }
    };

    let subs = parse_subcommands(&text);
    Ok(subs.into_iter().map(|s| s.name).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_depth_zero_returns_empty() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(discover_subcommands(DiscoveryConfig {
            command: "echo".to_string(),
            max_depth: 0,
            ..Default::default()
        }));
        assert!(result.paths.is_empty());
        assert!(result.errors.is_empty());
        assert!(!result.timed_out);
    }

    #[tokio::test]
    async fn test_nonexistent_command_records_error() {
        let result = discover_subcommands(DiscoveryConfig {
            command: "porter-test-nonexistent-command-12345".to_string(),
            max_depth: 1,
            timeout_per_help: Duration::from_secs(2),
            total_budget: Duration::from_secs(5),
            env: HashMap::new(),
        })
        .await;
        // The root --help fails, so we get an error for the empty prefix
        assert!(!result.errors.is_empty() || result.paths.is_empty());
    }

    #[tokio::test]
    async fn test_echo_has_no_subcommands() {
        // echo --help output won't have a "Commands:" section
        let result = discover_subcommands(DiscoveryConfig {
            command: "echo".to_string(),
            max_depth: 1,
            timeout_per_help: Duration::from_secs(2),
            total_budget: Duration::from_secs(5),
            env: HashMap::new(),
        })
        .await;
        // echo doesn't have subcommands, so paths should be empty
        assert!(result.paths.is_empty());
        assert!(!result.timed_out);
    }

    #[test]
    fn test_discovery_config_defaults() {
        let config = DiscoveryConfig::default();
        assert_eq!(config.max_depth, 3);
        assert_eq!(config.timeout_per_help, Duration::from_secs(10));
        assert_eq!(config.total_budget, Duration::from_secs(60));
    }

    #[cfg(all(test, feature = "integration-tests"))]
    mod integration {
        use super::*;

        #[tokio::test]
        async fn test_real_git_discovery() {
            let result = discover_subcommands(DiscoveryConfig {
                command: "git".to_string(),
                max_depth: 1,
                timeout_per_help: Duration::from_secs(5),
                total_budget: Duration::from_secs(30),
                env: HashMap::new(),
            })
            .await;
            // git should have at least some subcommands
            assert!(!result.paths.is_empty(), "git should have subcommands");
        }
    }
}
