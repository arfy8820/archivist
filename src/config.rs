use crate::types::{Config, Target};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigAction {
    #[command(about = "Show all config or one config property")]
    Show {
        #[arg(value_name = "PROPERTY", help = "Property name or alias to show")]
        property: Option<String>,
    },
    #[command(about = "Set or clear a config property")]
    Set {
        #[arg(value_name = "PROPERTY", help = "Property name or alias to update")]
        property: String,
        #[arg(
            value_name = "VALUE",
            help = "New value; omit to clear supported properties"
        )]
        value: Vec<String>,
    },
    #[command(about = "Open the config file in your editor")]
    Edit,
}

#[derive(Args, Debug, Clone)]
pub struct ConfigCommand {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Clone, Copy)]
pub enum ConfigProperty {
    All,
    YoutubeDir,
    PodcastDir,
    DefaultYoutubeTemplate,
    DefaultPodcastTemplate,
    Targets,
    YtDlpOptions,
    PodcastDlOptions,
}

#[derive(Serialize)]
struct TargetsToml<'a> {
    targets: BTreeMap<&'a String, &'a Target>,
}

#[derive(Serialize)]
struct ConfigToml<'a> {
    youtube_dir: &'a str,
    podcast_dir: &'a str,
    default_youtube_template: &'a str,
    default_podcast_template: &'a str,
    targets: BTreeMap<&'a String, &'a Target>,
    #[serde(skip_serializing_if = "Option::is_none")]
    yt_dlp_options: &'a Option<toml::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    podcast_dl_options: &'a Option<toml::Value>,
}

#[derive(Deserialize)]
struct TargetsConfig {
    targets: HashMap<String, Target>,
}

pub fn load_config(path: &Path) -> Result<Config, String> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = fs::read_to_string(path)
        .map_err(|error| format!("Failed to load config from '{}': {error}", path.display()))?;
    let mut config: Config = toml::from_str(&text)
        .map_err(|error| format!("Failed to parse config from '{}': {error}", path.display()))?;
    let defaults = Config::default();
    if config.youtube_dir.trim().is_empty() {
        config.youtube_dir = defaults.youtube_dir;
    }
    if config.podcast_dir.trim().is_empty() {
        config.podcast_dir = defaults.podcast_dir;
    }
    Ok(config)
}

pub fn save_config(path: &Path, config: &Config) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create config directory '{}': {error}",
                parent.display()
            )
        })?;
    }
    let text = toml::to_string_pretty(&ConfigToml::from(config))
        .map_err(|error| format!("Failed to serialize config: {error}"))?;
    fs::write(path, text)
        .map_err(|error| format!("Failed to save config to '{}': {error}", path.display()))
}

