//! Tests for extension type conversion (ext_ref_to_config / config_to_ext_ref).

use std::collections::HashMap;

use goose::agents::extension::{Envs, ExtensionConfig};

use opengoose_profiles::ExtensionRef;

use super::super::extensions::{config_to_ext_ref, ext_ref_to_config};

fn builtin_ext(name: &str) -> ExtensionRef {
    ExtensionRef {
        name: name.into(),
        ext_type: "builtin".into(),
        cmd: None,
        args: vec![],
        uri: None,
        timeout: Some(300),
        envs: HashMap::new(),
        env_keys: vec![],
        code: None,
        dependencies: None,
    }
}

// ── ext_ref_to_config ─────────────────────────────────────────────────

#[test]
fn builtin_ext_converts() {
    let ext = builtin_ext("developer");
    let config = ext_ref_to_config(&ext).unwrap();
    match config {
        ExtensionConfig::Builtin {
            name,
            timeout,
            bundled,
            ..
        } => {
            assert_eq!(name, "developer");
            assert_eq!(timeout, Some(300));
            assert_eq!(bundled, Some(true));
        }
        _ => panic!("expected Builtin config"),
    }
}

#[test]
fn stdio_ext_converts() {
    let ext = ExtensionRef {
        name: "my-tool".into(),
        ext_type: "stdio".into(),
        cmd: Some("my-tool-bin".into()),
        args: vec!["--verbose".into()],
        uri: None,
        timeout: Some(60),
        envs: HashMap::from([("KEY".into(), "val".into())]),
        env_keys: vec!["SECRET".into()],
        code: None,
        dependencies: None,
    };
    let config = ext_ref_to_config(&ext).unwrap();
    match config {
        ExtensionConfig::Stdio {
            name,
            cmd,
            args,
            env_keys,
            timeout,
            ..
        } => {
            assert_eq!(name, "my-tool");
            assert_eq!(cmd, "my-tool-bin");
            assert_eq!(args, vec!["--verbose"]);
            assert_eq!(env_keys, vec!["SECRET"]);
            assert_eq!(timeout, Some(60));
        }
        _ => panic!("expected Stdio config"),
    }
}

