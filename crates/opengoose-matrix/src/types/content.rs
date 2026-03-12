/// Build a plain-text `m.room.message` content object.
pub fn text_content(body: &str) -> serde_json::Value {
    serde_json::json!({
        "msgtype": "m.text",
        "body": body,
    })
}

/// Build an edited `m.room.message` that replaces an earlier event.
///
/// Editors use the `m.replace` relationship as defined in the Matrix spec.
pub fn edit_content(original_event_id: &str, new_body: &str) -> serde_json::Value {
    serde_json::json!({
        "msgtype": "m.text",
        "body": format!("* {new_body}"),
        "m.new_content": {
            "msgtype": "m.text",
            "body": new_body,
        },
        "m.relates_to": {
            "rel_type": "m.replace",
            "event_id": original_event_id,
        },
    })
}
