use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType {
    YouTube,
    Podcast,
}

impl SourceType {
    pub fn name(self) -> &'static str {
        match self {
            Self::YouTube => "youtube",
            Self::Podcast => "podcast",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "youtube" | "yt" | "yt-dlp" => Some(Self::YouTube),
            "podcast" | "podcast-dl" | "rss" => Some(Self::Podcast),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub urls: Vec<String>,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub subdir: bool,
    #[serde(
        default,
        rename = "output_template",
        skip_serializing_if = "Option::is_none"
    )]
    pub output_template: Option<String>,
}

impl Target {
    pub fn source_type(&self) -> SourceType {
        match self.source.trim().to_ascii_lowercase().as_str() {
            "podcast" | "podcast-dl" | "rss" => SourceType::Podcast,
            "youtube" | "yt" | "yt-dlp" => SourceType::YouTube,
            _ => infer_source_type(self.primary_url()),
        }
    }

    pub fn primary_url(&self) -> &str {
        self.urls.first().map(String::as_str).unwrap_or("")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_youtube_dir_string")]
    pub youtube_dir: String,
    #[serde(default = "default_podcast_dir_string")]
    pub podcast_dir: String,
    #[serde(default = "default_youtube_template")]
    pub default_youtube_template: String,
    #[serde(default = "default_podcast_template")]
    pub default_podcast_template: String,
    #[serde(default)]
    pub targets: HashMap<String, Target>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yt_dlp_options: Option<toml::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub podcast_dl_options: Option<toml::Value>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            youtube_dir: default_youtube_dir_string(),
            podcast_dir: default_podcast_dir_string(),
            default_youtube_template: default_youtube_template(),
            default_podcast_template: default_podcast_template(),
            targets: HashMap::new(),
            yt_dlp_options: Some(toml::Value::Array(vec![toml::Value::String(
                "--no-progress".to_string(),
            )])),
            podcast_dl_options: None,
        }
    }
}

#[derive(Debug)]
pub struct ProcessResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Deserialize)]
pub struct ProbeInfo {
    pub channel: Option<String>,
    #[allow(dead_code)]
    #[serde(rename = "channel_id")]
    pub channel_id: Option<String>,
    #[serde(rename = "channel_handle")]
    pub channel_handle: Option<String>,
    pub uploader: Option<String>,
    #[serde(rename = "uploader_id")]
    pub uploader_id: Option<String>,
}

pub fn infer_source_type(url: &str) -> SourceType {
    let value = url.to_ascii_lowercase();
    if value.contains("youtube.com")
        || value.contains("youtu.be")
        || value.contains("soundcloud.com")
    {
        SourceType::YouTube
    } else if value.contains("feed")
        || value.contains("rss")
        || value.ends_with(".xml")
        || value.contains("libsyn.com")
        || value.contains("megaphone.fm")
        || value.contains("supportingcast.fm")
    {
        SourceType::Podcast
    } else {
        SourceType::YouTube
    }
}

pub fn default_youtube_dir() -> PathBuf {
    user_home_directory().join("Videos").join("YouTube")
}

pub fn default_podcast_dir() -> PathBuf {
    user_home_directory().join("Music").join("Podcasts")
}

fn default_source() -> String {
    "auto".to_string()
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn default_youtube_dir_string() -> String {
    default_youtube_dir().to_string_lossy().into_owned()
}

fn default_podcast_dir_string() -> String {
    default_podcast_dir().to_string_lossy().into_owned()
}

fn default_youtube_template() -> String {
    "%(playlist)s/%(upload_date>%Y-%m-%d)s - %(title)s.%(ext)s".to_string()
}

fn default_podcast_template() -> String {
    "{{release_year}}-{{release_month}}-{{release_day}} - {{title}}".to_string()
}

fn user_home_directory() -> PathBuf {
    env::home_dir()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
