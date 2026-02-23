//! CLI harness — spawns CLI commands with timeout-kill, captures output, and
//! enforces access control before dispatch.
//!
//! `CliHandle` is the runtime handle for a CLI tool registered in PorterRegistry.
//! `CliHarness` is a struct namespace for the spawn logic.
//! `spawn_cli_server` is the entry point called by PorterRegistry.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use tokio::sync::{RwLock, Semaphore};

use crate::cli::access_guard::AccessGuard;
use crate::cli::discovery::{discover_subcommands, DiscoveryConfig};
use crate::cli::help_parser::parse_help_output;
use crate::cli::profiles;
use crate::cli::read_only_heuristic;
use crate::config::{resolve_env_vars, CliServerConfig};
use crate::error::PorterError;
use crate::namespace::namespace_tool;
use crate::server::health::HealthState;

/// External-facing handle for a registered CLI tool.
///
/// Parallel to `ServerHandle` for MCP servers. Carries the tool definitions,
/// access guard, and execution parameters for the CLI tool.
pub struct CliHandle {
    pub slug: String,
    pub tools: Arc<RwLock<Vec<Tool>>>,
    pub guard: Arc<AccessGuard>,
    pub command: String,
    pub inject_flags: Vec<String>,
    /// Resolved env vars (values already extracted from `$ENV_VAR` references)
    pub env: HashMap<String, String>,
    pub timeout: Duration,
    /// Whether subcommand expansion is active for this handle.
    ///
    /// When true, individual tool names encode a subcommand path (e.g., `ec2_describe-instances`)
    /// which is prepended to args before execution.
    pub expanded: bool,
    /// True while background discovery is still running.
    pub discovery_in_progress: Arc<AtomicBool>,
}

impl CliHandle {
    /// Call a tool by its (un-namespaced) name with the given parameters.
    ///
    /// # Access control
    /// Extracts subcommand args from `params.arguments` and calls
    /// `AccessGuard::check` before spawning the process. Denied calls return
    /// `PorterError::AccessDenied` without spawning.
    ///
    /// # Subcommand expansion
    /// When `expanded = true`, the tool name encodes a subcommand path using
    /// underscore separators (e.g., `ec2_describe-instances`). These are
    /// split on `_` and prepended to the user-provided args before execution.
    ///
    /// # Execution
    /// Spawns `tokio::process::Command` with structured args (never shell),
    /// injects `inject_flags`, and applies env overrides. Uses `tokio::select!`
    /// to race `child.wait_with_output()` against a timeout, calling
    /// `child.kill()` on timeout (Pitfall 5: kills the process, not just the future).
    ///
    /// # Output
    /// Tries `serde_json::from_str` on stdout first — if valid JSON, returns
    /// `Content::json()`. Otherwise returns `Content::text()`. Non-zero exit
    /// codes with non-empty stderr set `is_error = true`.
    pub async fn call_tool(&self, params: CallToolRequestParams) -> crate::Result<CallToolResult> {
        let start = Instant::now();

        // Extract subcommand args from JSON arguments map
        // Convention: positional args in "args" array, or individual flag values as keys
        let user_args = extract_args_from_params(&params);

        // If expanded, decode the subcommand path from the tool name and prepend to args.
        // Tool name format (un-namespaced): "ec2_describe-instances" → ["ec2", "describe-instances"]
        let args_vec: Vec<String> = if self.expanded {
            // Strip the "slug__" prefix to get the subcommand-encoded portion
            let tool_name = params.name.as_ref();
            let subcommand_encoded = if let Some(separator_pos) = tool_name.find("__") {
                &tool_name[separator_pos + 2..]
            } else {
                tool_name
            };
            // Split on '_' to reconstruct subcommand path tokens
            let mut full_args: Vec<String> =
                subcommand_encoded.split('_').map(String::from).collect();
            full_args.extend(user_args);
            full_args
        } else {
            user_args
        };

        let args_slice: Vec<&str> = args_vec.iter().map(String::as_str).collect();

        // Access control check
        self.guard
            .check(&self.command, &args_slice)
            .map_err(|denied| PorterError::AccessDenied(self.slug.clone(), denied.to_string()))?;

        // Build command
        let mut cmd = tokio::process::Command::new(&self.command);
        cmd.args(&args_vec);
        cmd.args(&self.inject_flags);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // Inject resolved env vars
        for (k, v) in &self.env {
            cmd.env(k, v);
        }

        // Spawn child — collect stdout/stderr immediately so we can kill on timeout
        let mut child = cmd.spawn().map_err(|e| {
            PorterError::InitializationFailed(
                self.slug.clone(),
                format!("failed to spawn '{}': {}", self.command, e),
            )
        })?;

        // Take pipes before moving child into wait_with_output
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();

        // Drain stdout and stderr concurrently while waiting for the child to exit.
        // We use wait() (which takes &mut self) so child is still accessible for kill().
        let slug = self.slug.clone();
        let output = tokio::select! {
            result = async {
                use tokio::io::AsyncReadExt;
                let mut stdout_bytes = Vec::new();
                let mut stderr_bytes = Vec::new();
                if let Some(mut out) = stdout_pipe {
                    let _ = out.read_to_end(&mut stdout_bytes).await;
                }
                if let Some(mut err) = stderr_pipe {
                    let _ = err.read_to_end(&mut stderr_bytes).await;
                }
                let status = child.wait().await.map_err(|e| PorterError::Transport(
                    slug.clone(),
                    format!("process wait error: {}", e),
                ))?;
                Ok::<std::process::Output, PorterError>(std::process::Output {
                    status,
                    stdout: stdout_bytes,
                    stderr: stderr_bytes,
                })
            } => {
                result.map_err(|e| PorterError::Transport(
                    self.slug.clone(),
                    format!("process I/O error: {}", e),
                ))?
            }
            _ = tokio::time::sleep(self.timeout) => {
                // Timeout: kill the child process (not just cancel the future)
                let _ = child.kill().await;
                return Err(PorterError::CallTimeout(self.slug.clone()));
            }
        };

        let elapsed = start.elapsed().as_millis();
        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        tracing::info!(
            command = %self.command,
            args = ?args_slice,
            exit_code = %exit_code,
            duration_ms = %elapsed,
            "CLI tool invocation"
        );

        if !stderr.is_empty() {
            tracing::debug!(
                slug = %self.slug,
                stderr = %stderr,
                "CLI tool stderr"
            );
        }

        // Determine is_error: non-zero exit code + non-empty stderr
        let is_error = exit_code != 0 && !stderr.is_empty();

        // Parse output: try JSON first, fall back to text
        let content = if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&stdout) {
            rmcp::model::Content::json(json_val).map_err(|e| {
                PorterError::Protocol(self.slug.clone(), format!("JSON content error: {}", e))
            })?
        } else {
            rmcp::model::Content::text(stdout)
        };

