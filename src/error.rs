//! Error types for Porter MCP gateway operations.

use thiserror::Error;

/// Main error type for Porter operations
#[derive(Error, Debug)]
pub enum PorterError {
    /// Duplicate server slug found in config
    #[error("duplicate server slug: {0}")]
    DuplicateSlug(String),

    /// Invalid configuration for a named server
    #[error("invalid config for server '{0}': {1}")]
    InvalidConfig(String, String),

    /// Initialization failed for a named server
    #[error("initialization failed for server '{0}': {1}")]
    InitializationFailed(String, String),

    /// Server is unhealthy
    #[error("server '{0}' is unhealthy: {1}")]
    ServerUnhealthy(String, String),

    /// MCP protocol error for a named server
    #[error("protocol error for server '{0}': {1}")]
    Protocol(String, String),

    /// Transport-level error for a named server
    #[error("transport error for server '{0}': {1}")]
    Transport(String, String),

    /// Call to a named server timed out
    #[error("call timeout for server '{0}'")]
    CallTimeout(String),

    /// Server is shutting down
    #[error("server '{0}' shutting down")]
    ShuttingDown(String),

    /// --help parsing failed for a CLI tool
    #[error("help parse failed for '{0}': {1}")]
    HelpParseFailed(String, String),

    /// --help command timed out for a CLI tool
    #[error("help timeout for '{0}'")]
    HelpTimeout(String),

    /// Access denied for a CLI tool invocation
    #[error("access denied for '{0}': {1}")]
    AccessDenied(String, String),

    /// Discovery timed out — partial results used
    #[error("discovery timed out for '{0}' — partial results used")]
    DiscoveryTimeout(String),
}

/// Result type alias for Porter operations
pub type Result<T> = std::result::Result<T, PorterError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duplicate_slug_display() {
        let err = PorterError::DuplicateSlug("gh".to_string());
        assert_eq!(err.to_string(), "duplicate server slug: gh");
    }

    #[test]
    fn test_invalid_config_display() {
        let err = PorterError::InvalidConfig(
            "gh".to_string(),
            "STDIO transport requires 'command' field".to_string(),
        );
        assert_eq!(
            err.to_string(),
            "invalid config for server 'gh': STDIO transport requires 'command' field"
        );
    }

    #[test]
    fn test_call_timeout_display() {
        let err = PorterError::CallTimeout("gh".to_string());
        assert_eq!(err.to_string(), "call timeout for server 'gh'");
    }
}
