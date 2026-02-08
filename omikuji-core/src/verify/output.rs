use std::collections::BTreeMap;

use super::types::{CheckResult, CheckStatus, VerificationReport};

const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

fn status_label(status: &CheckStatus) -> &'static str {
    match status {
        CheckStatus::Pass => "PASS",
        CheckStatus::Warn => "WARN",
        CheckStatus::Fail => "FAIL",
    }
}

fn status_color(status: &CheckStatus) -> &'static str {
    match status {
        CheckStatus::Pass => GREEN,
        CheckStatus::Warn => YELLOW,
        CheckStatus::Fail => RED,
    }
}

fn status_icon(status: &CheckStatus) -> &'static str {
    match status {
        CheckStatus::Pass => "✓",
        CheckStatus::Warn => "!",
        CheckStatus::Fail => "✗",
    }
}

/// Format a verification report as human-readable text with ANSI colors.
pub fn format_text_report(report: &VerificationReport) -> String {
    let mut out = String::new();

    // Group checks by category, preserving insertion order via BTreeMap key
    let mut groups: BTreeMap<String, Vec<&CheckResult>> = BTreeMap::new();
    for check in &report.checks {
        groups
            .entry(check.category.to_string())
            .or_default()
            .push(check);
    }

    for (category, checks) in &groups {
        out.push_str(&format!("\n{BOLD}{category}{RESET}\n"));
        for check in checks {
            let color = status_color(&check.status);
            let label = status_label(&check.status);
            let icon = status_icon(&check.status);
            out.push_str(&format!(
                "  {color}{icon} [{label}]{RESET} {} — {} ({} ms)\n",
                check.name,
                check.message,
                check.duration.as_millis(),
            ));
            if let Some(hint) = &check.hint {
                out.push_str(&format!("           {YELLOW}hint: {hint}{RESET}\n"));
            }
        }
    }

    let (pass, warn, fail) = report.count_by_status();
    out.push_str(&format!(
        "\n{BOLD}Summary:{RESET} {GREEN}{pass} passed{RESET}, {YELLOW}{warn} warnings{RESET}, {RED}{fail} failed{RESET} ({} ms)\n",
        report.total_duration.as_millis(),
    ));

    out
}

/// Format a verification report as pretty-printed JSON.
pub fn format_json_report(report: &VerificationReport) -> String {
    serde_json::to_string_pretty(report).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::types::CheckCategory;
    use std::time::Duration;

    fn sample_report() -> VerificationReport {
        VerificationReport {
            checks: vec![
                CheckResult {
                    category: CheckCategory::Database,
                    name: "DATABASE_URL set".to_string(),
                    status: CheckStatus::Pass,
                    message: "Environment variable is set".to_string(),
                    hint: None,
                    duration: Duration::from_millis(1),
                },
                CheckResult {
                    category: CheckCategory::Database,
                    name: "Connectivity".to_string(),
                    status: CheckStatus::Fail,
                    message: "Connection refused".to_string(),
                    hint: Some("Check that PostgreSQL is running".to_string()),
                    duration: Duration::from_millis(5000),
                },
                CheckResult {
                    category: CheckCategory::Rpc,
                    name: "RPC: mainnet/Infura".to_string(),
                    status: CheckStatus::Warn,
                    message: "Block number < 100".to_string(),
                    hint: Some("Node may still be syncing".to_string()),
                    duration: Duration::from_millis(200),
                },
            ],
            total_duration: Duration::from_millis(5201),
        }
    }

    #[test]
    fn test_text_report_contains_status_labels() {
        let text = format_text_report(&sample_report());
        assert!(text.contains("[PASS]"));
        assert!(text.contains("[FAIL]"));
        assert!(text.contains("[WARN]"));
    }

    #[test]
    fn test_text_report_contains_hints() {
        let text = format_text_report(&sample_report());
        assert!(text.contains("hint: Check that PostgreSQL is running"));
    }

    #[test]
    fn test_text_report_contains_summary() {
        let text = format_text_report(&sample_report());
        assert!(text.contains("1 passed"));
        assert!(text.contains("1 warnings"));
        assert!(text.contains("1 failed"));
    }

    #[test]
    fn test_json_report_is_valid_json() {
        let json = format_json_report(&sample_report());
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("checks").unwrap().is_array());
        assert!(parsed.get("total_duration").unwrap().is_number());
    }
}
