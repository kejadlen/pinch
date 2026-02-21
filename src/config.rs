use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;

use color_eyre::eyre::{Result, WrapErr, bail};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Plugin {
    pub name: String,
    pub marketplace: String,
    pub src: String,
    pub version: String,
    /// Path to marketplace.json within the repo.
    /// Defaults to `.claude-plugin/marketplace.json`.
    pub manifest: Option<String>,
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
    let mut seen_marketplaces: HashMap<String, String> = HashMap::new();

    for entry in &entries {
        let path = entry.path();
        let contents = fs_err::read_to_string(&path)?;
        let file: ConfigFile = serde_yml::from_str(&contents)
            .wrap_err_with(|| format!("failed to parse {}", path.display()))?;

        for (mkt_name, mkt) in file.marketplaces {
            if let Some(existing_src) = seen_marketplaces.get(&mkt_name) {
                if existing_src != &mkt.src {
                    bail!(
                        "marketplace '{}' has conflicting sources:\n  {}\n  {}\n(found in {})",
                        mkt_name,
                        existing_src,
                        mkt.src,
                        path.display()
                    );
                }
            } else {
                seen_marketplaces.insert(mkt_name.clone(), mkt.src.clone());
            }

            for name in mkt.plugins {
                if !seen_names.insert(name.clone()) {
                    bail!(
                        "duplicate plugin name '{}' (found in {})",
                        name,
                        path.display()
                    );
                }

                plugins.push(Plugin {
                    name,
                    marketplace: mkt_name.clone(),
                    src: mkt.src.clone(),
                    version: mkt.version.clone(),
                    manifest: mkt.manifest.clone(),
                });
            }
        }
    }

    Ok(plugins)
}

#[derive(Deserialize)]
struct MarketplaceEntry {
    src: String,
    version: String,
    manifest: Option<String>,
    #[serde(default)]
    plugins: Vec<String>,
}

#[derive(Deserialize)]
struct ConfigFile {
    #[serde(default)]
    marketplaces: BTreeMap<String, MarketplaceEntry>,
}
