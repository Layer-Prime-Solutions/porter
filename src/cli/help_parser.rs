//! Help text parser for CLI tool argument schema discovery.
//!
//! Runs `<command> [subcommand] --help` and parses the output to extract
//! argument definitions (flags, their types, and descriptions). The resulting
//! `ArgumentSchema` can be converted to JSON Schema for MCP tool registration.

use std::collections::HashMap;
use std::time::Duration;

use regex::Regex;
use tokio::process::Command;

use crate::error::PorterError;

/// The type of a CLI flag argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgType {
    /// A boolean flag (e.g., `--verbose`, `--dry-run`)
    Bool,
    /// A required string value (e.g., `--output json`, `--region=us-east-1`)
    String,
    /// An optional string value (e.g., `--output [format]`)
    OptionalString,
}

/// A single argument property extracted from --help output.
#[derive(Debug, Clone)]
pub struct ArgProperty {
    pub arg_type: ArgType,
    pub description: Option<String>,
    /// Long flag name including leading dashes (e.g., "--output")
    pub long_flag: String,
    /// Short flag including leading dash (e.g., "-o"), if any
    pub short_flag: Option<String>,
}

/// Argument schema extracted from a CLI tool's --help output.
///
/// Contains a map of property names (without dashes) to their definitions.
#[derive(Debug, Clone)]
pub struct ArgumentSchema {
    pub properties: HashMap<String, ArgProperty>,
}

impl ArgumentSchema {
    /// Convert to a JSON Schema object suitable for MCP tool registration.
    ///
    /// Maps Bool -> boolean, String/OptionalString -> string.
    pub fn to_json_schema(&self) -> serde_json::Value {
        let mut props = serde_json::Map::new();
        let mut required = Vec::new();

        for (name, prop) in &self.properties {
            let type_str = match prop.arg_type {
                ArgType::Bool => "boolean",
                ArgType::String | ArgType::OptionalString => "string",
            };

            let mut schema = serde_json::json!({ "type": type_str });
            if let Some(desc) = &prop.description {
                schema["description"] = serde_json::Value::String(desc.clone());
            }
            if let Some(short) = &prop.short_flag {
                schema["x-short-flag"] = serde_json::Value::String(short.clone());
            }
            props.insert(name.clone(), schema);

            if prop.arg_type == ArgType::String {
                required.push(serde_json::Value::String(name.clone()));
            }
        }

        let mut schema = serde_json::json!({
            "type": "object",
            "properties": props,
        });

        if !required.is_empty() {
            schema["required"] = serde_json::Value::Array(required);
        }

        schema
    }
}

/// Run `<command> [subcommand] --help` and parse the output into an `ArgumentSchema`.
///
/// Tries stdout first; falls back to stderr if stdout is empty (many CLI tools
/// write help to stderr when invoked with --help). Returns `PorterError::HelpTimeout`
/// if the command does not complete within `timeout`, and `PorterError::HelpParseFailed`
/// if no flag definitions are found in the output.
pub async fn parse_help_output(
    command: &str,
    subcommand: Option<&str>,
    timeout: Duration,
) -> crate::Result<ArgumentSchema> {
    let mut cmd = Command::new(command);
    if let Some(sub) = subcommand {
        cmd.arg(sub);
    }
    cmd.arg("--help");
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let output_result = tokio::time::timeout(timeout, async {
        let child = cmd
            .spawn()
            .map_err(|e| PorterError::HelpParseFailed(command.to_string(), e.to_string()))?;
        child
            .wait_with_output()
            .await
            .map_err(|e| PorterError::HelpParseFailed(command.to_string(), e.to_string()))
    })
    .await;

    let output = match output_result {
        Ok(result) => result?,
        Err(_elapsed) => return Err(PorterError::HelpTimeout(command.to_string())),
    };

    // Try stdout first; fall back to stderr (Pitfall 1: many CLIs write help to stderr)
    let help_text = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr).into_owned()
    } else {
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    parse_flag_definitions(command, &help_text)
}

