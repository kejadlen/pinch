use std::path::PathBuf;

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

pub fn cache_dir() -> PathBuf {
    base_dirs()
        .get_cache_home()
        .expect("XDG cache home not set")
}

pub fn repos_dir() -> PathBuf {
    cache_dir().join("repos")
}

pub fn data_dir() -> PathBuf {
    base_dirs().get_data_home().expect("XDG data home not set")
}

pub fn lockfile_path() -> PathBuf {
    data_dir().join("pinch-lock.yml")
}

pub fn skills_dir() -> PathBuf {
    home_dir().join(".claude").join("skills")
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .expect("could not determine home directory")
}