pub fn show_config_property(json: bool, config: &Config, property: ConfigProperty) {
    if json {
        match property {
            ConfigProperty::All => print_json(config),
            ConfigProperty::YoutubeDir => {
                print_json(&serde_json::json!({ "youtube_dir": config.youtube_dir }))
            }
            ConfigProperty::PodcastDir => {
                print_json(&serde_json::json!({ "podcast_dir": config.podcast_dir }))
            }
            ConfigProperty::DefaultYoutubeTemplate => print_json(
                &serde_json::json!({ "default_youtube_template": config.default_youtube_template }),
            ),
            ConfigProperty::DefaultPodcastTemplate => print_json(
                &serde_json::json!({ "default_podcast_template": config.default_podcast_template }),
            ),
            ConfigProperty::Targets => print_json(&config.targets),
            ConfigProperty::YtDlpOptions => {
                print_json(&toml_value_to_json(config.yt_dlp_options.as_ref()))
            }
            ConfigProperty::PodcastDlOptions => {
                print_json(&toml_value_to_json(config.podcast_dl_options.as_ref()))
            }
        }
    } else {
        match property {
            ConfigProperty::All => {
                println!("youtube_dir: {}", config.youtube_dir);
                println!("podcast_dir: {}", config.podcast_dir);
                println!(
                    "default_youtube_template: {}",
                    config.default_youtube_template
                );
                println!(
                    "default_podcast_template: {}",
                    config.default_podcast_template
                );
                println!("targets: {}", config.targets.len());
                println!(
                    "yt_dlp_options: {}",
                    if config.yt_dlp_options.is_some() {
                        "configured"
                    } else {
                        "unset"
                    }
                );
                println!(
                    "podcast_dl_options: {}",
                    if config.podcast_dl_options.is_some() {
                        "configured"
                    } else {
                        "unset"
                    }
                );
            }
            ConfigProperty::YoutubeDir => println!("{}", config.youtube_dir),
            ConfigProperty::PodcastDir => println!("{}", config.podcast_dir),
            ConfigProperty::DefaultYoutubeTemplate => {
                println!("{}", config.default_youtube_template)
            }
            ConfigProperty::DefaultPodcastTemplate => {
                println!("{}", config.default_podcast_template)
            }
            ConfigProperty::Targets => println!(
                "{}",
                toml::to_string_pretty(&TargetsToml {
                    targets: sorted_targets(&config.targets)
                })
                .unwrap_or_default()
            ),
            ConfigProperty::YtDlpOptions => print_toml_option(config.yt_dlp_options.as_ref()),
            ConfigProperty::PodcastDlOptions => {
                print_toml_option(config.podcast_dl_options.as_ref())
            }
        }
    }
}

pub fn set_config_property(
    mut config: Config,
    property: ConfigProperty,
    value: Option<&str>,
) -> Result<Config, String> {
    match property {
        ConfigProperty::All => Err("Cannot set all config properties at once.".to_string()),
        ConfigProperty::YoutubeDir => {
            match value.map(str::trim).filter(|value| !value.is_empty()) {
                Some(value) => {
                    config.youtube_dir = value.to_string();
                    Ok(config)
                }
                None => Err("youtube_dir requires a value.".to_string()),
            }
        }
        ConfigProperty::PodcastDir => {
            match value.map(str::trim).filter(|value| !value.is_empty()) {
                Some(value) => {
                    config.podcast_dir = value.to_string();
                    Ok(config)
                }
                None => Err("podcast_dir requires a value.".to_string()),
            }
        }
        ConfigProperty::DefaultYoutubeTemplate => {
            config.default_youtube_template = value.unwrap_or("").to_string();
            Ok(config)
        }
        ConfigProperty::DefaultPodcastTemplate => {
            config.default_podcast_template = value.unwrap_or("").to_string();
            Ok(config)
        }
        ConfigProperty::Targets => {
            config.targets = match value.map(str::trim).filter(|value| !value.is_empty()) {
                Some(value) => toml::from_str::<TargetsConfig>(value)
                    .map(|parsed| parsed.targets)
                    .map_err(|error| format!("targets must be a TOML table: {error}"))?,
                None => HashMap::new(),
            };
            Ok(config)
        }
        ConfigProperty::YtDlpOptions => {
            config.yt_dlp_options = parse_toml_option(value)?;
            Ok(config)
        }
        ConfigProperty::PodcastDlOptions => {
            config.podcast_dl_options = parse_toml_option(value)?;
            Ok(config)
        }
    }
}

pub fn parse_toml_option(value: Option<&str>) -> Result<Option<toml::Value>, String> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => value
            .parse::<toml::Value>()
            .or_else(|_| toml::from_str(value))
            .map_err(|error| format!("value must be valid TOML: {error}"))
            .and_then(|value| {
                config_option_args(&Some(value.clone()), "value")?;
                Ok(Some(value))
            }),
        None => Ok(None),
    }
}

