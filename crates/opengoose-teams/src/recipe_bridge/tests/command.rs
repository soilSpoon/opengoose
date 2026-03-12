use std::collections::HashMap;

use goose::agents::extension::{Envs, ExtensionConfig};

use opengoose_profiles::ExtensionRef;

use super::super::{config_to_ext_ref, ext_ref_to_config, profile_to_recipe, recipe_to_profile};
use super::empty_profile;

#[test]
fn inline_python_round_trip() {
    let profile = opengoose_profiles::AgentProfile {
        version: "1.0.0".into(),
        title: "py-agent".into(),
        description: None,
        instructions: Some("Run python".into()),
        prompt: None,
        extensions: vec![ExtensionRef {
            name: "analyzer".into(),
            ext_type: "inline_python".into(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: Some(60),
            envs: HashMap::new(),
            env_keys: vec![],
            code: Some("print('hello')".into()),
            dependencies: Some(vec!["numpy".into()]),
        }],
        skills: vec![],
        settings: None,
        activities: None,
        response: None,
        sub_recipes: None,
        parameters: None,
    };

    let recipe = profile_to_recipe(&profile);
    let exts = recipe.extensions.as_ref().unwrap();
    assert_eq!(exts.len(), 1);
    match &exts[0] {
        ExtensionConfig::InlinePython {
            name,
            code,
            dependencies,
            ..
        } => {
            assert_eq!(name, "analyzer");
            assert_eq!(code, "print('hello')");
            assert_eq!(dependencies.as_ref().unwrap(), &vec!["numpy".to_string()]);
        }
        other => unreachable!("expected InlinePython, got {:?}", other),
    }

    let back = recipe_to_profile(&recipe);
    assert_eq!(back.extensions[0].ext_type, "inline_python");
    assert_eq!(back.extensions[0].code.as_deref(), Some("print('hello')"));
}

#[test]
fn platform_extension_round_trip() {
    let profile = opengoose_profiles::AgentProfile {
        version: "1.0.0".into(),
        title: "summon-agent".into(),
        description: None,
        instructions: Some("Use summon".into()),
        prompt: None,
        extensions: vec![ExtensionRef {
            name: "summon".into(),
            ext_type: "platform".into(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: None,
            envs: HashMap::new(),
            env_keys: vec![],
            code: None,
            dependencies: None,
        }],
        skills: vec![],
        settings: None,
        activities: None,
        response: None,
        sub_recipes: None,
        parameters: None,
    };

    let recipe = profile_to_recipe(&profile);
    match &recipe.extensions.as_ref().unwrap()[0] {
        ExtensionConfig::Platform { name, .. } => assert_eq!(name, "summon"),
        other => unreachable!("expected Platform, got {:?}", other),
    }

    let back = recipe_to_profile(&recipe);
    assert_eq!(back.extensions[0].ext_type, "platform");
    assert_eq!(back.extensions[0].name, "summon");
}

#[test]
fn profile_to_recipe_skips_incomplete_command_extensions_and_unsupported_types() {
    let mut profile = empty_profile("invalid-command-extensions");
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
            name: "stdio-missing-cmd".into(),
            ext_type: "stdio".into(),
            cmd: None,
            args: vec!["--flag".into()],
            uri: None,
            timeout: None,
            envs: HashMap::new(),
            env_keys: vec![],
            code: None,
            dependencies: None,
        },
        ExtensionRef {
            name: "python-missing-code".into(),
            ext_type: "inline_python".into(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: None,
            envs: HashMap::new(),
            env_keys: vec![],
            code: None,
            dependencies: None,
        },
        ExtensionRef {
            name: "unsupported".into(),
            ext_type: "frontend".into(),
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
fn ext_ref_to_config_requires_stdio_command() {
    let missing_stdio_cmd = ExtensionRef {
        name: "stdio".into(),
        ext_type: "stdio".into(),
        cmd: None,
        args: vec![],
        uri: None,
        timeout: None,
        envs: HashMap::new(),
        env_keys: vec![],
        code: None,
        dependencies: None,
    };

    assert!(ext_ref_to_config(&missing_stdio_cmd).is_none());
}

#[test]
fn ext_ref_to_config_requires_inline_python_code() {
    let missing_python_code = ExtensionRef {
        name: "python".into(),
        ext_type: "inline_python".into(),
        cmd: None,
        args: vec![],
        uri: None,
        timeout: None,
        envs: HashMap::new(),
        env_keys: vec![],
        code: None,
        dependencies: None,
    };

    assert!(ext_ref_to_config(&missing_python_code).is_none());
}

#[test]
fn ext_ref_to_config_rejects_unsupported_extensions() {
    let unsupported = ExtensionRef {
        name: "unsupported".into(),
        ext_type: "frontend".into(),
        cmd: None,
        args: vec![],
        uri: None,
        timeout: None,
        envs: HashMap::new(),
        env_keys: vec![],
        code: None,
        dependencies: None,
    };

    assert!(ext_ref_to_config(&unsupported).is_none());
}

#[test]
fn config_to_ext_ref_preserves_sanitized_envs_and_env_keys() {
    let config = ExtensionConfig::Stdio {
        name: "tool".into(),
        description: String::new(),
        cmd: "tool-bin".into(),
        args: vec!["--json".into()],
        envs: Envs::new(HashMap::from([
            ("PATH".to_string(), "/tmp/bin".to_string()),
            ("API_KEY".to_string(), "secret".to_string()),
        ])),
        env_keys: vec!["API_KEY".into()],
        timeout: Some(45),
        bundled: None,
        available_tools: vec![],
    };

    let ext = config_to_ext_ref(&config).expect("stdio config should map to profile");
    assert_eq!(ext.ext_type, "stdio");
    assert_eq!(ext.cmd.as_deref(), Some("tool-bin"));
    assert_eq!(ext.args, vec!["--json".to_string()]);
    assert_eq!(ext.timeout, Some(45));
    assert_eq!(ext.env_keys, vec!["API_KEY".to_string()]);
    assert_eq!(
        ext.envs,
        HashMap::from([("API_KEY".to_string(), "secret".to_string())])
    );
}
