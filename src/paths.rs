use std::path::PathBuf;

use color_eyre::eyre::{Result, WrapErr};
use xdg::BaseDirectories;

fn base_dirs() -> BaseDirectories {
    BaseDirectories::with_prefix("pinch")
}

pub fn config_dir() -> PathBuf {
    base_dirs()
        .get_config_home()
        .expect("XDG config home not set")
        .join("conf.d")
}

pub fn repos_dir() -> Result<PathBuf> {
    base_dirs()
        .create_cache_directory("repos")
        .wrap_err("failed to create repos dir")
}

pub fn lockfile_path() -> Result<PathBuf> {
    base_dirs()
        .place_data_file("pinch-lock.yml")
        .wrap_err("failed to create data dir")
}

pub fn skills_dir() -> Result<PathBuf> {
    let path = home_dir().join(".claude").join("skills");
    fs_err::create_dir_all(&path)?;
    Ok(path)
}

pub fn commands_dir() -> Result<PathBuf> {
    let path = home_dir().join(".claude").join("commands");
    fs_err::create_dir_all(&path)?;
    Ok(path)
}

pub fn cache_dir() -> PathBuf {
    base_dirs()
        .get_cache_home()
        .expect("XDG cache home not set")
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .expect("could not determine home directory")
}