pub fn parse_config_property(value: Option<&str>) -> Result<ConfigProperty, String> {
    match value.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "" => Ok(ConfigProperty::All),
        "youtube_dir" | "youtube-dir" => Ok(ConfigProperty::YoutubeDir),
        "podcast_dir" | "podcast-dir" => Ok(ConfigProperty::PodcastDir),
        "default_youtube_template" | "default-youtube-template" => {
            Ok(ConfigProperty::DefaultYoutubeTemplate)
        }
        "default_podcast_template" | "default-podcast-template" => {
            Ok(ConfigProperty::DefaultPodcastTemplate)
        }
        "targets" => Ok(ConfigProperty::Targets),
        "yt_dlp" | "yt_dlp_opts" | "yt_dlp_options" | "yt-dlp" => Ok(ConfigProperty::YtDlpOptions),
        "podcast_dl" | "podcast_dl_opts" | "podcast_dl_options" | "podcast-dl"
        | "podcast-dl-options" => Ok(ConfigProperty::PodcastDlOptions),
        unknown => Err(format!("Unknown config property: {unknown}")),
    }
}

pub fn config_option_args(
    value: &Option<toml::Value>,
    property_name: &str,
) -> Result<Vec<String>, String> {
    match value {
        None => Ok(Vec::new()),
        Some(toml::Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(index, item)| match item {
                toml::Value::String(value) => Ok(value.clone()),
                _ => Err(format!(
                    "{property_name} must be an array of strings; item {index} is not a string."
                )),
            })
            .collect(),
        Some(_) => Err(format!("{property_name} must be a TOML array of strings.")),
    }
}

pub fn print_json<T: Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => println!("{json}"),
        Err(error) => eprintln!("Failed to serialize JSON output: {error}"),
    }
}

fn print_toml_option(value: Option<&toml::Value>) {
    match value {
        Some(value) => println!("{value}"),
        None => println!("null"),
    }
}

fn toml_value_to_json(value: Option<&toml::Value>) -> serde_json::Value {
    match value {
        Some(value) => serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
        None => serde_json::Value::Null,
    }
}

impl<'a> From<&'a Config> for ConfigToml<'a> {
    fn from(config: &'a Config) -> Self {
        Self {
            youtube_dir: &config.youtube_dir,
            podcast_dir: &config.podcast_dir,
            default_youtube_template: &config.default_youtube_template,
            default_podcast_template: &config.default_podcast_template,
            targets: sorted_targets(&config.targets),
            yt_dlp_options: &config.yt_dlp_options,
            podcast_dl_options: &config.podcast_dl_options,
        }
    }
}

fn sorted_targets(targets: &HashMap<String, Target>) -> BTreeMap<&String, &Target> {
    targets.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_options_must_be_arrays_of_strings() {
        let value = Some(toml::Value::Array(vec![toml::Value::Integer(1)]));

        let error = config_option_args(&value, "yt_dlp_options").expect_err("invalid option");

        assert!(error.contains("array of strings"));
    }

    #[test]
    fn parse_toml_option_accepts_single_string_array() {
        let value = parse_toml_option(Some("[\"--debug\"]")).expect("valid option array");

        assert_eq!(
            value,
            Some(toml::Value::Array(vec![toml::Value::String(
                "--debug".to_string()
            )]))
        );
    }

    #[test]
    fn save_config_sorts_targets_alphabetically() {
        let path = std::env::temp_dir().join(format!(
            "archivist-sorted-targets-{}-{}.toml",
            std::process::id(),
            chrono::Local::now()
                .timestamp_nanos_opt()
                .unwrap_or_default()
        ));
        let mut config = Config::default();
        config.targets.insert(
            "zeta".to_string(),
            Target {
                urls: vec!["https://example.com/zeta".to_string()],
                source: "youtube".to_string(),
                subdir: false,
                output_template: None,
            },
        );
        config.targets.insert(
            "alpha".to_string(),
            Target {
                urls: vec!["https://example.com/alpha".to_string()],
                source: "youtube".to_string(),
                subdir: false,
                output_template: None,
            },
        );

        save_config(&path, &config).expect("config should save");
        let text = fs::read_to_string(&path).expect("config should be readable");
        let _ = fs::remove_file(&path);

        let alpha = text.find("[targets.alpha]").expect("alpha target");
        let zeta = text.find("[targets.zeta]").expect("zeta target");
        assert!(alpha < zeta);
    }
}
