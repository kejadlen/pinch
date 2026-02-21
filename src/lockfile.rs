use std::collections::BTreeMap;

use color_eyre::eyre::{Result, WrapErr};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    #[serde(default)]
    pub plugins: BTreeMap<String, LockedPlugin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedPlugin {
    pub marketplace: String,
    pub path: String,
    pub rev: String,
}

pub fn load() -> Result<Lockfile> {
    let path = crate::paths::lockfile_path()?;
    let contents = fs_err::read_to_string(&path)?;
    let lockfile: Lockfile = serde_yml::from_str(&contents)
        .wrap_err_with(|| format!("failed to parse lockfile: {}", path.display()))?;
    Ok(lockfile)
}

pub fn save(lockfile: &Lockfile) -> Result<()> {
    let path = crate::paths::lockfile_path()?;
    let contents = serde_yml::to_string(lockfile).wrap_err("failed to serialize lockfile")?;
    fs_err::write(&path, contents)?;
    Ok(())
}
