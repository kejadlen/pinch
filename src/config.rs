use std::collections::HashSet;

use color_eyre::eyre::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    #[serde(default)]
    pub plugins: Vec<Plugin>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Plugin {
    pub name: String,
    pub src: String,
    pub path: String,
    pub r#ref: String,
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

        for plugin in file.plugins {
            if !seen_names.insert(plugin.name.clone()) {
                bail!(
                    "duplicate plugin name '{}' (found in {})",
                    plugin.name,
                    path.display()
                );
            }
            plugins.push(plugin);
        }
    }

    Ok(Config { plugins })
}
