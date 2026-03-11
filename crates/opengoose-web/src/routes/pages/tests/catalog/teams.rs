use axum::extract::Form;
use axum::response::Html;

use super::super::support::{run_async, with_pages_home};
use super::{TeamSaveForm, team_save};

#[test]
fn team_save_invalid_yaml_renders_editor_error_notice() {
    with_pages_home(|| {
        run_async(async {
            let Html(html) = team_save(Form(TeamSaveForm {
                original_name: "broken-team".into(),
                yaml: "title: broken-team".into(),
            }))
            .await
            .expect("handler should render");

            assert!(html.contains("Fix the YAML validation error and try again."));
            assert!(html.contains("Editor draft"));
        });
    });
}
