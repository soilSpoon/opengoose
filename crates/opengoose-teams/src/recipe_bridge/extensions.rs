//! Conversion between `ExtensionRef` (profile) and `ExtensionConfig` (recipe).

use std::collections::HashMap;

use goose::agents::extension::{Envs, ExtensionConfig};

use opengoose_profiles::ExtensionRef;

/// Convert an `ExtensionRef` into a Goose `ExtensionConfig`.
///
/// Returns `None` for unsupported extension types or when required fields
/// (e.g. `cmd` for stdio, `uri` for streamable_http) are missing.
pub fn ext_ref_to_config(ext: &ExtensionRef) -> Option<ExtensionConfig> {
    match ext.ext_type.as_str() {
        "builtin" => Some(ExtensionConfig::Builtin {
            name: ext.name.clone(),
            description: String::new(),
            display_name: None,
            timeout: ext.timeout,
            bundled: Some(true),
            available_tools: vec![],
        }),
        "stdio" => {
            let cmd = ext.cmd.as_ref()?.clone();
            Some(ExtensionConfig::Stdio {
                name: ext.name.clone(),
                description: String::new(),
                cmd,
                args: ext.args.clone(),
                envs: Envs::new(ext.envs.clone()),
                env_keys: ext.env_keys.clone(),
                timeout: ext.timeout,
                bundled: None,
                available_tools: vec![],
            })
        }
        "streamable_http" => {
            let uri = ext.uri.as_ref()?.clone();
            Some(ExtensionConfig::StreamableHttp {
                name: ext.name.clone(),
                description: String::new(),
                uri,
                envs: Envs::new(ext.envs.clone()),
                env_keys: ext.env_keys.clone(),
                headers: HashMap::new(),
                timeout: ext.timeout,
                bundled: None,
                available_tools: vec![],
            })
        }
        "platform" => Some(ExtensionConfig::Platform {
            name: ext.name.clone(),
            description: String::new(),
            display_name: None,
            bundled: None,
            available_tools: vec![],
        }),
        "inline_python" => {
            let code = ext.code.as_ref()?.clone();
            Some(ExtensionConfig::InlinePython {
                name: ext.name.clone(),
                description: String::new(),
                code,
                timeout: ext.timeout,
                dependencies: ext.dependencies.clone(),
                available_tools: vec![],
            })
        }
        _ => None,
    }
}

pub fn config_to_ext_ref(config: &ExtensionConfig) -> Option<ExtensionRef> {
    match config {
        ExtensionConfig::Builtin { name, timeout, .. } => Some(ExtensionRef {
            name: name.clone(),
            ext_type: "builtin".into(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: *timeout,
            envs: HashMap::new(),
            env_keys: vec![],
            code: None,
            dependencies: None,
        }),
        ExtensionConfig::Stdio {
            name,
            cmd,
            args,
            envs,
            env_keys,
            timeout,
            ..
        } => Some(ExtensionRef {
            name: name.clone(),
            ext_type: "stdio".into(),
            cmd: Some(cmd.clone()),
            args: args.clone(),
            uri: None,
            timeout: *timeout,
            envs: envs.get_env(),
            env_keys: env_keys.clone(),
            code: None,
            dependencies: None,
        }),
        ExtensionConfig::StreamableHttp {
            name,
            uri,
            envs,
            env_keys,
            timeout,
            ..
        } => Some(ExtensionRef {
            name: name.clone(),
            ext_type: "streamable_http".into(),
            cmd: None,
            args: vec![],
            uri: Some(uri.clone()),
            timeout: *timeout,
            envs: envs.get_env(),
            env_keys: env_keys.clone(),
            code: None,
            dependencies: None,
        }),
        ExtensionConfig::Platform { name, .. } => Some(ExtensionRef {
            name: name.clone(),
            ext_type: "platform".into(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: None,
            envs: HashMap::new(),
            env_keys: vec![],
            code: None,
            dependencies: None,
        }),
        ExtensionConfig::InlinePython {
            name,
            code,
            timeout,
            dependencies,
            ..
        } => Some(ExtensionRef {
            name: name.clone(),
            ext_type: "inline_python".into(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: *timeout,
            envs: HashMap::new(),
            env_keys: vec![],
            code: Some(code.clone()),
            dependencies: dependencies.clone(),
        }),
        // Sse and Frontend are not mapped to profiles
        _ => None,
    }
}