        Ok(CallToolResult {
            content: vec![content],
            is_error: Some(is_error),
            structured_content: None,
            meta: None,
        })
    }

    /// Returns a snapshot of the tools registered for this CLI handle.
    pub async fn tools(&self) -> Vec<Tool> {
        self.tools.read().await.clone()
    }

    /// CLI tools are always Healthy — they are local executables with no
    /// persistent connection to maintain.
    pub fn health(&self) -> HealthState {
        HealthState::Healthy
    }
}

/// Extract positional + flag args from CallToolRequestParams.
///
/// Convention:
/// - If `params.arguments` contains an "args" array, use those as positional args
/// - For each other key-value pair, emit `--key value` pairs (skipping null/false bools)
/// - Boolean true values emit just the flag `--key` without a value
fn extract_args_from_params(params: &CallToolRequestParams) -> Vec<String> {
    let Some(ref arguments) = params.arguments else {
        return vec![];
    };

    let mut result = Vec::new();

    // Positional args from "args" array (if present)
    if let Some(serde_json::Value::Array(positional)) = arguments.get("args") {
        for v in positional {
            if let Some(s) = v.as_str() {
                result.push(s.to_string());
            }
        }
    }

    // Remaining keys as --flag value pairs
    for (key, value) in arguments {
        if key == "args" {
            continue;
        }
        let flag = format!("--{}", key.replace('_', "-"));
        match value {
            serde_json::Value::Bool(true) => {
                result.push(flag);
            }
            serde_json::Value::Bool(false) | serde_json::Value::Null => {
                // Skip — false/null means "don't include this flag"
            }
            serde_json::Value::String(s) => {
                result.push(flag);
                result.push(s.clone());
            }
            other => {
                result.push(flag);
                result.push(other.to_string());
            }
        }
    }

    result
}

