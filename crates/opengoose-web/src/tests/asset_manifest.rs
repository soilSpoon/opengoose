use std::fs;
use std::path::PathBuf;

fn asset_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

#[test]
fn css_entrypoint_stays_thin_and_imports_existing_partials() {
    let asset_dir = asset_dir();
    let app_css_path = asset_dir.join("app.css");
    let app_css = fs::read_to_string(&app_css_path).expect("app.css should be readable");

    let expected_imports = [
        "./styles/theme.css",
        "./styles/base.css",
        "./styles/shell.css",
        "./styles/data-views.css",
        "./styles/monitoring.css",
        "./styles/forms.css",
        "./styles/responsive.css",
    ];

    let line_count = app_css.lines().count();
    assert!(
        line_count <= 12,
        "expected app.css to remain a thin entrypoint, found {line_count} lines",
    );

    for path in expected_imports {
        assert!(app_css.contains(path), "expected app.css to import {path}",);
        assert!(
            asset_dir.join(path.trim_start_matches("./")).is_file(),
            "expected imported stylesheet {path} to exist",
        );
    }
}
