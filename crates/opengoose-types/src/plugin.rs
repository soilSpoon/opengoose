/// Shared runtime snapshot for an installed plugin.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginStatusSnapshot {
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub source_path: String,
    pub capabilities: Vec<String>,
    pub runtime_initialized: bool,
    pub registered_skills: Vec<String>,
    pub missing_skills: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_note: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::PluginStatusSnapshot;

    #[test]
    fn snapshot_serializes_with_camel_case_fields() {
        let json = serde_json::to_value(PluginStatusSnapshot {
            name: "file-tools".into(),
            version: "1.0.0".into(),
            enabled: true,
            source_path: "/tmp/file-tools".into(),
            capabilities: vec!["skill".into()],
            runtime_initialized: false,
            registered_skills: vec![],
            missing_skills: vec!["file-tools/tool".into()],
            runtime_note: Some("missing declared skill registration".into()),
        })
        .expect("snapshot should serialize");

        assert_eq!(json["runtimeInitialized"], false);
        assert_eq!(json["missingSkills"][0], "file-tools/tool");
        assert_eq!(json["runtimeNote"], "missing declared skill registration");
    }
}
