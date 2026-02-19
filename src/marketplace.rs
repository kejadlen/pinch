use std::path::Path;

use color_eyre::eyre::{Context, Result, bail};
use serde::Deserialize;

const DEFAULT_PATH: &str = ".claude-plugin/marketplace.json";

#[derive(Debug, Deserialize)]
pub struct Marketplace {
    pub plugins: Vec<MarketplacePlugin>,
}

#[derive(Debug, Deserialize)]
pub struct MarketplacePlugin {
    pub name: String,
    pub source: serde_json::Value,
}

/// Find the source path for a plugin by name in the marketplace.json.
pub fn find_plugin_source(
    repo_path: &Path,
    plugin_name: &str,
    marketplace_override: Option<&str>,
) -> Result<String> {
    let manifest_path = repo_path.join(marketplace_override.unwrap_or(DEFAULT_PATH));

    let contents = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;

    let marketplace: Marketplace = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;

    for mp in &marketplace.plugins {
        if mp.name == plugin_name {
            return match &mp.source {
                serde_json::Value::String(s) => Ok(s.clone()),
                _ => bail!(
                    "plugin '{}': unsupported source type in {} (only string paths are supported)",
                    plugin_name,
                    manifest_path.display()
                ),
            };
        }
    }

    bail!(
        "plugin '{}' not found in {}",
        plugin_name,
        manifest_path.display()
    )
}