/// Extract description text from the "rest" portion after a flag definition.
///
/// Skips the value placeholder token (e.g., "OUTPUT", "<VALUE>", "[value]")
/// and returns the remaining human-readable description text.
fn extract_description(rest: &str) -> Option<String> {
    let trimmed = rest.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Skip leading value token: UPPER_CASE, <VALUE>, [value], or =VALUE patterns
    let after_value = if let Some(s) = trimmed.strip_prefix('<') {
        // <VALUE> — skip to after '>'
        s.find('>').map(|i| &s[i + 1..]).unwrap_or("").trim()
    } else if let Some(s) = trimmed.strip_prefix('[') {
        // [value] — skip to after ']'
        s.find(']').map(|i| &s[i + 1..]).unwrap_or("").trim()
    } else if let Some(s) = trimmed.strip_prefix('=') {
        // =VALUE — skip the whole =token
        let end = s.find(|c: char| c.is_whitespace()).unwrap_or(s.len());
        s[end..].trim()
    } else if trimmed
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
    {
        // ALLCAPS token — skip it
        let end = trimmed
            .find(|c: char| c.is_whitespace())
            .unwrap_or(trimmed.len());
        trimmed[end..].trim()
    } else {
        // No recognized value pattern — the whole rest is description
        trimmed
    };

    if after_value.is_empty() {
        None
    } else {
        Some(after_value.to_string())
    }
}

/// Parse flag definitions from raw --help text.
///
/// Recognizes patterns:
/// - `--flag` (bool — no value)
/// - `--flag VALUE`, `--flag=VALUE`, `--flag <VALUE>` (required string)
/// - `--flag [VALUE]` (optional string)
/// - `-f, --flag` or `-f/--flag` (short + long combined)
/// - Trailing description text is captured from the same line
pub fn parse_flag_definitions(command: &str, help_text: &str) -> crate::Result<ArgumentSchema> {
    // Regex groups:
    // 1: optional short flag letter (from "-X, " or "-X " prefix before --)
    // 2: long flag name (after --)
    // 3: rest of the token after the flag name (value placeholder + description)
    let flag_re = Regex::new(
        r"(?:-([a-zA-Z0-9])(?:[,/]\s*|\s+))?--([a-zA-Z][a-zA-Z0-9_-]*)((?:[= ][^\s,]+|\s+\[[^\]]+\])?(?:\s+.+)?)",
    )
    .expect("valid regex");

    // Inline value patterns (applied to the captured "rest" string):
    // Required: starts with value placeholder before description
    //   - " ALLCAPS" or " <anything>" or " word" (lowercase type name like "string", "int")
    //   - "=ALLCAPS" or "=<anything>"
    let required_re = Regex::new(r"^[ =]<[^>]+>|^[ =][A-Z][A-Z0-9_-]+|^ [a-z][a-zA-Z0-9_-]+")
        .expect("valid regex");
    // Optional: starts with " [word]" or "[=word]"
    let optional_re = Regex::new(r"^\s+\[[a-zA-Z]|^\[=").expect("valid regex");

    let mut properties = HashMap::new();

    for caps in flag_re.captures_iter(help_text) {
        // Group 2: long flag name (required)
        let long_name = match caps.get(2) {
            Some(m) => m.as_str().to_string(),
            None => continue,
        };
        let long_flag = format!("--{}", long_name);

        // Group 1: optional short flag
        let short = caps.get(1).map(|m| format!("-{}", m.as_str()));

        // Group 3: rest of line after flag name
        let rest = caps.get(3).map(|m| m.as_str()).unwrap_or("");

        let arg_type = if optional_re.is_match(rest) {
            ArgType::OptionalString
        } else if required_re.is_match(rest) {
            ArgType::String
        } else {
            ArgType::Bool
        };

        // Extract description: skip the value placeholder (first token), rest is description
        let description = extract_description(rest);

        // Property name: long flag name with underscores for hyphens
        let prop_name = long_name.replace('-', "_");

        properties.insert(
            prop_name,
            ArgProperty {
                arg_type,
                description,
                long_flag,
                short_flag: short,
            },
        );
    }

    if properties.is_empty() {
        return Err(PorterError::HelpParseFailed(
            command.to_string(),
            "no flag definitions found in help output".to_string(),
        ));
    }

    Ok(ArgumentSchema { properties })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aws_style_help() -> &'static str {
        r#"usage: aws [options] <command> <subcommand>

Options:
  --debug                   Turn on debug logging
  --endpoint-url ENDPOINT   Override command's default URL
  --no-verify-ssl           By default, the AWS CLI uses SSL
  --output OUTPUT           The formatting style for command output
  --profile PROFILE_NAME    Use a specific profile from your credential file
  --region REGION           The region to use. Overrides config/env settings.
  --version, -v             Display the version of this tool
  --color VALUE             Turn on/off color output

See 'aws help' for descriptions of global parameters.
"#
    }

    fn kubectl_style_help() -> &'static str {
        r#"kubectl controls the Kubernetes cluster manager.

