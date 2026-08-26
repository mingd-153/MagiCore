//! Unity project scaffolding.

use super::{render_template, TemplateContext};
use mgc_types::MgResult;
use std::path::Path;

/// Scaffold Unity project với Packages/manifest.json + Bootstrap.cs
pub async fn scaffold(context: TemplateContext, target_dir: &Path) -> MgResult<()> {
    let ctx_map = context.to_map();

    // Packages/manifest.json
    let packages_dir = target_dir.join("Packages");
    std::fs::create_dir_all(&packages_dir)?;

    let manifest = render_template(MANIFEST_TEMPLATE, &ctx_map);
    std::fs::write(packages_dir.join("manifest.json"), manifest)?;

    // Assets/Bootstrap.cs
    let assets_dir = target_dir.join("Assets");
    std::fs::create_dir_all(&assets_dir)?;

    let bootstrap = render_template(BOOTSTRAP_TEMPLATE, &ctx_map);
    std::fs::write(assets_dir.join("Bootstrap.cs"), bootstrap)?;

    // mgc.toml
    let mgc_toml = render_template(MGC_TEMPLATE, &ctx_map);
    std::fs::write(target_dir.join("mgc.toml"), mgc_toml)?;

    Ok(())
}

const MANIFEST_TEMPLATE: &str = r#"{
  "dependencies": {
    "com.unity.collab-proxy": "2.4.4",
    "com.unity.feature.development": "1.0.2",
    "com.unity.textmeshpro": "3.0.6",
    "com.unity.timeline": "1.8.7",
    "com.unity.ugui": "2.0.0",
    "com.unity.modules.ai": "1.0.0",
    "com.unity.modules.animation": "1.0.0"
  }
}
"#;

const BOOTSTRAP_TEMPLATE: &str = r#"using UnityEngine;

public class Bootstrap : MonoBehaviour
{
    void Start()
    {
        Debug.Log("Hello from {{project_name}}!");
    }
}
"#;

const MGC_TEMPLATE: &str = r#"name = "{{project_slug}}"
version = "0.1.0"
ecosystem = "game"

[game]
engine = "unity"
unity_version = "{{unity_version}}"

[execution]
architecture = "native-first"
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        TempDir::new().unwrap()
    }

    #[tokio::test]
    async fn test_scaffold_unity() {
        let tmp = tmp();
        let mut ctx = TemplateContext::new("my-unity-game", crate::engine::GameEngine::Unity);
        ctx.unity_version = Some("6000.0".to_string());

        scaffold(ctx, tmp.path()).await.unwrap();

        assert!(tmp.path().join("Packages/manifest.json").exists());
        assert!(tmp.path().join("Assets/Bootstrap.cs").exists());
        assert!(tmp.path().join("mgc.toml").exists());

        let bootstrap = std::fs::read_to_string(tmp.path().join("Assets/Bootstrap.cs")).unwrap();
        assert!(bootstrap.contains("my-unity-game"));
    }
}
