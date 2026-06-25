use crate::config::config_option_args;
use crate::paths::youtube_archive_file;
use crate::process::run_process;
use crate::types::{Config, ProbeInfo, SourceType, Target};
use std::path::Path;

pub fn probe_args(url: &str) -> Vec<String> {
    vec![
        "--dump-json".to_string(),
        "--skip-download".to_string(),
        "--playlist-end".to_string(),
        "1".to_string(),
        url.to_string(),
    ]
}

pub fn probe(url: &str) -> Result<ProbeInfo, String> {
    let result = run_process("yt-dlp", &probe_args(url)).map_err(|error| error.to_string())?;
    if result.exit_code != 0 {
        return Err(result.stderr);
    }
    serde_json::from_str(&result.stdout)
        .map_err(|error| format!("Failed to parse yt-dlp metadata: {error}"))
}

pub fn build_sync_args(
    config: &Config,
    label: &str,
    target: &Target,
) -> Result<Vec<String>, String> {
    let mut args = config_option_args(&config.yt_dlp_options, "yt_dlp_options")?;
    args.extend([
        "--download-archive".to_string(),
        youtube_archive_file(config, label)
            .to_string_lossy()
            .into_owned(),
        "--paths".to_string(),
        if target.subdir {
            Path::new(&config.youtube_dir)
                .join(label)
                .to_string_lossy()
                .into_owned()
        } else {
            config.youtube_dir.clone()
        },
        "-o".to_string(),
        target
            .output_template
            .as_deref()
            .filter(|template| !template.trim().is_empty())
            .unwrap_or(&config.default_youtube_template)
            .to_string(),
    ]);
    args.extend(target.urls.clone());
    Ok(args)
}

pub fn expand_playlist_urls<F>(
    urls: &mut Vec<String>,
    source_type: SourceType,
    include_all: bool,
    mut confirm: F,
) where
    F: FnMut(&str) -> bool,
{
    if source_type != SourceType::YouTube {
        return;
    }

    let base_urls = urls
        .iter()
        .filter_map(|url| playlist_base_url(url))
        .collect::<Vec<_>>();

    for base_url in base_urls {
        if urls.iter().any(|url| same_url(url, &base_url)) {
            continue;
        }

        if include_all || confirm(&base_url) {
            urls.push(base_url);
        }
    }
}

pub fn playlist_base_url(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    let lower = trimmed.to_ascii_lowercase();
    let suffix = "/playlists";

    if !(lower.contains("youtube.com") || lower.contains("youtu.be")) || !lower.ends_with(suffix) {
        return None;
    }

    Some(trimmed[..trimmed.len() - suffix.len()].to_string())
}

fn same_url(left: &str, right: &str) -> bool {
    left.trim()
        .trim_end_matches('/')
        .eq_ignore_ascii_case(right.trim().trim_end_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ytdlp_options_are_prepended_to_sync_args() {
        let config = Config {
            yt_dlp_options: Some(toml::Value::Array(vec![
                toml::Value::String("--ignore-errors".to_string()),
                toml::Value::String("--no-warnings".to_string()),
            ])),
            ..Config::default()
        };
        let target = Target {
            urls: vec!["https://www.youtube.com/example".to_string()],
            source: "youtube".to_string(),
            subdir: false,
            output_template: None,
        };

        let args = build_sync_args(&config, "example", &target).expect("valid options");

        assert_eq!(args[0], "--ignore-errors");
        assert_eq!(args[1], "--no-warnings");
        assert_eq!(args[2], "--download-archive");
    }

    #[test]
    fn youtube_playlist_base_url_strips_playlists_suffix() {
        let base = playlist_base_url("https://www.youtube.com/@example/playlists/");

        assert_eq!(base.as_deref(), Some("https://www.youtube.com/@example"));
    }

    #[test]
    fn include_all_adds_youtube_playlist_base_url_once() {
        let mut urls = vec![
            "https://www.youtube.com/@example/playlists".to_string(),
            "https://www.youtube.com/@example/".to_string(),
        ];

        expand_playlist_urls(&mut urls, SourceType::YouTube, true, |_| false);

        assert_eq!(urls.len(), 2);
    }
}
