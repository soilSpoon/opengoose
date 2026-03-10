diesel::table! {
    sessions (id) {
        id -> Integer,
        session_key -> Text,
        active_team -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    messages (id) {
        id -> Integer,
        session_key -> Text,
        role -> Text,
        content -> Text,
        author -> Nullable<Text>,
        created_at -> Text,
    }
}

diesel::table! {
    message_queue (id) {
        id -> Integer,
        session_key -> Text,
        team_run_id -> Text,
        sender -> Text,
        recipient -> Text,
        content -> Text,
        msg_type -> Text,
        status -> Text,
        retry_count -> Integer,
        max_retries -> Integer,
        created_at -> Text,
        processed_at -> Nullable<Text>,
        error -> Nullable<Text>,
    }
}

diesel::table! {
    work_items (id) {
        id -> Integer,
        session_key -> Text,
        team_run_id -> Text,
        parent_id -> Nullable<Integer>,
        title -> Text,
        description -> Nullable<Text>,
        status -> Text,
        assigned_to -> Nullable<Text>,
        workflow_step -> Nullable<Integer>,
        input -> Nullable<Text>,
        output -> Nullable<Text>,
        error -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    orchestration_runs (id) {
        id -> Integer,
        team_run_id -> Text,
        session_key -> Text,
        team_name -> Text,
        workflow -> Text,
        input -> Text,
        status -> Text,
        current_step -> Integer,
        total_steps -> Integer,
        result -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    alert_rules (id) {
        id -> Text,
        name -> Text,
        description -> Nullable<Text>,
        metric -> Text,
        condition -> Text,
        threshold -> Double,
        enabled -> Integer,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    alert_history (id) {
        id -> Integer,
        rule_id -> Text,
        rule_name -> Text,
        metric -> Text,
        value -> Double,
        triggered_at -> Text,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    sessions,
    messages,
    message_queue,
    work_items,
    orchestration_runs,
    alert_rules,
    alert_history,
);
