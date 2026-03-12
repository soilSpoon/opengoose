use opengoose_teams::scheduler;

#[test]
fn validate_cron_accepts_standard_six_field_expression() {
    assert!(scheduler::validate_cron("0 0 * * * *").is_ok());
}

#[test]
fn validate_cron_accepts_every_minute() {
    assert!(scheduler::validate_cron("0 * * * * *").is_ok());
}

#[test]
fn validate_cron_accepts_specific_time() {
    assert!(scheduler::validate_cron("0 30 9 * * *").is_ok());
}

#[test]
fn validate_cron_rejects_empty_string() {
    assert!(scheduler::validate_cron("").is_err());
}

#[test]
fn validate_cron_rejects_invalid_expression() {
    let err = scheduler::validate_cron("not-a-cron").unwrap_err();
    assert!(err.contains("invalid cron expression"));
}

#[test]
fn validate_cron_rejects_too_few_fields() {
    assert!(scheduler::validate_cron("* * *").is_err());
}

#[test]
fn next_fire_time_returns_some_for_valid_expression() {
    let result = scheduler::next_fire_time("0 * * * * *");
    assert!(result.is_some());
    let time_str = result.unwrap();
    assert!(time_str.contains('-'));
    assert!(time_str.contains(':'));
}

#[test]
fn next_fire_time_returns_none_for_invalid_expression() {
    let result = scheduler::next_fire_time("invalid");
    assert!(result.is_none());
}
