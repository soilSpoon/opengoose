use crate::db_enum::db_enum;

db_enum! {
    /// Status of an orchestration or workflow run.
    pub enum RunStatus {
        Running => "running",
        Completed => "completed",
        Failed => "failed",
        Suspended => "suspended",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PersistenceError;

    #[test]
    fn test_run_status_as_str() {
        assert_eq!(RunStatus::Running.as_str(), "running");
        assert_eq!(RunStatus::Completed.as_str(), "completed");
        assert_eq!(RunStatus::Failed.as_str(), "failed");
        assert_eq!(RunStatus::Suspended.as_str(), "suspended");
    }

    #[test]
    fn test_run_status_parse_valid() {
        assert_eq!(RunStatus::parse("running").unwrap(), RunStatus::Running);
        assert_eq!(RunStatus::parse("completed").unwrap(), RunStatus::Completed);
        assert_eq!(RunStatus::parse("failed").unwrap(), RunStatus::Failed);
        assert_eq!(RunStatus::parse("suspended").unwrap(), RunStatus::Suspended);
    }

    #[test]
    fn test_run_status_parse_invalid() {
        let err = RunStatus::parse("bogus").unwrap_err();
        match err {
            PersistenceError::InvalidEnumValue(msg) => {
                assert!(msg.contains("RunStatus"));
                assert!(msg.contains("bogus"));
            }
            other => unreachable!("expected InvalidEnumValue, got: {:?}", other),
        }
    }

    #[test]
    fn test_run_status_roundtrip() {
        for status in [
            RunStatus::Running,
            RunStatus::Completed,
            RunStatus::Failed,
            RunStatus::Suspended,
        ] {
            assert_eq!(RunStatus::parse(status.as_str()).unwrap(), status);
        }
    }
}
