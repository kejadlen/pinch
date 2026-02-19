use std::collections::BTreeMap;

use color_eyre::eyre::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    #[serde(default)]
    pub plugins: BTreeMap<String, LockedPlugin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedPlugin {
    pub src: String,
    pub path: String,
    pub rev: String,
}

pub fn load() -> Result<Lockfile> {
    let path = crate::paths::lockfile_path()?;
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read lockfile: {}", path.display()))?;
    let lockfile: Lockfile = serde_yml::from_str(&contents)
        .with_context(|| format!("failed to parse lockfile: {}", path.display()))?;
    Ok(lockfile)
}

pub fn save(lockfile: &Lockfile) -> Result<()> {
    let path = crate::paths::lockfile_path()?;
    let contents = serde_yml::to_string(lockfile).context("failed to serialize lockfile")?;
    std::fs::write(&path, contents)
        .with_context(|| format!("failed to write lockfile: {}", path.display()))?;
    Ok(())
}