/// Namespace for CLI server lifecycle helpers (unused struct — functions are top-level).
pub struct CliHarness;

/// Internal expansion mode determined from config.
enum ExpansionMode {
    /// Single tool, no expansion.
    SingleTool,
    /// Static profile expansion (backward compat: expand_subcommands = true).
    StaticProfile,
    /// Discovery mode with the given depth.
    Discovery { depth: u8 },
}

/// Determine expansion mode from config + profile.
///
/// Evaluation order:
/// 1. expand_subcommands = Some(false) → SingleTool
/// 2. help_depth = Some(0)             → SingleTool
/// 3. help_depth = Some(n) where n > 0 → Discovery(n)
/// 4. help_depth = None + profile       → Discovery(3)
/// 5. expand_subcommands = Some(true)  → StaticProfile
/// 6. No profile, no help_depth        → SingleTool
fn determine_expansion_mode(
    config: &CliServerConfig,
    profile: &Option<Box<dyn profiles::BuiltinProfile>>,
    slug: &str,
) -> crate::Result<ExpansionMode> {
    // 1. expand_subcommands = Some(false) → single tool
    if config.expand_subcommands == Some(false) {
        return Ok(ExpansionMode::SingleTool);
    }

    // 2. help_depth = Some(0) → single tool (discovery disabled)
    if config.help_depth == Some(0) {
        return Ok(ExpansionMode::SingleTool);
    }

    // 3. help_depth = Some(n) where n > 0 → discovery mode
    if let Some(depth) = config.help_depth {
        if depth > 0 {
            return Ok(ExpansionMode::Discovery { depth });
        }
    }

    // 4. help_depth = None + profile → discovery mode with depth 3
    if config.help_depth.is_none() && profile.is_some() {
        if profile.as_ref().map(|p| p.expand_by_default()).unwrap_or(false) {
            return Ok(ExpansionMode::Discovery { depth: 3 });
        }
    }

    // 5. expand_subcommands = Some(true) → static profile expansion
    if config.expand_subcommands == Some(true) {
        if profile.is_none() {
            return Err(PorterError::InvalidConfig(
                slug.to_string(),
                "expand_subcommands = true requires a built-in profile".to_string(),
            ));
        }
        return Ok(ExpansionMode::StaticProfile);
    }

    // 6. No profile, no help_depth → single tool
    Ok(ExpansionMode::SingleTool)
}

