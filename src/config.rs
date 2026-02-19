use std::collections::BTreeMap;
use std::collections::HashSet;

use color_eyre::eyre::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Plugin {
    pub name: String,
    pub src: String,
    pub r#ref: String,
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

    let mut entries: Vec<_> = std::fs::read_dir(&config_dir)
        .with_context(|| format!("failed to read config dir: {}", config_dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "yml"))
        .collect();

    entries.sort_by_key(|e| e.file_name());

    if entries.is_empty() {
        bail!("no .yml files found in {}", config_dir.display());
    }

    let mut marketplaces: BTreeMap<String, MarketplaceEntry> = BTreeMap::new();
    let mut plugins = Vec::new();
    let mut seen_names = HashSet::new();

    // First pass: collect all marketplaces
    for entry in &entries {
        let path = entry.path();
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let file: ConfigFile = serde_yml::from_str(&contents)
            .with_context(|| format!("failed to parse {}", path.display()))?;

        for (name, mkt) in file.marketplaces {
            if marketplaces.contains_key(&name) {
                bail!(
                    "duplicate marketplace '{}' (found in {})",
                    name,
                    path.display()
                );
            }
            marketplaces.insert(name, mkt);
        }
    }

    // Second pass: resolve plugins
    for entry in &entries {
        let path = entry.path();
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let file: ConfigFile = serde_yml::from_str(&contents)
            .with_context(|| format!("failed to parse {}", path.display()))?;

        for (name, entry) in file.plugins {
            if !seen_names.insert(name.clone()) {
                bail!(
                    "duplicate plugin name '{}' (found in {})",
                    name,
                    path.display()
                );
            }

            let (src, r#ref, marketplace_path) = match entry.marketplace {
                Some(mkt_name) => {
                    let mkt = marketplaces.get(&mkt_name).ok_or_else(|| {
                        color_eyre::eyre::eyre!(
                            "plugin '{}' references unknown marketplace '{}'",
                            name,
                            mkt_name
                        )
                    })?;
                    (
                        mkt.src.clone(),
                        entry.r#ref.unwrap_or_else(|| mkt.r#ref.clone()),
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
                    let r#ref = entry.r#ref.ok_or_else(|| {
                        color_eyre::eyre::eyre!("plugin '{}': 'ref' is required", name)
                    })?;
                    (src, r#ref, None)
                }
            };

            plugins.push(Plugin {
                name,
                src,
                r#ref,
                marketplace: marketplace_path,
            });
        }
    }

    Ok(plugins)
}

#[derive(Deserialize)]
struct MarketplaceEntry {
    src: String,
    r#ref: String,
    /// Override path to marketplace.json within the repo.
    manifest: Option<String>,
}

#[derive(Deserialize)]
struct PluginEntry {
    /// Git repo URL. Required if no marketplace.
    src: Option<String>,
    /// Git ref. Required if no marketplace (optional override with marketplace).
    r#ref: Option<String>,
    /// Name of a marketplace defined in the config.
    marketplace: Option<String>,
}

#[derive(Deserialize)]
struct ConfigFile {
    #[serde(default)]
    marketplaces: BTreeMap<String, MarketplaceEntry>,
    #[serde(default)]
    plugins: BTreeMap<String, PluginEntry>,
}
