use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CheckCategory {
    Database,
    Feed,
    Rpc,
    Secrets,
}

impl fmt::Display for CheckCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheckCategory::Database => write!(f, "Database"),
            CheckCategory::Feed => write!(f, "Feed URLs"),
            CheckCategory::Rpc => write!(f, "RPC Nodes"),
            CheckCategory::Secrets => write!(f, "Secrets"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub category: CheckCategory,
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
    pub hint: Option<String>,
    #[serde(with = "duration_millis")]
    pub duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    pub checks: Vec<CheckResult>,
    #[serde(with = "duration_millis")]
    pub total_duration: Duration,
}

impl VerificationReport {
    pub fn has_failures(&self) -> bool {
        self.checks.iter().any(|c| c.status == CheckStatus::Fail)
    }

    pub fn count_by_status(&self) -> (usize, usize, usize) {
        let pass = self
            .checks
            .iter()
            .filter(|c| c.status == CheckStatus::Pass)
            .count();
        let warn = self
            .checks
            .iter()
            .filter(|c| c.status == CheckStatus::Warn)
            .count();
        let fail = self
            .checks
            .iter()
            .filter(|c| c.status == CheckStatus::Fail)
            .count();
        (pass, warn, fail)
    }
}

mod duration_millis {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u128(duration.as_millis())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_category_display() {
        assert_eq!(CheckCategory::Database.to_string(), "Database");
        assert_eq!(CheckCategory::Feed.to_string(), "Feed URLs");
        assert_eq!(CheckCategory::Rpc.to_string(), "RPC Nodes");
        assert_eq!(CheckCategory::Secrets.to_string(), "Secrets");
    }

    #[test]
    fn test_verification_report_has_failures() {
        let report = VerificationReport {
            checks: vec![CheckResult {
                category: CheckCategory::Database,
                name: "test".to_string(),
                status: CheckStatus::Pass,
                message: "ok".to_string(),
                hint: None,
                duration: Duration::from_millis(10),
            }],
            total_duration: Duration::from_millis(10),
        };
        assert!(!report.has_failures());

        let report = VerificationReport {
            checks: vec![CheckResult {
                category: CheckCategory::Database,
                name: "test".to_string(),
                status: CheckStatus::Fail,
                message: "failed".to_string(),
                hint: Some("fix it".to_string()),
                duration: Duration::from_millis(10),
            }],
            total_duration: Duration::from_millis(10),
        };
        assert!(report.has_failures());
    }

    #[test]
    fn test_count_by_status() {
        let report = VerificationReport {
            checks: vec![
                CheckResult {
                    category: CheckCategory::Database,
                    name: "a".to_string(),
                    status: CheckStatus::Pass,
                    message: "ok".to_string(),
                    hint: None,
                    duration: Duration::from_millis(1),
                },
                CheckResult {
                    category: CheckCategory::Feed,
                    name: "b".to_string(),
                    status: CheckStatus::Warn,
                    message: "warning".to_string(),
                    hint: None,
                    duration: Duration::from_millis(2),
                },
                CheckResult {
                    category: CheckCategory::Rpc,
                    name: "c".to_string(),
                    status: CheckStatus::Fail,
                    message: "fail".to_string(),
                    hint: None,
                    duration: Duration::from_millis(3),
                },
            ],
            total_duration: Duration::from_millis(6),
        };
        assert_eq!(report.count_by_status(), (1, 1, 1));
    }

    #[test]
    fn test_json_serialization() {
        let report = VerificationReport {
            checks: vec![CheckResult {
                category: CheckCategory::Database,
                name: "test".to_string(),
                status: CheckStatus::Pass,
                message: "ok".to_string(),
                hint: None,
                duration: Duration::from_millis(42),
            }],
            total_duration: Duration::from_millis(42),
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"duration\":42"));
    }
}
