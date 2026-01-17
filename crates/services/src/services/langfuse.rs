/// Configuration for Langfuse tracing integration.
///
/// Reads from environment variables:
/// - `LANGFUSE_PUBLIC_KEY`: Public API key (required)
/// - `LANGFUSE_SECRET_KEY`: Secret API key (required)
/// - `LANGFUSE_HOST`: API host (optional, defaults to `https://cloud.langfuse.com`)
/// - `TRACE_TO_LANGFUSE`: Enable flag (checked by `is_enabled()`)
#[derive(Debug, Clone)]
pub struct LangfuseConfig {
    pub public_key: String,
    pub secret_key: String,
    pub host: String,
}

impl LangfuseConfig {
    const DEFAULT_HOST: &'static str = "https://cloud.langfuse.com";

    /// Creates a new `LangfuseConfig` from environment variables.
    ///
    /// Returns `Some(config)` if both `LANGFUSE_PUBLIC_KEY` and `LANGFUSE_SECRET_KEY`
    /// are set. Returns `None` if either required key is missing.
    pub fn from_env() -> Option<Self> {
        let public_key = std::env::var("LANGFUSE_PUBLIC_KEY").ok()?;
        let secret_key = std::env::var("LANGFUSE_SECRET_KEY").ok()?;
        let host = std::env::var("LANGFUSE_HOST")
            .unwrap_or_else(|_| Self::DEFAULT_HOST.to_string());

        Some(Self {
            public_key,
            secret_key,
            host,
        })
    }

    /// Checks if Langfuse tracing is enabled via the `TRACE_TO_LANGFUSE` env var.
    ///
    /// Returns `true` if `TRACE_TO_LANGFUSE` is set to "true", "1", or "yes" (case-insensitive).
    pub fn is_enabled() -> bool {
        std::env::var("TRACE_TO_LANGFUSE")
            .map(|v| matches!(v.to_lowercase().as_str(), "true" | "1" | "yes"))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    /// # Safety
    /// This function modifies environment variables, which is inherently unsafe
    /// in multi-threaded contexts. Only use in single-threaded tests.
    unsafe fn clear_langfuse_env_vars() {
        unsafe {
            env::remove_var("LANGFUSE_PUBLIC_KEY");
            env::remove_var("LANGFUSE_SECRET_KEY");
            env::remove_var("LANGFUSE_HOST");
            env::remove_var("TRACE_TO_LANGFUSE");
        }
    }

    #[test]
    fn test_from_env_returns_none_when_keys_missing() {
        unsafe { clear_langfuse_env_vars() };

        assert!(LangfuseConfig::from_env().is_none());
    }

    #[test]
    fn test_from_env_returns_none_when_only_public_key_set() {
        unsafe {
            clear_langfuse_env_vars();
            env::set_var("LANGFUSE_PUBLIC_KEY", "pk-test");
        }

        assert!(LangfuseConfig::from_env().is_none());

        unsafe { clear_langfuse_env_vars() };
    }

    #[test]
    fn test_from_env_returns_none_when_only_secret_key_set() {
        unsafe {
            clear_langfuse_env_vars();
            env::set_var("LANGFUSE_SECRET_KEY", "sk-test");
        }

        assert!(LangfuseConfig::from_env().is_none());

        unsafe { clear_langfuse_env_vars() };
    }

    #[test]
    fn test_from_env_returns_some_when_both_keys_set() {
        unsafe {
            clear_langfuse_env_vars();
            env::set_var("LANGFUSE_PUBLIC_KEY", "pk-test");
            env::set_var("LANGFUSE_SECRET_KEY", "sk-test");
        }

        let config = LangfuseConfig::from_env();
        assert!(config.is_some());

        let config = config.unwrap();
        assert_eq!(config.public_key, "pk-test");
        assert_eq!(config.secret_key, "sk-test");
        assert_eq!(config.host, "https://cloud.langfuse.com");

        unsafe { clear_langfuse_env_vars() };
    }

    #[test]
    fn test_from_env_uses_custom_host() {
        unsafe {
            clear_langfuse_env_vars();
            env::set_var("LANGFUSE_PUBLIC_KEY", "pk-test");
            env::set_var("LANGFUSE_SECRET_KEY", "sk-test");
            env::set_var("LANGFUSE_HOST", "https://custom.langfuse.io");
        }

        let config = LangfuseConfig::from_env().unwrap();
        assert_eq!(config.host, "https://custom.langfuse.io");

        unsafe { clear_langfuse_env_vars() };
    }

    #[test]
    fn test_is_enabled_returns_false_when_not_set() {
        unsafe { clear_langfuse_env_vars() };

        assert!(!LangfuseConfig::is_enabled());
    }

    #[test]
    fn test_is_enabled_returns_true_for_valid_values() {
        for value in ["true", "TRUE", "True", "1", "yes", "YES", "Yes"] {
            unsafe {
                clear_langfuse_env_vars();
                env::set_var("TRACE_TO_LANGFUSE", value);
            }

            assert!(
                LangfuseConfig::is_enabled(),
                "Expected is_enabled() to be true for value: {}",
                value
            );
        }

        unsafe { clear_langfuse_env_vars() };
    }

    #[test]
    fn test_is_enabled_returns_false_for_invalid_values() {
        for value in ["false", "0", "no", "anything", ""] {
            unsafe {
                clear_langfuse_env_vars();
                env::set_var("TRACE_TO_LANGFUSE", value);
            }

            assert!(
                !LangfuseConfig::is_enabled(),
                "Expected is_enabled() to be false for value: {}",
                value
            );
        }

        unsafe { clear_langfuse_env_vars() };
    }
}
