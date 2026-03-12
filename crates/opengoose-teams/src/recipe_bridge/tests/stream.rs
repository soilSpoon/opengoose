use std::collections::HashMap;

use goose::agents::extension::{Envs, ExtensionConfig};

use opengoose_profiles::ExtensionRef;

use super::super::{config_to_ext_ref, ext_ref_to_config, profile_to_recipe};
use super::empty_profile;

#[test]
fn profile_to_recipe_skips_incomplete_streamable_http_extensions() {
    let mut profile = empty_profile("invalid-stream-extensions");
    profile.extensions = vec![
        ExtensionRef {
            name: "builtin".into(),
            ext_type: "builtin".into(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: Some(30),
            envs: HashMap::new(),
            env_keys: vec![],
            code: None,
            dependencies: None,
        },
        ExtensionRef {
            name: "http-missing-uri".into(),
            ext_type: "streamable_http".into(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: None,
            envs: HashMap::new(),
            env_keys: vec![],
            code: None,
            dependencies: None,
        },
    ];

    let recipe = profile_to_recipe(&profile);
    let extensions = recipe.extensions.expect("valid builtin should remain");
    assert_eq!(extensions.len(), 1);
    match &extensions[0] {
        ExtensionConfig::Builtin { name, timeout, .. } => {
            assert_eq!(name, "builtin");
            assert_eq!(*timeout, Some(30));
        }
        other => unreachable!("expected Builtin, got {:?}", other),
    }
}

#[test]
fn ext_ref_to_config_requires_streamable_http_uri() {
    let missing_http_uri = ExtensionRef {
        name: "http".into(),
        ext_type: "streamable_http".into(),
        cmd: None,
        args: vec![],
        uri: None,
        timeout: None,
        envs: HashMap::new(),
        env_keys: vec![],
        code: None,
        dependencies: None,
    };

    assert!(ext_ref_to_config(&missing_http_uri).is_none());
}

#[test]
fn config_to_ext_ref_preserves_stream_uri_and_sanitized_envs() {
    let config = ExtensionConfig::StreamableHttp {
        name: "remote".into(),
        description: String::new(),
        uri: "https://example.invalid/stream".into(),
        envs: Envs::new(HashMap::from([
            (
                "BASE_URL".to_string(),
                "https://example.invalid".to_string(),
            ),
            ("API_TOKEN".to_string(), "secret".to_string()),
        ])),
        env_keys: vec!["API_TOKEN".into()],
        headers: HashMap::new(),
        timeout: Some(20),
        bundled: None,
        available_tools: vec![],
    };

    let ext = config_to_ext_ref(&config).expect("stream config should map to profile");
    assert_eq!(ext.ext_type, "streamable_http");
    assert_eq!(ext.uri.as_deref(), Some("https://example.invalid/stream"));
    assert_eq!(ext.timeout, Some(20));
    assert_eq!(ext.env_keys, vec!["API_TOKEN".to_string()]);
    assert_eq!(
        ext.envs,
        HashMap::from([
            (
                "BASE_URL".to_string(),
                "https://example.invalid".to_string(),
            ),
            ("API_TOKEN".to_string(), "secret".to_string()),
        ])
    );
}

#[test]
fn config_to_ext_ref_skips_sse_extensions() {
    let sse = ExtensionConfig::Sse {
        name: "legacy-sse".into(),
        description: String::new(),
        uri: Some("https://example.invalid/sse".into()),
    };

    assert!(config_to_ext_ref(&sse).is_none());
}
