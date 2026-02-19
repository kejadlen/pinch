use std::path::Path;

use color_eyre::eyre::{Context, Result, bail, eyre};
use serde::Deserialize;

const DEFAULT_PATH: &str = ".claude-plugin/marketplace.json";

#[derive(Debug, Deserialize)]
pub struct Marketplace {
    pub plugins: Vec<MarketplacePlugin>,
}

#[derive(Debug, Deserialize)]
pub struct MarketplacePlugin {
    pub name: String,
    pub source: PluginSource,
}

/// A plugin source in marketplace.json.
///
/// Can be a relative path string (e.g. `"./plugins/my-plugin"`) or an object
/// describing an external source (github, git url, npm, pip). Pinch only
/// supports relative paths since it clones the marketplace repo directly.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum PluginSource {
    /// Relative path within the marketplace repo.
    Path(String),
    /// External source (github, url, npm, pip) — not supported by pinch.
    External(serde_json::Value),
}

/// Find the source path for a plugin by name in the marketplace.json.
pub fn find_plugin_source(
    repo_path: &Path,
    plugin_name: &str,
    marketplace_override: Option<&str>,
) -> Result<String> {
    let manifest_path = repo_path.join(marketplace_override.unwrap_or(DEFAULT_PATH));

    let contents = fs_err::read_to_string(&manifest_path)?;

    let marketplace: Marketplace = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;

    let plugin = marketplace
        .plugins
        .iter()
        .find(|mp| mp.name == plugin_name)
        .ok_or_else(|| {
            eyre!(
                "plugin '{}' not found in {}",
                plugin_name,
                manifest_path.display()
            )
        })?;

    match &plugin.source {
        PluginSource::Path(path) => Ok(path.clone()),
        PluginSource::External(value) => {
            bail!(
                "plugin '{}': external source {} — pinch only supports relative path sources",
                plugin_name,
                value
            );
        }
    }
}
