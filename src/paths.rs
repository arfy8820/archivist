use crate::types::Config;
use crate::util::sanitize_label;
use chrono::Local;
use std::env;
use std::path::{Path, PathBuf};

const APP_NAME: &str = "archivist";

pub fn config_directory() -> PathBuf {
    match env::var_os("ARCHIVIST_CONFIG_DIR") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => user_home_directory().join(".config").join(APP_NAME),
    }
}

pub fn config_file() -> PathBuf {
    config_directory().join("config.toml")
}

pub fn logs_directory() -> PathBuf {
    config_directory().join("logs")
}

pub fn youtube_archive_file(config: &Config, label: &str) -> PathBuf {
    Path::new(&config.youtube_dir)
        .join(label)
        .join(".download-archive.txt")
}

pub fn podcast_archive_file(config: &Config, label: &str) -> PathBuf {
    Path::new(&config.podcast_dir)
        .join(label)
        .join("archive.json")
}

pub fn sync_log_file(label: &str) -> PathBuf {
    logs_directory().join(format!(
        "sync-{}-{}.log",
        sanitize_label(label),
        Local::now().format("%Y%m%d-%H%M%S")
    ))
}

fn user_home_directory() -> PathBuf {
    env::home_dir()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
