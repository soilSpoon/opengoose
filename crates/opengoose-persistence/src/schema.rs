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
    schedules (id) {
        id -> Integer,
        name -> Text,
        cron_expression -> Text,
        team_name -> Text,
        input -> Text,
        enabled -> Integer,
        last_run_at -> Nullable<Text>,
        next_run_at -> Nullable<Text>,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    agent_messages (id) {
        id -> Integer,
        session_key -> Text,
        from_agent -> Text,
        to_agent -> Nullable<Text>,
        channel -> Nullable<Text>,
        payload -> Text,
        status -> Text,
        created_at -> Text,
        delivered_at -> Nullable<Text>,
    }
}

diesel::table! {
    triggers (id) {
        id -> Integer,
        name -> Text,
        trigger_type -> Text,
        condition_json -> Text,
        team_name -> Text,
        input -> Text,
        enabled -> Integer,
        last_fired_at -> Nullable<Text>,
        fire_count -> Integer,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::table! {
    plugins (id) {
        id -> Integer,
        name -> Text,
        version -> Text,
        author -> Nullable<Text>,
        description -> Nullable<Text>,
        capabilities -> Text,
        source_path -> Text,
        enabled -> Integer,
        created_at -> Text,
        updated_at -> Text,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    sessions,
    messages,
    message_queue,
    work_items,
    orchestration_runs,
    schedules,
    agent_messages,
    triggers,
    plugins,
);
