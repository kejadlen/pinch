use std::collections::BTreeMap;
use std::collections::HashSet;

use color_eyre::eyre::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Plugin {
    pub name: String,
    pub src: String,
    pub version: String,
    /// Path to marketplace.json within the repo.
    /// Defaults to `.claude-plugin/marketplace.json`.
    pub marketplace: Option<String>,
}

pub fn load() -> Result<Vec<Plugin>> {
    let config_dir = crate::paths::config_dir();

    if !config_dir.is_dir() {
        bail!(
            "config directory not found: {}\ncreate it and add .yml config files",
            config_dir.display()
        );
    }

    let mut entries: Vec<_> = fs_err::read_dir(&config_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "yml"))
        .collect();

    entries.sort_by_key(|e| e.file_name());

    if entries.is_empty() {
        bail!("no .yml files found in {}", config_dir.display());
    }

    let mut plugins = Vec::new();
    let mut seen_names = HashSet::new();

    for entry in &entries {
        let path = entry.path();
        let contents = fs_err::read_to_string(&path)?;
        let file: ConfigFile = serde_yml::from_str(&contents)
            .with_context(|| format!("failed to parse {}", path.display()))?;

        // Marketplaces are scoped to this file
        let marketplaces = &file.marketplaces;

        for (name, entry) in file.plugins {
            if !seen_names.insert(name.clone()) {
                bail!(
                    "duplicate plugin name '{}' (found in {})",
                    name,
                    path.display()
                );
            }

            let (src, version, marketplace_path) = match entry.marketplace {
                Some(mkt_name) => {
                    let mkt = marketplaces.get(&mkt_name).ok_or_else(|| {
                        color_eyre::eyre::eyre!(
                            "plugin '{}' references unknown marketplace '{}' (in {})",
                            name,
                            mkt_name,
                            path.display()
                        )
                    })?;
                    (
                        mkt.src.clone(),
                        entry.version.unwrap_or_else(|| mkt.version.clone()),
                        mkt.manifest.clone(),
                    )
                }
                None => {
                    let src = entry.src.ok_or_else(|| {
                        color_eyre::eyre::eyre!(
                            "plugin '{}': must specify either 'src' or 'marketplace'",
                            name
                        )
                    })?;
                    let version = entry.version.ok_or_else(|| {
                        color_eyre::eyre::eyre!("plugin '{}': 'version' is required", name)
                    })?;
                    (src, version, None)
                }
            };

            plugins.push(Plugin {
                name,
                src,
                version,
                marketplace: marketplace_path,
            });
        }
    }

    Ok(plugins)
}

#[derive(Deserialize)]
struct MarketplaceEntry {
    src: String,
    version: String,
    manifest: Option<String>,
}

#[derive(Deserialize)]
struct PluginEntry {
    src: Option<String>,
    version: Option<String>,
    marketplace: Option<String>,
}

#[derive(Deserialize)]
struct ConfigFile {
    #[serde(default)]
    marketplaces: BTreeMap<String, MarketplaceEntry>,
    #[serde(default)]
    plugins: BTreeMap<String, PluginEntry>,
}
