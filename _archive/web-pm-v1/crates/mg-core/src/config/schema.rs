use schemars::schema::RootSchema;

/// Generate JSON Schema for MgpmConfig (for IDE auto-completion)
pub fn generate_schema() -> RootSchema {
    schemars::schema_for!(super::MgpmConfig)
}

/// Write the JSON Schema to a file
pub fn write_schema(path: &std::path::Path) -> Result<(), String> {
    let schema = generate_schema();
    let json = serde_json::to_string_pretty(&schema)
        .map_err(|e| format!("failed to serialize schema: {e}"))?;
    std::fs::write(path, json)
        .map_err(|e| format!("failed to write schema: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_schema() {
        let schema = generate_schema();
        assert!(schema.schema.metadata.is_some());
        let title = schema.schema.metadata.as_ref().unwrap().title.as_deref();
        assert_eq!(title, Some("MgpmConfig"));
    }

    #[test]
    fn test_write_schema() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mg-schema.json");
        write_schema(&path).unwrap();
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("MgpmConfig"));
    }
}
