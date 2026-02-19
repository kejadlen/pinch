use color_eyre::eyre::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    #[serde(default)]
    pub plugins: Vec<LockedPlugin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedPlugin {
    pub name: String,
    pub src: String,
    pub path: String,
    pub rev: String,
}

pub fn load() -> Result<Lockfile> {
    let path = crate::paths::lockfile_path();
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read lockfile: {}", path.display()))?;
    let lockfile: Lockfile = serde_yml::from_str(&contents)
        .with_context(|| format!("failed to parse lockfile: {}", path.display()))?;
    Ok(lockfile)
}

pub fn save(lockfile: &Lockfile) -> Result<()> {
    let path = crate::paths::lockfile_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create data dir: {}", parent.display()))?;
    }
    let mut sorted = lockfile.clone();
    sorted.plugins.sort_by(|a, b| a.name.cmp(&b.name));
    let contents = serde_yml::to_string(&sorted).context("failed to serialize lockfile")?;
    std::fs::write(&path, contents)
        .with_context(|| format!("failed to write lockfile: {}", path.display()))?;
    Ok(())
}
