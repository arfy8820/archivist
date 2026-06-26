use crate::config::config_option_args;
use crate::paths::podcast_archive_file;
use crate::process::run_process;
use crate::types::{Config, Target};
use std::path::Path;

pub fn info_args(url: &str) -> Vec<String> {
    vec![
        "x".to_string(),
        "podcast-dl".to_string(),
        "--info".to_string(),
        "--url".to_string(),
        url.to_string(),
    ]
}

pub fn probe_label(url: &str) -> Result<String, String> {
    let result = run_process("deno", &info_args(url)).map_err(|error| error.to_string())?;
    if result.exit_code != 0 {
        return Err(result.stderr);
    }
    result
        .stdout
        .replace("\r\n", "\n")
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .ok_or_else(|| "podcast-dl --info did not return a podcast title.".to_string())
}

pub fn build_sync_args(
    config: &Config,
    label: &str,
    target: &Target,
) -> Result<Vec<String>, String> {
    let mut args = vec!["x".to_string(), "podcast-dl".to_string()];
    args.extend(config_option_args(
        &config.podcast_dl_options,
        "podcast_dl_options",
    )?);
    args.extend([
        "--url".to_string(),
        target.primary_url().to_string(),
        "--out-dir".to_string(),
        Path::new(&config.podcast_dir)
            .join(label)
            .to_string_lossy()
            .into_owned(),
        "--threads".to_string(),
        "3".to_string(),
        "--episode-template".to_string(),
        target
            .output_template
            .as_deref()
            .filter(|template| !template.trim().is_empty())
            .unwrap_or(&config.default_podcast_template)
            .to_string(),
        "--archive".to_string(),
        podcast_archive_file(config, label)
            .to_string_lossy()
            .into_owned(),
        "--include-meta".to_string(),
        "--include-episode-meta".to_string(),
    ]);
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn podcast_options_are_inserted_after_deno_podcast_dl_prefix() {
        let config = Config {
            podcast_dl_options: Some(toml::Value::Array(vec![
                toml::Value::String("--debug".to_string()),
                toml::Value::String("--retry".to_string()),
            ])),
            ..Config::default()
        };
        let target = Target {
            urls: vec!["https://example.com/feed.xml".to_string()],
            source: "podcast".to_string(),
            subdir: false,
            output_template: None,
        };

        let args = build_sync_args(&config, "test-podcast", &target).expect("valid options");

        assert_eq!(args[0], "x");
        assert_eq!(args[1], "podcast-dl");
        assert_eq!(args[2], "--debug");
        assert_eq!(args[3], "--retry");
        assert_eq!(args[4], "--url");
    }
}
