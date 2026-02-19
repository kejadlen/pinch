use std::collections::BTreeMap;
use std::collections::HashSet;

use color_eyre::eyre::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    #[serde(default)]
    pub plugins: BTreeMap<String, PluginEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PluginEntry {
    pub src: String,
    pub r#ref: String,
    /// Override path to marketplace.json within the repo.
    /// Defaults to `.claude-plugin/marketplace.json`.
    pub marketplace: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Plugin {
    pub name: String,
    pub src: String,
    pub r#ref: String,
    pub marketplace: Option<String>,
}

#[derive(Debug)]
pub struct Config {
    pub plugins: Vec<Plugin>,
}

pub fn load() -> Result<Config> {
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

    let mut plugins = Vec::new();
    let mut seen_names = HashSet::new();

    for entry in entries {
        let path = entry.path();
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let file: ConfigFile = serde_yml::from_str(&contents)
            .with_context(|| format!("failed to parse {}", path.display()))?;

        for (name, plugin) in file.plugins {
            if !seen_names.insert(name.clone()) {
                bail!(
                    "duplicate plugin name '{}' (found in {})",
                    name,
                    path.display()
                );
            }
            plugins.push(Plugin {
                name,
                src: plugin.src,
                r#ref: plugin.r#ref,
                marketplace: plugin.marketplace,
            });
        }
    }

    Ok(Config { plugins })
}
