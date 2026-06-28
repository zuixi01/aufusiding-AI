//! 结构化日志初始化与敏感信息脱敏

/// 日志初始化句柄（持有非阻塞写入 guard）
pub struct LoggingHandle {
    _guard: tracing_appender::non_blocking::WorkerGuard,
}

/// 初始化文件日志（按天轮转）
pub fn init_file_logging(log_dir: &std::path::Path, level: &str) -> anyhow::Result<LoggingHandle> {
    std::fs::create_dir_all(log_dir)?;

    let appender = tracing_appender::rolling::daily(log_dir, "prometheus.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(appender);

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(normalize_level(level))
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true)
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);

    Ok(LoggingHandle { _guard: guard })
}

/// 对日志文本进行敏感信息脱敏
pub fn sanitize_text(input: &str) -> String {
    let mut out = input.to_string();
    out = redact_prefixed_secret(&out, "sk-");
    out = redact_prefixed_secret(&out, "ghp_");
    out = redact_key_value_secret(&out, "api_key=");
    out = redact_key_value_secret(&out, "token=");
    out
}

fn redact_prefixed_secret(input: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    let bytes = input.as_bytes();

    while i < bytes.len() {
        if input[i..].starts_with(prefix) {
            out.push_str("[REDACTED]");
            i += prefix.len();
            while i < bytes.len() && is_secret_char(bytes[i] as char) {
                i += 1;
            }
            continue;
        }

        if let Some(ch) = input[i..].chars().next() {
            out.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }

    out
}

fn redact_key_value_secret(input: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    let bytes = input.as_bytes();

    while i < bytes.len() {
        if input[i..].starts_with(prefix) {
            out.push_str(prefix);
            out.push_str("[REDACTED]");
            i += prefix.len();
            while i < bytes.len() && is_secret_char(bytes[i] as char) {
                i += 1;
            }
            continue;
        }

        if let Some(ch) = input[i..].chars().next() {
            out.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }

    out
}

fn is_secret_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.')
}

fn normalize_level(level: &str) -> tracing::Level {
    match level.to_ascii_lowercase().as_str() {
        "trace" => tracing::Level::TRACE,
        "debug" => tracing::Level::DEBUG,
        "warn" => tracing::Level::WARN,
        "error" => tracing::Level::ERROR,
        _ => tracing::Level::INFO,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_openai_style_secret() {
        let input = "api_key=sk-1234567890abcdef";
        let output = sanitize_text(input);
        assert!(!output.contains("sk-1234567890abcdef"));
        assert!(output.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_github_token_prefix() {
        let input = "token=ghp_abcdefghijklmnopqrstuvwxyz123456";
        let output = sanitize_text(input);
        assert!(!output.contains("ghp_abcdefghijklmnopqrstuvwxyz123456"));
        assert!(output.contains("[REDACTED]"));
    }

    #[test]
    fn init_file_logging_accepts_known_levels() {
        let log_dir = crate::test_support::temp_dir_path("prometheus-log");
        let res = init_file_logging(&log_dir, "info");
        assert!(res.is_ok());
    }
}