/// Spawn a CLI tool handle from config.
///
/// Resolves env vars, determines argument schema (via `--help` parsing or
/// `schema_override`), registers the tool(s), and returns a `CliHandle`.
///
/// # Profile resolution
/// If `config.profile` is set, the built-in profile is resolved and used to:
/// - Provide default inject flags (if config.inject_flags is empty)
/// - Wire `is_read_only` into AccessGuard
/// - Determine subcommand expansion behavior
///
/// # Subcommand expansion
/// When expansion is active (`expand_subcommands = true` or profile's
/// `expand_by_default() = true`), creates one MCP tool per read-only
/// subcommand from the profile's `read_only_subcommands()`.
///
/// # Returns
/// Returns `PorterError::InvalidConfig` if:
/// - `config.profile` names an unknown built-in profile
/// - `--help` parsing fails and no `schema_override` is provided
/// - `expand_subcommands = true` without a built-in profile
pub async fn spawn_cli_server(config: CliServerConfig, slug: String) -> crate::Result<CliHandle> {
    let resolved_env = resolve_env_vars(&config.env);

    // Resolve built-in profile (if configured)
    let profile: Option<Box<dyn profiles::BuiltinProfile>> =
        if let Some(ref profile_name) = config.profile {
            match profiles::get_profile(profile_name) {
                Some(p) => Some(p),
                None => {
                    return Err(PorterError::InvalidConfig(
                        slug.clone(),
                        format!(
                            "unknown built-in profile: '{}'. Available profiles: {}",
                            profile_name,
                            profiles::available_profiles().join(", ")
                        ),
                    ));
                }
            }
        } else {
            None
        };

    // Determine inject flags: user config overrides profile default
    let inject_flags = if !config.inject_flags.is_empty() {
        config.inject_flags.clone()
    } else if let Some(ref p) = profile {
        p.default_inject_flags()
    } else {
        config.inject_flags.clone()
    };

    // Determine expansion mode:
    //
    // Evaluation order:
    // 1. expand_subcommands = Some(false) → single tool (unchanged)
    // 2. help_depth = Some(0)             → single tool (discovery disabled)
    // 3. help_depth = Some(n) where n > 0 → DISCOVERY MODE
    // 4. help_depth = None + profile       → discovery mode with depth 3 (new default)
    // 5. expand_subcommands = Some(true)  → static profile expansion (backward compat)
    // 6. No profile, no help_depth        → single tool (unchanged)
    let expansion_mode = determine_expansion_mode(&config, &profile, &slug)?;

    // Build AccessGuard: wire profile's is_read_only if available
    let guard = if profile.is_some() {
        let profile_arc: Arc<Box<dyn profiles::BuiltinProfile>> = Arc::new(
            profiles::get_profile(config.profile.as_deref().unwrap()).unwrap(),
        );
        AccessGuard::new(&config)
            .with_read_only_checker(move |args: &[&str]| profile_arc.is_read_only(args))
    } else {
        AccessGuard::new(&config)
    };

    // Generic JSON input schema for expanded subcommand tools
    let generic_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "args": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Additional arguments to pass to the command"
            }
        }
    });

    match expansion_mode {
        ExpansionMode::SingleTool => {
            // Single tool: use schema_override or --help parsing
            let schema = if let Some(override_schema) = &config.schema_override {
                override_schema.clone()
            } else {
                let timeout = Duration::from_secs(config.timeout_secs);
                let arg_schema = parse_help_output(&config.command, None, timeout)
                    .await
                    .map_err(|e| match e {
                        PorterError::HelpParseFailed(_, msg) => {
                            PorterError::InvalidConfig(
                                slug.clone(),
                                format!("--help parsing failed: {}. Provide schema_override to skip help parsing.", msg),
                            )
                        }
                        PorterError::HelpTimeout(_) => {
                            PorterError::InvalidConfig(
                                slug.clone(),
                                format!(
                                    "--help timed out after {}s. Provide schema_override to skip help parsing.",
                                    config.timeout_secs
                                ),
                            )
                        }
                        other => other,
                    })?;
                arg_schema.to_json_schema()
            };

            let tool_name = format!("{}__{}", slug, config.command);
            let input_schema = Arc::new(schema.as_object().cloned().unwrap_or_default());
            let tool = Tool {
                name: tool_name.into(),
                title: None,
                description: Some(
                    format!(
                        "[via {}] CLI tool: {} (via Porter CLI harness)",
                        slug, config.command
                    )
                    .into(),
                ),
                input_schema,
                output_schema: None,
                annotations: None,
                icons: None,
                meta: None,
            };
            let namespaced_tool = namespace_tool(&slug, tool);

            let tools = Arc::new(RwLock::new(vec![namespaced_tool]));
            let guard = Arc::new(guard);
            let timeout = Duration::from_secs(config.timeout_secs);

            Ok(CliHandle {
                slug,
                tools,
                guard,
                command: config.command,
                inject_flags,
                env: resolved_env,
                timeout,
                expanded: false,
                discovery_in_progress: Arc::new(AtomicBool::new(false)),
            })
        }

        ExpansionMode::StaticProfile => {
            // Static profile expansion (backward compat): one tool per read-only subcommand
            let subcommands = profile
                .as_ref()
                .map(|p| p.read_only_subcommands())
                .unwrap_or_default();

            let input_schema = Arc::new(generic_schema.as_object().cloned().unwrap_or_default());
            let mut expanded_tools = Vec::with_capacity(subcommands.len());
            for subcommand_path in subcommands {
                let subcmd_encoded = subcommand_path.join("_");
                let tool_name = format!("{}__{}", slug, subcmd_encoded);
                let description = format!(
                    "{} {} (read-only)",
                    config.command,
                    subcommand_path.join(" ")
                );
                let tool = Tool {
                    name: tool_name.into(),
                    title: None,
                    description: Some(format!("[via {}] {}", slug, description).into()),
                    input_schema: input_schema.clone(),
                    output_schema: None,
                    annotations: None,
                    icons: None,
                    meta: None,
                };
                expanded_tools.push(tool);
            }

            tracing::info!(
                slug = %slug,
                command = %config.command,
                tool_count = %expanded_tools.len(),
                "CLI subcommand expansion: created tools (static profile)"
            );

            let tools = Arc::new(RwLock::new(expanded_tools));
            let guard = Arc::new(guard);
            let timeout = Duration::from_secs(config.timeout_secs);

            Ok(CliHandle {
                slug,
                tools,
                guard,
                command: config.command,
                inject_flags,
                env: resolved_env,
                timeout,
                expanded: true,
                discovery_in_progress: Arc::new(AtomicBool::new(false)),
            })
        }

        ExpansionMode::Discovery { depth } => {
            // Phase A (synchronous): build initial tool list from profile's static entries
            let input_schema = Arc::new(generic_schema.as_object().cloned().unwrap_or_default());
            let initial_tools: Vec<Tool> = if let Some(ref p) = profile {
                p.read_only_subcommands()
                    .into_iter()
                    .map(|subcommand_path| {
                        let subcmd_encoded = subcommand_path.join("_");
                        let tool_name = format!("{}__{}", slug, subcmd_encoded);
                        let description = format!(
                            "{} {} (read-only)",
                            config.command,
                            subcommand_path.join(" ")
                        );
                        Tool {
                            name: tool_name.into(),
                            title: None,
                            description: Some(format!("[via {}] {}", slug, description).into()),
                            input_schema: input_schema.clone(),
                            output_schema: None,
                            annotations: None,
                            icons: None,
                            meta: None,
                        }
                    })
                    .collect()
            } else {
                vec![]
            };

            tracing::info!(
                slug = %slug,
                command = %config.command,
                initial_tools = %initial_tools.len(),
                depth = %depth,
                "CLI discovery mode: initial tools from profile, starting background discovery"
            );

            let tools = Arc::new(RwLock::new(initial_tools));
            let discovery_in_progress = Arc::new(AtomicBool::new(true));
            let guard = Arc::new(guard);
            let timeout = Duration::from_secs(config.timeout_secs);

            let handle = CliHandle {
                slug: slug.clone(),
                tools: tools.clone(),
                guard: guard.clone(),
                command: config.command.clone(),
                inject_flags,
                env: resolved_env.clone(),
                timeout,
                expanded: true,
                discovery_in_progress: discovery_in_progress.clone(),
            };

            // Phase B + C (background): discover subcommands, then enrich schemas
            let bg_slug = slug.clone();
            let bg_command = config.command.clone();
            let bg_profile_name = config.profile.clone();
            let bg_env = resolved_env;
            let bg_budget = config.discovery_budget_secs;
            let bg_timeout_secs = config.timeout_secs;
            let bg_input_schema = input_schema;
            let bg_tools = tools;
            let bg_discovery_flag = discovery_in_progress;

            tokio::spawn(async move {
                // Phase B: discover subcommands
                let discovery_config = DiscoveryConfig {
                    command: bg_command.clone(),
                    max_depth: depth,
                    timeout_per_help: Duration::from_secs(bg_timeout_secs.min(10)),
                    total_budget: Duration::from_secs(bg_budget),
                    env: bg_env.clone(),
                };

                let result = discover_subcommands(discovery_config).await;

                // Resolve profile for filtering
                let profile_for_filter: Option<Box<dyn profiles::BuiltinProfile>> =
                    bg_profile_name.as_deref().and_then(profiles::get_profile);

                // Filter discovered paths: only keep read-only ones
                let mut accepted_paths: Vec<Vec<String>> = Vec::new();
                for dp in &result.paths {
                    let path_refs: Vec<&str> = dp.path.iter().map(String::as_str).collect();
                    let is_read = if let Some(ref p) = profile_for_filter {
                        p.is_read_only(&path_refs)
                    } else {
                        read_only_heuristic::is_likely_read_only(&path_refs)
                    };
                    if is_read {
                        accepted_paths.push(dp.path.clone());
                    }
                }

                // Merge with existing static profile entries (deduplicate on path-join key)
                let mut seen_keys: HashSet<String> = HashSet::new();
                let mut merged_tools: Vec<Tool> = Vec::new();

                // Keep existing tools first (static profile entries take precedence)
                {
                    let existing = bg_tools.read().await;
                    for tool in existing.iter() {
                        let name = tool.name.as_ref().to_string();
                        seen_keys.insert(name);
                        merged_tools.push(tool.clone());
                    }
                }

                // Add discovered paths not already present
                for path in &accepted_paths {
                    let subcmd_encoded = path.join("_");
                    let tool_name = format!("{}__{}", bg_slug, subcmd_encoded);
                    if seen_keys.contains(&tool_name) {
                        continue;
                    }
                    seen_keys.insert(tool_name.clone());

                    let description = format!(
                        "{} {} (read-only, discovered)",
                        bg_command,
                        path.join(" ")
                    );
                    let tool = Tool {
                        name: tool_name.into(),
                        title: None,
                        description: Some(format!("[via {}] {}", bg_slug, description).into()),
                        input_schema: bg_input_schema.clone(),
                        output_schema: None,
                        annotations: None,
                        icons: None,
                        meta: None,
                    };
                    merged_tools.push(tool);
                }

                // Write merged tools
                {
                    let mut tools_guard = bg_tools.write().await;
                    *tools_guard = merged_tools;
                }

                let total_tools = bg_tools.read().await.len();
                tracing::info!(
                    slug = %bg_slug,
                    discovered = %accepted_paths.len(),
                    total = %total_tools,
                    "CLI discovery Phase B complete: tools merged"
                );

                // Phase C: enrich schemas for discovered leaf tools
                let schema_semaphore = Arc::new(Semaphore::new(4));
                let leaf_paths: Vec<Vec<String>> = result
                    .paths
                    .iter()
                    .filter(|dp| dp.is_leaf)
                    .filter(|dp| {
                        let path_refs: Vec<&str> = dp.path.iter().map(String::as_str).collect();
                        if let Some(ref p) = profile_for_filter {
                            p.is_read_only(&path_refs)
                        } else {
                            read_only_heuristic::is_likely_read_only(&path_refs)
                        }
                    })
                    .map(|dp| dp.path.clone())
                    .collect();

                let enrichment_timeout = Duration::from_secs(bg_timeout_secs.min(10));
                let mut enrich_handles = Vec::new();

                for path in leaf_paths {
                    let cmd = bg_command.clone();
                    let slug = bg_slug.clone();
                    let tools_ref = bg_tools.clone();
                    let sem = schema_semaphore.clone();

                    let handle = tokio::spawn(async move {
                        let _permit = sem.acquire().await;
                        let subcommand_str = path.join(" ");
                        let result = parse_help_output(
                            &cmd,
                            Some(&subcommand_str),
                            enrichment_timeout,
                        )
                        .await;

                        if let Ok(arg_schema) = result {
                            let schema = arg_schema.to_json_schema();
                            let enriched_input_schema =
                                Arc::new(schema.as_object().cloned().unwrap_or_default());

                            let subcmd_encoded = path.join("_");
                            let tool_name = format!("{}__{}", slug, subcmd_encoded);

                            let mut tools_guard = tools_ref.write().await;
                            if let Some(tool) = tools_guard.iter_mut().find(|t| {
                                t.name.as_ref() == tool_name
                            }) {
                                tool.input_schema = enriched_input_schema;
                                tracing::debug!(
                                    slug = %slug,
                                    tool = %tool_name,
                                    "enriched tool schema from --help"
                                );
                            }
                        } else {
                            tracing::debug!(
                                slug = %slug,
                                path = ?path,
                                "schema enrichment skipped (--help parse failed)"
                            );
                        }
                    });
                    enrich_handles.push(handle);
                }

                for h in enrich_handles {
                    let _ = h.await;
                }

                bg_discovery_flag.store(false, std::sync::atomic::Ordering::Release);

                tracing::info!(
                    slug = %bg_slug,
                    "CLI discovery complete (all phases)"
                );
            });

            Ok(handle)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CliServerConfig, TransportKind};
    use std::collections::HashMap;

    fn make_cli_config(command: &str) -> CliServerConfig {
        CliServerConfig {
            slug: "test-cli".to_string(),
            enabled: true,
            transport: TransportKind::Cli,
            command: command.to_string(),
            profile: None,
            args: vec![],
            env: HashMap::new(),
            allow: vec![],
            deny: vec![],
            write_access: HashMap::new(),
            timeout_secs: 5,
            inject_flags: vec![],
            expand_subcommands: None,
            schema_override: None,
            help_depth: None,
            discovery_budget_secs: 60,
        }
    }

    fn make_cli_config_with_profile(command: &str, profile: &str) -> CliServerConfig {
        CliServerConfig {
            slug: command.to_string(),
            enabled: true,
            transport: TransportKind::Cli,
            command: command.to_string(),
            profile: Some(profile.to_string()),
            args: vec![],
            env: HashMap::new(),
            allow: vec![],
            deny: vec![],
            write_access: HashMap::new(),
            timeout_secs: 5,
            inject_flags: vec![],
            expand_subcommands: None,
            schema_override: None,
            help_depth: None,
            discovery_budget_secs: 60,
        }
    }

    #[test]
    fn test_extract_args_empty() {
        let params = CallToolRequestParams {
            name: "test".into(),
            arguments: None,
            task: None,
            meta: None,
        };
        assert_eq!(extract_args_from_params(&params), Vec::<String>::new());
    }

    #[test]
    fn test_extract_args_bool_true_emits_flag() {
        let mut args = serde_json::Map::new();
        args.insert("verbose".to_string(), serde_json::Value::Bool(true));
        let params = CallToolRequestParams {
            name: "test".into(),
            arguments: Some(args),
            task: None,
            meta: None,
        };
        let result = extract_args_from_params(&params);
        assert!(result.contains(&"--verbose".to_string()));
    }

    #[test]
    fn test_extract_args_bool_false_skipped() {
        let mut args = serde_json::Map::new();
        args.insert("dry-run".to_string(), serde_json::Value::Bool(false));
        let params = CallToolRequestParams {
            name: "test".into(),
            arguments: Some(args),
            task: None,
            meta: None,
        };
        let result = extract_args_from_params(&params);
        assert!(!result.contains(&"--dry-run".to_string()));
    }

    #[test]
    fn test_extract_args_string_value() {
        let mut args = serde_json::Map::new();
        args.insert(
            "region".to_string(),
            serde_json::Value::String("us-east-1".to_string()),
        );
        let params = CallToolRequestParams {
            name: "test".into(),
            arguments: Some(args),
            task: None,
            meta: None,
        };
        let result = extract_args_from_params(&params);
        assert!(result.contains(&"--region".to_string()));
        assert!(result.contains(&"us-east-1".to_string()));
    }

    #[test]
    fn test_expand_subcommands_without_profile_fails() {
        let mut config = make_cli_config("aws");
        config.expand_subcommands = Some(true);
        config.schema_override = Some(serde_json::json!({"type": "object", "properties": {}}));

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(spawn_cli_server(config, "aws".to_string()));
        assert!(
            matches!(result, Err(PorterError::InvalidConfig(_, ref msg)) if msg.contains("requires a built-in profile")),
            "expand_subcommands=true without profile should fail"
        );
    }

    #[tokio::test]
    async fn test_spawn_cli_server_with_schema_override() {
        let mut config = make_cli_config("echo");
        config.schema_override = Some(serde_json::json!({
            "type": "object",
            "properties": {
                "message": { "type": "string", "description": "Message to echo" }
            }
        }));

        let handle = spawn_cli_server(config, "echo-tool".to_string())
            .await
            .unwrap();

        assert_eq!(handle.slug, "echo-tool");
        assert_eq!(handle.command, "echo");
        assert_eq!(handle.health(), HealthState::Healthy);
        assert!(
            !handle.expanded,
            "schema_override without profile should not expand"
        );

        let tools = handle.tools().await;
        assert_eq!(tools.len(), 1);
    }

    #[tokio::test]
    async fn test_cli_handle_call_tool_echo() {
        let mut config = make_cli_config("echo");
        config.schema_override = Some(serde_json::json!({
            "type": "object",
            "properties": {}
        }));

        let handle = spawn_cli_server(config, "echo-tool".to_string())
            .await
            .unwrap();

        // Call with positional args
        let mut arguments = serde_json::Map::new();
        arguments.insert("args".to_string(), serde_json::json!(["hello", "world"]));
        let params = CallToolRequestParams {
            name: "echo-tool__echo".into(),
            arguments: Some(arguments),
            task: None,
            meta: None,
        };

        let result = handle.call_tool(params).await.unwrap();
        assert!(!result.content.is_empty());
        // echo returns exit 0, so is_error should be false
        assert_ne!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn test_unknown_profile_returns_invalid_config() {
        let config = make_cli_config_with_profile("aws", "not-a-real-profile");
        let result = spawn_cli_server(config, "aws".to_string()).await;
        assert!(
            matches!(result, Err(PorterError::InvalidConfig(_, ref msg)) if msg.contains("unknown built-in profile")),
            "unknown profile name should return InvalidConfig: {}",
            result.err().map(|e| e.to_string()).unwrap_or_default()
        );
    }

    #[tokio::test]
    async fn test_profile_aws_uses_inject_flags() {
        let config = make_cli_config_with_profile("aws", "aws");
        let handle = spawn_cli_server(config, "aws".to_string()).await.unwrap();
        // AWS profile injects --output json
        assert!(
            handle.inject_flags.contains(&"--output".to_string()),
            "aws profile should inject --output flag"
        );
        assert!(
            handle.inject_flags.contains(&"json".to_string()),
            "aws profile should inject json value"
        );
    }

    #[tokio::test]
    async fn test_user_inject_flags_override_profile() {
        let mut config = make_cli_config_with_profile("aws", "aws");
        config.inject_flags = vec!["--output".to_string(), "text".to_string()];
        let handle = spawn_cli_server(config, "aws".to_string()).await.unwrap();
        // User override wins
        assert!(
            handle.inject_flags.contains(&"text".to_string()),
            "user-specified inject_flags should override profile defaults"
        );
        assert!(
            !handle.inject_flags.contains(&"json".to_string()),
            "profile default should be overridden"
        );
    }

    #[tokio::test]
    async fn test_aws_profile_expand_by_default_creates_multiple_tools() {
        let config = make_cli_config_with_profile("aws", "aws");
        let handle = spawn_cli_server(config, "aws".to_string()).await.unwrap();
        assert!(handle.expanded, "aws profile should expand by default");
        let tools = handle.tools().await;
        assert!(
            tools.len() > 10,
            "aws expansion should create many tools, got: {}",
            tools.len()
        );
        // All tool names should start with "aws__"
        for tool in &tools {
            assert!(
                tool.name.starts_with("aws__"),
                "expanded tool name should start with slug__: {}",
                tool.name
            );
        }
    }

    #[tokio::test]
    async fn test_expand_subcommands_false_overrides_profile_default() {
        let mut config = make_cli_config_with_profile("aws", "aws");
        config.expand_subcommands = Some(false);
        config.schema_override = Some(serde_json::json!({"type": "object", "properties": {}}));
        let handle = spawn_cli_server(config, "aws".to_string()).await.unwrap();
        assert!(
            !handle.expanded,
            "expand_subcommands=false should override profile default"
        );
        let tools = handle.tools().await;
        assert_eq!(tools.len(), 1, "no expansion means single tool");
    }

    #[tokio::test]
    async fn test_rg_profile_does_not_expand() {
        let mut config = make_cli_config_with_profile("rg", "rg");
        config.schema_override = Some(serde_json::json!({"type": "object", "properties": {}}));
        let handle = spawn_cli_server(config, "rg".to_string()).await.unwrap();
        assert!(!handle.expanded, "rg profile should not expand by default");
        let tools = handle.tools().await;
        assert_eq!(tools.len(), 1, "rg is a single tool");
    }

    #[tokio::test]
    async fn test_expanded_call_tool_prepends_subcommand() {
        // Use echo as the command, encode a subcommand in the tool name
        // echo will just print whatever args we give it
        let mut config = make_cli_config_with_profile("echo", "doggo");
        // doggo profile: expand_by_default=false, so override to test expansion
        config.expand_subcommands = Some(false);
        config.schema_override = Some(serde_json::json!({"type": "object", "properties": {}}));

        // Manually create a CliHandle with expanded=true to test the subcommand injection logic
        let handle = CliHandle {
            slug: "echo-tool".to_string(),
            tools: Arc::new(RwLock::new(vec![])),
            guard: Arc::new(AccessGuard::new(&config)),
            command: "echo".to_string(),
            inject_flags: vec![],
            env: HashMap::new(),
            timeout: Duration::from_secs(5),
            expanded: true,
            discovery_in_progress: Arc::new(AtomicBool::new(false)),
        };

        // Tool name "echo-tool__ec2_describe-instances" encodes subcommand ["ec2", "describe-instances"]
        let params = CallToolRequestParams {
            name: "echo-tool__ec2_describe-instances".into(),
            arguments: None,
            task: None,
            meta: None,
        };

        // This should execute: echo ec2 describe-instances
        let result = handle.call_tool(params).await.unwrap();
        assert!(!result.content.is_empty());
    }
}