Usage:
  kubectl [command]

Options:
      --as string                      Username to impersonate
      --as-group stringArray           Group to impersonate
  -n, --namespace string               If present, the namespace scope
      --request-timeout string         The length of time to wait
  -v, --v Level                        number for the log level verbosity
      --kubeconfig string              Path to the kubeconfig file
"#
    }

    #[test]
    fn test_parse_aws_style_help() {
        let schema = parse_flag_definitions("aws", aws_style_help()).unwrap();
        assert!(
            schema.properties.contains_key("output"),
            "expected --output flag"
        );
        assert!(
            schema.properties.contains_key("region"),
            "expected --region flag"
        );
        assert!(
            schema.properties.contains_key("profile"),
            "expected --profile flag"
        );
        assert!(
            schema.properties.contains_key("debug"),
            "expected --debug flag"
        );

        let output_prop = &schema.properties["output"];
        assert_eq!(output_prop.arg_type, ArgType::String);
        assert_eq!(output_prop.long_flag, "--output");

        let debug_prop = &schema.properties["debug"];
        assert_eq!(debug_prop.arg_type, ArgType::Bool);
    }

    #[test]
    fn test_parse_kubectl_style_help_with_short_flags() {
        let schema = parse_flag_definitions("kubectl", kubectl_style_help()).unwrap();
        assert!(
            schema.properties.contains_key("namespace"),
            "expected --namespace flag"
        );

        let ns_prop = &schema.properties["namespace"];
        assert_eq!(ns_prop.arg_type, ArgType::String);
        assert_eq!(ns_prop.long_flag, "--namespace");
        assert_eq!(ns_prop.short_flag, Some("-n".to_string()));
    }

    #[test]
    fn test_parse_empty_help_returns_error() {
        let result = parse_flag_definitions("mytool", "");
        assert!(
            matches!(result, Err(PorterError::HelpParseFailed(cmd, _)) if cmd == "mytool"),
            "expected HelpParseFailed for empty help text"
        );
    }

    #[test]
    fn test_parse_no_flags_returns_error() {
        let help = "Usage: mytool <command>\n\nCommands:\n  run    Run something\n  stop   Stop something\n";
        let result = parse_flag_definitions("mytool", help);
        assert!(
            matches!(result, Err(PorterError::HelpParseFailed(_, _))),
            "expected HelpParseFailed when no flags found"
        );
    }

    #[test]
    fn test_to_json_schema_correct_types() {
        let schema = parse_flag_definitions("aws", aws_style_help()).unwrap();
        let json = schema.to_json_schema();

        let props = json["properties"].as_object().unwrap();

        // --output OUTPUT -> string
        assert_eq!(props["output"]["type"], "string");

        // --debug (bool)
        assert_eq!(props["debug"]["type"], "boolean");

        // --region REGION -> string
        assert_eq!(props["region"]["type"], "string");
    }

    #[test]
    fn test_to_json_schema_has_object_type() {
        let schema = parse_flag_definitions("aws", aws_style_help()).unwrap();
        let json = schema.to_json_schema();
        assert_eq!(json["type"], "object");
    }
}