#[test]
fn stdio_ext_requires_cmd() {
    let ext = ExtensionRef {
        name: "no-cmd".into(),
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
    assert!(ext_ref_to_config(&ext).is_none());
}

#[test]
fn streamable_http_ext_converts() {
    let ext = ExtensionRef {
        name: "remote-mcp".into(),
        ext_type: "streamable_http".into(),
        cmd: None,
        args: vec![],
        uri: Some("https://mcp.example.com".into()),
        timeout: Some(120),
        envs: HashMap::new(),
        env_keys: vec!["API_KEY".into()],
        code: None,
        dependencies: None,
    };
    let config = ext_ref_to_config(&ext).unwrap();
    match config {
        ExtensionConfig::StreamableHttp {
            name,
            uri,
            env_keys,
            timeout,
            ..
        } => {
            assert_eq!(name, "remote-mcp");
            assert_eq!(uri, "https://mcp.example.com");
            assert_eq!(env_keys, vec!["API_KEY"]);
            assert_eq!(timeout, Some(120));
        }
        _ => panic!("expected StreamableHttp config"),
    }
}

#[test]
fn streamable_http_requires_uri() {
    let ext = ExtensionRef {
        name: "no-uri".into(),
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
    assert!(ext_ref_to_config(&ext).is_none());
}

#[test]
fn platform_ext_converts() {
    let ext = ExtensionRef {
        name: "platform-ext".into(),
        ext_type: "platform".into(),
        cmd: None,
        args: vec![],
        uri: None,
        timeout: None,
        envs: HashMap::new(),
        env_keys: vec![],
        code: None,
        dependencies: None,
    };
    let config = ext_ref_to_config(&ext).unwrap();
    match &config {
        ExtensionConfig::Platform { name, .. } => assert_eq!(name, "platform-ext"),
        _ => panic!("expected Platform config"),
    }
}

#[test]
fn inline_python_ext_converts() {
    let ext = ExtensionRef {
        name: "py-script".into(),
        ext_type: "inline_python".into(),
        cmd: None,
        args: vec![],
        uri: None,
        timeout: Some(30),
        envs: HashMap::new(),
        env_keys: vec![],
        code: Some("print('hello')".into()),
        dependencies: Some(vec!["requests".into()]),
    };
    let config = ext_ref_to_config(&ext).unwrap();
    match config {
        ExtensionConfig::InlinePython {
            name,
            code,
            timeout,
            dependencies,
            ..
        } => {
            assert_eq!(name, "py-script");
            assert_eq!(code, "print('hello')");
            assert_eq!(timeout, Some(30));
            assert_eq!(dependencies, Some(vec!["requests".to_string()]));
        }
        _ => panic!("expected InlinePython config"),
    }
}

#[test]
fn inline_python_requires_code() {
    let ext = ExtensionRef {
        name: "no-code".into(),
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
    assert!(ext_ref_to_config(&ext).is_none());
}

#[test]
fn unknown_ext_type_returns_none() {
    let ext = ExtensionRef {
        name: "mystery".into(),
        ext_type: "quantum_entanglement".into(),
        cmd: None,
        args: vec![],
        uri: None,
        timeout: None,
        envs: HashMap::new(),
        env_keys: vec![],
        code: None,
        dependencies: None,
    };
    assert!(ext_ref_to_config(&ext).is_none());
}

// ── config_to_ext_ref ─────────────────────────────────────────────────

#[test]
fn builtin_config_to_ext_ref() {
    let config = ExtensionConfig::Builtin {
        name: "developer".into(),
        description: "Dev tools".into(),
        display_name: Some("Developer".into()),
        timeout: Some(300),
        bundled: Some(true),
        available_tools: vec![],
    };
    let ext = config_to_ext_ref(&config).unwrap();
    assert_eq!(ext.name, "developer");
    assert_eq!(ext.ext_type, "builtin");
    assert_eq!(ext.timeout, Some(300));
    assert!(ext.cmd.is_none());
}

#[test]
fn stdio_config_to_ext_ref() {
    let config = ExtensionConfig::Stdio {
        name: "tool".into(),
        description: "A tool".into(),
        cmd: "tool-bin".into(),
        args: vec!["--flag".into()],
        envs: Envs::new(HashMap::from([("K".into(), "V".into())])),
        env_keys: vec!["SECRET".into()],
        timeout: Some(60),
        bundled: None,
        available_tools: vec![],
    };
    let ext = config_to_ext_ref(&config).unwrap();
    assert_eq!(ext.name, "tool");
    assert_eq!(ext.ext_type, "stdio");
    assert_eq!(ext.cmd, Some("tool-bin".into()));
    assert_eq!(ext.args, vec!["--flag"]);
    assert_eq!(ext.env_keys, vec!["SECRET"]);
    assert_eq!(ext.timeout, Some(60));
}

#[test]
fn streamable_http_config_to_ext_ref() {
    let config = ExtensionConfig::StreamableHttp {
        name: "remote".into(),
        description: "Remote MCP".into(),
        uri: "https://example.com/mcp".into(),
        envs: Envs::new(HashMap::new()),
        env_keys: vec![],
        headers: HashMap::new(),
        timeout: None,
        bundled: None,
        available_tools: vec![],
    };
    let ext = config_to_ext_ref(&config).unwrap();
    assert_eq!(ext.name, "remote");
    assert_eq!(ext.ext_type, "streamable_http");
    assert_eq!(ext.uri, Some("https://example.com/mcp".into()));
    assert!(ext.timeout.is_none());
}

#[test]
fn platform_config_to_ext_ref() {
    let config = ExtensionConfig::Platform {
        name: "plat".into(),
        description: "Platform ext".into(),
        display_name: None,
        bundled: None,
        available_tools: vec![],
    };
    let ext = config_to_ext_ref(&config).unwrap();
    assert_eq!(ext.name, "plat");
    assert_eq!(ext.ext_type, "platform");
    assert!(ext.timeout.is_none());
}

#[test]
fn inline_python_config_to_ext_ref() {
    let config = ExtensionConfig::InlinePython {
        name: "pyscript".into(),
        description: "Python script".into(),
        code: "import os".into(),
        timeout: Some(45),
        dependencies: Some(vec!["numpy".into()]),
        available_tools: vec![],
    };
    let ext = config_to_ext_ref(&config).unwrap();
    assert_eq!(ext.name, "pyscript");
    assert_eq!(ext.ext_type, "inline_python");
    assert_eq!(ext.code, Some("import os".into()));
    assert_eq!(ext.timeout, Some(45));
    assert_eq!(ext.dependencies, Some(vec!["numpy".to_string()]));
}

// ── round-trip: ext_ref → config → ext_ref ────────────────────────────

#[test]
fn builtin_round_trip() {
    let original = builtin_ext("dev");
    let config = ext_ref_to_config(&original).unwrap();
    let back = config_to_ext_ref(&config).unwrap();
    assert_eq!(back.name, original.name);
    assert_eq!(back.ext_type, original.ext_type);
    assert_eq!(back.timeout, original.timeout);
}

#[test]
fn stdio_round_trip() {
    let original = ExtensionRef {
        name: "tool".into(),
        ext_type: "stdio".into(),
        cmd: Some("cmd".into()),
        args: vec!["a".into(), "b".into()],
        uri: None,
        timeout: Some(10),
        envs: HashMap::new(),
        env_keys: vec!["KEY".into()],
        code: None,
        dependencies: None,
    };
    let config = ext_ref_to_config(&original).unwrap();
    let back = config_to_ext_ref(&config).unwrap();
    assert_eq!(back.name, original.name);
    assert_eq!(back.ext_type, original.ext_type);
    assert_eq!(back.cmd, original.cmd);
    assert_eq!(back.args, original.args);
    assert_eq!(back.env_keys, original.env_keys);
    assert_eq!(back.timeout, original.timeout);
}
