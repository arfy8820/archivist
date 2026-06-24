use chrono::Local;
use clap::{Args, Parser, Subcommand};
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

const APP_NAME: &str = "archivist";
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceType {
    YouTube,
    Podcast,
}

impl SourceType {
    fn name(self) -> &'static str {
        match self {
            Self::YouTube => "youtube",
            Self::Podcast => "podcast",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "youtube" | "yt" | "yt-dlp" => Some(Self::YouTube),
            "podcast" | "podcast-dl" | "rss" => Some(Self::Podcast),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Target {
    url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    urls: Option<Vec<String>>,
    #[serde(default = "default_mode")]
    mode: String,
    #[serde(
        default,
        deserialize_with = "deserialize_subdir",
        skip_serializing_if = "is_false"
    )]
    subdir: bool,
    #[serde(
        default,
        rename = "output_template",
        skip_serializing_if = "Option::is_none"
    )]
    output_template: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct NamedTarget {
    name: String,
    #[serde(flatten)]
    target: Target,
}

fn default_mode() -> String {
    "auto".to_string()
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn deserialize_subdir<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<toml::Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(toml::Value::Boolean(value)) => value,
        Some(toml::Value::String(value)) => !value.trim().is_empty(),
        Some(_) => false,
        None => false,
    })
}

impl Target {
    fn source_type(&self) -> SourceType {
        match self.mode.trim().to_ascii_lowercase().as_str() {
            "podcast" | "podcast-dl" | "rss" => SourceType::Podcast,
            "youtube" | "yt" | "yt-dlp" => SourceType::YouTube,
            _ => infer_source_type(&self.url),
        }
    }

    fn sync_urls(&self) -> Vec<String> {
        match &self.urls {
            Some(urls) => {
                let urls: Vec<String> = urls
                    .iter()
                    .map(|url| url.trim().to_string())
                    .filter(|url| !url.is_empty())
                    .collect();
                if urls.is_empty() {
                    vec![self.url.clone()]
                } else {
                    urls
                }
            }
            None => vec![self.url.clone()],
        }
    }
}

fn infer_source_type(url: &str) -> SourceType {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    #[serde(default = "default_youtube_dir_string")]
    youtube_dir: String,
    #[serde(default = "default_podcast_dir_string")]
    podcast_dir: String,
    #[serde(default = "default_youtube_template")]
    default_youtube_template: String,
    #[serde(default = "default_podcast_template")]
    default_podcast_template: String,
    #[serde(default)]
    targets: HashMap<String, Target>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    yt_dlp_options: Option<toml::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    podcast_dl_options: Option<toml::Value>,
}

#[derive(Serialize)]
struct TargetsToml<'a> {
    targets: &'a HashMap<String, Target>,
}

#[derive(Deserialize)]
struct TargetsConfig {
    targets: HashMap<String, Target>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            youtube_dir: default_youtube_dir_string(),
            podcast_dir: default_podcast_dir_string(),
            default_youtube_template: default_youtube_template(),
            default_podcast_template: default_podcast_template(),
            targets: HashMap::new(),
            yt_dlp_options: None,
            podcast_dl_options: None,
        }
    }
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

#[derive(Debug)]
struct ProcessResult {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

#[derive(Debug, Deserialize)]
struct ProbeInfo {
    channel: Option<String>,
    #[allow(dead_code)]
    #[serde(rename = "channel_id")]
    channel_id: Option<String>,
    #[serde(rename = "channel_handle")]
    channel_handle: Option<String>,
    uploader: Option<String>,
    #[serde(rename = "uploader_id")]
    uploader_id: Option<String>,
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = APP_NAME,
    version = VERSION,
    about = "Personal media archiving CLI",
    disable_version_flag = true
)]
struct Cli {
    #[arg(short = 'c', long = "config-file")]
    config_file: Option<PathBuf>,
    #[arg(short = 'j', long = "json")]
    json: bool,
    #[arg(long)]
    quiet: bool,
    #[arg(short = 'v', long = "version")]
    version: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug, Clone)]
enum Commands {
    List,
    Add(AddArgs),
    Remove(RemoveArgs),
    Sync(SyncArgs),
    Probe(ProbeArgs),
    Config(ConfigCommand),
    ImportJson(ImportJsonArgs),
}

#[derive(Args, Debug, Clone)]
struct AddArgs {
    #[arg(long)]
    url: Option<String>,
    #[arg(long)]
    label: Option<String>,
    #[arg(long)]
    output: Option<String>,
    #[arg(long = "type")]
    source_type: Option<String>,
    #[arg(long)]
    subdir: bool,
    #[arg(long = "include-all")]
    include_all: bool,
}

#[derive(Args, Debug, Clone)]
struct RemoveArgs {
    name: String,
    #[arg(long = "delete-archive")]
    delete_archive: bool,
}

#[derive(Args, Debug, Clone)]
struct SyncArgs {
    name: Option<String>,
    #[arg(long)]
    all: bool,
}

#[derive(Args, Debug, Clone)]
struct ProbeArgs {
    name: String,
}

#[derive(Args, Debug, Clone)]
struct ImportJsonArgs {
    input: PathBuf,
    #[arg(short, long)]
    output: Option<PathBuf>,
    #[arg(long)]
    force: bool,
}

#[derive(Subcommand, Debug, Clone)]
enum ConfigAction {
    Show {
        property: Option<String>,
    },
    Set {
        property: String,
        value: Vec<String>,
    },
}

#[derive(Args, Debug, Clone)]
struct ConfigCommand {
    #[command(subcommand)]
    action: ConfigAction,
}

#[derive(Clone, Copy)]
enum ConfigProperty {
    All,
    YoutubeDir,
    PodcastDir,
    DefaultYoutubeTemplate,
    DefaultPodcastTemplate,
    Targets,
    YtDlpOptions,
    PodcastDlOptions,
}

fn main() {
    let cli = Cli::parse();
    let code = run(cli);
    std::process::exit(code);
}

fn run(cli: Cli) -> i32 {
    if cli.version {
        if cli.json {
            print_json(&serde_json::json!({ "version": VERSION }));
        } else {
            println!("archivist version {VERSION}");
        }
        return 0;
    }

    let Some(command) = cli.command.clone() else {
        println!("archivist version {VERSION}");
        return 0;
    };

    let config_path = cli.config_file.clone().unwrap_or_else(config_file);

    if let Commands::ImportJson(args) = command {
        return handle_import_json(&cli, &config_path, args);
    }

    log_info(
        cli.quiet,
        &format!("Loading config from {}", config_path.display()),
    );

    let config = match load_config(&config_path) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };

    match command {
        Commands::List => handle_list(&cli, &config),
        Commands::Add(args) => handle_add(&cli, &config_path, config, args),
        Commands::Remove(args) => handle_remove(&config_path, config, args),
        Commands::Sync(args) => handle_sync(&cli, &config, args),
        Commands::Probe(args) => handle_probe(&cli, &config, args),
        Commands::Config(args) => handle_config(&cli, &config_path, config, args),
        Commands::ImportJson(_) => unreachable!("import-json is handled before config load"),
    }
}

fn handle_add(cli: &Cli, config_path: &Path, mut config: Config, args: AddArgs) -> i32 {
    let url = match args
        .url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
    {
        Some(url) => url.to_string(),
        None => prompt_required("URL: "),
    };

    let output_template = match args
        .output
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => Some(value.to_string()),
        None => prompt_optional("Output template (optional): "),
    };

    let requested_source = match parse_source_arg(args.source_type.as_deref()) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    let source_type = requested_source.unwrap_or_else(|| infer_source_type(&url));
    let label = match resolve_label(cli, &url, source_type, args.label.as_deref()) {
        Ok(label) if !label.is_empty() => label,
        Ok(_) => {
            eprintln!("Label cannot be empty.");
            return 1;
        }
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };

    let urls = target_urls_for_add(&url, source_type, args.include_all);
    let subdir = if args.subdir {
        true
    } else if confirm_no_default("Store target in subdirectory? [y/N]: ") {
        true
    } else {
        false
    };
    let mode = requested_source
        .map(SourceType::name)
        .unwrap_or("auto")
        .to_string();
    let target = Target {
        url,
        urls,
        mode,
        subdir,
        output_template,
    };

    config.targets.insert(label.clone(), target);

    match save_config(config_path, &config) {
        Ok(()) => {
            println!("Added mapping '{label}'.");
            0
        }
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

fn handle_remove(config_path: &Path, mut config: Config, args: RemoveArgs) -> i32 {
    let existing = config.targets.remove(&args.name);

    if let Err(error) = save_config(config_path, &config) {
        eprintln!("{error}");
        return 1;
    }

    if existing.is_some() {
        println!("Removed mapping '{}'.", args.name);
    } else {
        println!("No mapping found for '{}'. Config unchanged.", args.name);
    }

    if !args.delete_archive {
        return 0;
    }

    let archive_path = match existing
        .as_ref()
        .map(Target::source_type)
        .unwrap_or(SourceType::YouTube)
    {
        SourceType::YouTube => youtube_archive_file(&config, &args.name),
        SourceType::Podcast => podcast_archive_template(&config),
    };

    match fs::remove_file(&archive_path) {
        Ok(()) => {
            println!("Removed archive file '{}'.", archive_path.display());
            0
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            println!("Archive file '{}' did not exist.", archive_path.display());
            1
        }
        Err(error) => {
            eprintln!(
                "Failed to remove archive file '{}': {error}",
                archive_path.display()
            );
            1
        }
    }
}

fn handle_list(cli: &Cli, config: &Config) -> i32 {
    if cli.json {
        print_json(&config.targets);
    } else if config.targets.is_empty() {
        println!("No archive mappings configured.");
    } else {
        let mut entries: Vec<_> = config.targets.iter().collect();
        entries.sort_by(|left, right| left.0.cmp(right.0));
        for (name, target) in entries {
            print_target(name, target);
        }
    }
    0
}

fn handle_probe(cli: &Cli, config: &Config, args: ProbeArgs) -> i32 {
    let Some(target) = config.targets.get(&args.name) else {
        eprintln!("No entry found for label '{}'.", args.name);
        return 1;
    };

    let mode = target.source_type().name();
    if cli.json {
        print_json(&serde_json::json!({ "name": args.name, "mode": mode, "url": target.url }));
    } else {
        println!("{mode}");
    }
    0
}

fn handle_sync(cli: &Cli, config: &Config, args: SyncArgs) -> i32 {
    if args.all && args.name.is_some() {
        eprintln!("Use either 'sync --all' or 'sync <name>', not both.");
        return 2;
    }

    if let Err(error) = ensure_sync_directories(config) {
        eprintln!("{error}");
        return 1;
    }

    let entries: Vec<(String, Target)> = match args.name {
        Some(label) => match config.targets.get(&label) {
            Some(target) => vec![(label, target.clone())],
            None => {
                eprintln!("No entry found for label '{label}'.");
                return 1;
            }
        },
        None => {
            let mut entries: Vec<_> = config
                .targets
                .iter()
                .map(|(name, target)| (name.clone(), target.clone()))
                .collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            entries
        }
    };

    if entries.is_empty() {
        println!("No archive mappings configured.");
        return 0;
    }

    let mut final_code = 0;
    for (name, target) in entries {
        let (executable, command_args) = match target.source_type() {
            SourceType::YouTube => ("yt-dlp", build_ytdlp_sync_args(config, &name, &target)),
            SourceType::Podcast => ("deno", build_podcast_sync_args(config, &target)),
        };

        let command_line = format_command(executable, &command_args);
        println!("Syncing '{name}'...");
        log_info(cli.quiet, &format!("Running: {command_line}"));
        let result = run_process(executable, &command_args);
        let log_path = sync_log_file(&name);

        match &result {
            Ok(result) => {
                if let Err(error) = write_process_log(&log_path, &command_line, result) {
                    eprintln!("Failed to write log '{}': {error}", log_path.display());
                } else {
                    log_info(cli.quiet, &format!("Wrote log to {}", log_path.display()));
                }
                print_sync_result(&name, result);
                if result.exit_code != 0 {
                    final_code = result.exit_code;
                }
            }
            Err(error) => {
                eprintln!("Failed to run {executable}: {error}");
                final_code = 1;
            }
        }
    }

    final_code
}

fn handle_config(cli: &Cli, config_path: &Path, config: Config, args: ConfigCommand) -> i32 {
    match args.action {
        ConfigAction::Show { property } => match parse_config_property(property.as_deref()) {
            Ok(property) => {
                show_config_property(cli, &config, property);
                0
            }
            Err(error) => {
                eprintln!("{error}");
                1
            }
        },
        ConfigAction::Set { property, value } => {
            let property = match parse_config_property(Some(&property)) {
                Ok(ConfigProperty::All) => {
                    eprintln!("The 'set' action requires a specific property.");
                    return 1;
                }
                Ok(property) => property,
                Err(error) => {
                    eprintln!("{error}");
                    return 1;
                }
            };
            let value = if value.is_empty() {
                None
            } else {
                Some(value.join(" "))
            };
            match set_config_property(config, property, value.as_deref()) {
                Ok(updated) => match save_config(config_path, &updated) {
                    Ok(()) => {
                        println!("Updated config.");
                        0
                    }
                    Err(error) => {
                        eprintln!("{error}");
                        1
                    }
                },
                Err(error) => {
                    eprintln!("{error}");
                    1
                }
            }
        }
    }
}

fn handle_import_json(cli: &Cli, default_output: &Path, args: ImportJsonArgs) -> i32 {
    let output_path = args.output.unwrap_or_else(|| default_output.to_path_buf());

    if output_path.exists() && !args.force {
        eprintln!(
            "Output config '{}' already exists. Pass --force to overwrite it.",
            output_path.display()
        );
        return 1;
    }

    let text = match fs::read_to_string(&args.input) {
        Ok(text) => text,
        Err(error) => {
            eprintln!(
                "Failed to load JSON config from '{}': {error}",
                args.input.display()
            );
            return 1;
        }
    };

    let config = match parse_json_config(&text) {
        Ok(config) => config,
        Err(error) => {
            eprintln!(
                "Failed to parse JSON config from '{}': {error}",
                args.input.display()
            );
            return 1;
        }
    };

    match save_config(&output_path, &config) {
        Ok(()) => {
            if cli.json {
                print_json(&serde_json::json!({
                    "input": args.input,
                    "output": output_path,
                    "targets": config.targets.len()
                }));
            } else {
                println!(
                    "Imported '{}' to '{}' with {} target(s).",
                    args.input.display(),
                    output_path.display(),
                    config.targets.len()
                );
            }
            0
        }
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

fn parse_source_arg(value: Option<&str>) -> Result<Option<SourceType>, String> {
    match value {
        None => Ok(None),
        Some(value) if value.trim().eq_ignore_ascii_case("auto") => Ok(None),
        Some(value) => SourceType::parse(value)
            .map(Some)
            .ok_or_else(|| "Unknown target type. Use 'auto', 'youtube', or 'podcast'.".to_string()),
    }
}

fn resolve_label(
    cli: &Cli,
    url: &str,
    source_type: SourceType,
    label: Option<&str>,
) -> Result<String, String> {
    if let Some(label) = label.map(str::trim).filter(|label| !label.is_empty()) {
        return Ok(sanitize_label(label));
    }

    match source_type {
        SourceType::Podcast => {
            let args = podcast_info_args(url);
            println!("No label supplied. Probing podcast-dl for feed info...");
            log_info(
                cli.quiet,
                &format!("Running: {}", format_command("deno", &args)),
            );
            match probe_podcast_label(url) {
                Ok(title) => Ok(choose_label_from_suggestion(&title)),
                Err(error) => {
                    eprintln!("Could not probe podcast label automatically.");
                    if !error.trim().is_empty() {
                        eprintln!("{error}");
                    }
                    Ok(sanitize_label(&prompt_required("Label: ")))
                }
            }
        }
        SourceType::YouTube => {
            let args = ytdlp_probe_args(url);
            println!("No label supplied. Probing yt-dlp for metadata...");
            log_info(
                cli.quiet,
                &format!("Running: {}", format_command("yt-dlp", &args)),
            );
            match probe_youtube(url) {
                Ok(info) => Ok(choose_label_from_probe(&info)),
                Err(error) => {
                    eprintln!("Could not probe label automatically.");
                    if !error.trim().is_empty() {
                        eprintln!("{error}");
                    }
                    Ok(sanitize_label(&prompt_required("Label: ")))
                }
            }
        }
    }
}

fn target_urls_for_add(
    url: &str,
    source_type: SourceType,
    include_all: bool,
) -> Option<Vec<String>> {
    let trimmed = url.trim_end_matches('/');
    let suffix = "/playlists";
    if source_type == SourceType::YouTube && trimmed.to_ascii_lowercase().ends_with(suffix) {
        let base = &trimmed[..trimmed.len() - suffix.len()];
        if include_all
            || confirm_no_default(&format!(
                "Also download '{base}' to capture videos not in a playlist? [y/N]: "
            ))
        {
            return Some(vec![url.to_string(), base.to_string()]);
        }
    }
    None
}

fn choose_label_from_probe(probe: &ProbeInfo) -> String {
    if let Some(stable) = stable_suggested_label(probe) {
        if confirm_yes_default(&format!("Use detected label '{stable}'? [Y/n]: ")) {
            return stable;
        }
        return sanitize_label(&prompt_required("Label: "));
    }

    match suggested_label(probe) {
        Some(suggestion) => choose_label_from_suggestion(&suggestion),
        None => sanitize_label(&prompt_required("Label: ")),
    }
}

fn choose_label_from_suggestion(suggestion: &str) -> String {
    let sanitized = sanitize_label(suggestion);
    match prompt(&format!("Label [{sanitized}]: ")) {
        Some(value) if !value.trim().is_empty() => sanitize_label(value.trim()),
        _ => sanitized,
    }
}

fn suggested_label(probe: &ProbeInfo) -> Option<String> {
    [
        normalize_handle(probe.channel_handle.as_deref()),
        normalize_handle(probe.uploader_id.as_deref()),
        probe.channel.as_deref().map(sanitize_label),
        probe.uploader.as_deref().map(sanitize_label),
    ]
    .into_iter()
    .flatten()
    .next()
}

fn stable_suggested_label(probe: &ProbeInfo) -> Option<String> {
    [
        normalize_handle(probe.channel_handle.as_deref()),
        normalize_handle(probe.uploader_id.as_deref()),
    ]
    .into_iter()
    .flatten()
    .next()
}

fn normalize_handle(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty() || trimmed.starts_with("UC") || trimmed.contains(' ') {
        None
    } else if trimmed.starts_with('@') {
        Some(sanitize_label(trimmed))
    } else {
        Some(sanitize_label(&format!("@{trimmed}")))
    }
}

fn ytdlp_probe_args(url: &str) -> Vec<String> {
    vec![
        "--dump-json".to_string(),
        "--skip-download".to_string(),
        "--playlist-end".to_string(),
        "1".to_string(),
        url.to_string(),
    ]
}

fn probe_youtube(url: &str) -> Result<ProbeInfo, String> {
    let result =
        run_process("yt-dlp", &ytdlp_probe_args(url)).map_err(|error| error.to_string())?;
    if result.exit_code != 0 {
        return Err(result.stderr);
    }
    serde_json::from_str(&result.stdout)
        .map_err(|error| format!("Failed to parse yt-dlp metadata: {error}"))
}

fn podcast_info_args(url: &str) -> Vec<String> {
    vec![
        "x".to_string(),
        "podcast-dl".to_string(),
        "--info".to_string(),
        "--url".to_string(),
        url.to_string(),
    ]
}

fn probe_podcast_label(url: &str) -> Result<String, String> {
    let result = run_process("deno", &podcast_info_args(url)).map_err(|error| error.to_string())?;
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

fn build_ytdlp_sync_args(config: &Config, label: &str, target: &Target) -> Vec<String> {
    let mut args = vec![
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
    ];
    args.extend(target.sync_urls());
    args
}

fn build_podcast_sync_args(config: &Config, target: &Target) -> Vec<String> {
    vec![
        "x".to_string(),
        "podcast-dl".to_string(),
        "--url".to_string(),
        target.url.clone(),
        "--out-dir".to_string(),
        Path::new(&config.podcast_dir)
            .join("{{podcast_title}}")
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
        podcast_archive_template(config)
            .to_string_lossy()
            .into_owned(),
        "--include-meta".to_string(),
        "--include-episode-meta".to_string(),
    ]
}

fn run_process(executable: &str, args: &[String]) -> io::Result<ProcessResult> {
    let output = ProcessCommand::new(executable).args(args).output()?;
    Ok(ProcessResult {
        exit_code: output.status.code().unwrap_or(1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn ensure_sync_directories(config: &Config) -> Result<(), String> {
    fs::create_dir_all(&config.youtube_dir).map_err(|error| {
        format!(
            "Failed to create directories under '{}': {error}",
            config.youtube_dir
        )
    })?;
    fs::create_dir_all(logs_directory()).map_err(|error| {
        format!(
            "Failed to create log directory '{}': {error}",
            logs_directory().display()
        )
    })?;
    Ok(())
}

fn print_sync_result(label: &str, result: &ProcessResult) {
    if result.exit_code == 0 {
        println!("Sync succeeded for '{label}'.");
    } else {
        eprintln!(
            "Warning: Sync for '{label}' returned with exit code {}.",
            result.exit_code
        );
    }

    if result.exit_code != 0 && !result.stderr.trim().is_empty() {
        eprintln!("{}", result.stderr);
    }
}

fn write_process_log(path: &Path, command_line: &str, result: &ProcessResult) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = format!(
        "Timestamp: {}\nCommand: {command_line}\nExitCode: {}\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
        Local::now().to_rfc3339(),
        result.exit_code,
        result.stdout,
        result.stderr
    );
    fs::write(path, content)
}

fn print_target(name: &str, target: &Target) {
    println!("{name}");
    println!("  Type: {}", target.source_type().name());
    println!("  Mode: {}", target.mode);
    println!("  URL: {}", target.url);
    if let Some(urls) = &target.urls {
        if urls.len() > 1 {
            for url in urls {
                println!("  Sync URL: {url}");
            }
        }
    }
    if target.subdir {
        println!("  Subdir: {name}");
    }
    match &target.output_template {
        Some(template) => println!("  Output: {template}"),
        None => println!("  Output: default"),
    }
}

fn show_config_property(cli: &Cli, config: &Config, property: ConfigProperty) {
    if cli.json {
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
                    targets: &config.targets
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

fn set_config_property(
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

fn parse_toml_option(value: Option<&str>) -> Result<Option<toml::Value>, String> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => toml::from_str(value)
            .map(Some)
            .or_else(|_| value.parse::<toml::Value>().map(Some))
            .map_err(|error| format!("value must be valid TOML: {error}")),
        None => Ok(None),
    }
}

fn parse_config_property(value: Option<&str>) -> Result<ConfigProperty, String> {
    match value.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "" => Ok(ConfigProperty::All),
        "base_dir" | "base-dir" | "youtube_dir" | "youtube-dir" => Ok(ConfigProperty::YoutubeDir),
        "podcast_dir" | "podcast-dir" => Ok(ConfigProperty::PodcastDir),
        "default_output_template"
        | "default-output-template"
        | "default_youtube_template"
        | "default-youtube-template" => Ok(ConfigProperty::DefaultYoutubeTemplate),
        "podcast_template"
        | "podcast-template"
        | "default_podcast_template"
        | "default-podcast-template" => Ok(ConfigProperty::DefaultPodcastTemplate),
        "targets" => Ok(ConfigProperty::Targets),
        "yt_dlp" | "yt_dlp_opts" | "yt_dlp_options" | "yt-dlp" => Ok(ConfigProperty::YtDlpOptions),
        "podcast_dl" | "podcast_dl_opts" | "podcast_dl_options" | "podcast-dl"
        | "podcast-dl-options" => Ok(ConfigProperty::PodcastDlOptions),
        unknown => Err(format!("Unknown config property: {unknown}")),
    }
}

fn parse_json_config(text: &str) -> Result<Config, String> {
    let root: JsonValue = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let defaults = Config::default();

    let youtube_dir = json_get_string(&root, &["base_dir", "youtube_dir", "baseDir", "youtubeDir"])
        .unwrap_or(defaults.youtube_dir);
    let podcast_dir =
        json_get_string(&root, &["podcast_dir", "podcastDir"]).unwrap_or(defaults.podcast_dir);
    let default_youtube_template = json_get_string(
        &root,
        &[
            "default_output_template",
            "defaultOutputTemplate",
            "defaultYoutubeTemplate",
            "default_youtube_template",
        ],
    )
    .unwrap_or(defaults.default_youtube_template);
    let default_podcast_template = json_get_string(
        &root,
        &["default_podcast_template", "defaultPodcastTemplate"],
    )
    .unwrap_or(defaults.default_podcast_template);

    let mut targets = parse_json_targets(&root);
    if targets.is_empty() {
        targets = parse_legacy_json_entries(&root);
    }

    Ok(Config {
        youtube_dir,
        podcast_dir,
        default_youtube_template,
        default_podcast_template,
        targets,
        yt_dlp_options: json_get_toml_value(&root, &["yt_dlp", "yt_dlp_opts", "ytDlp"]),
        podcast_dl_options: json_get_toml_value(
            &root,
            &[
                "podcast_dl",
                "podcast-dl",
                "podcastDL",
                "podcast_dl_opts",
                "podcast_dl_options",
            ],
        ),
    })
}

fn parse_json_targets(root: &JsonValue) -> HashMap<String, Target> {
    root.get("targets")
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let name = json_get_string(item, &["name"])?;
                    let url = json_get_string(item, &["url"])?;
                    let target = Target {
                        url,
                        urls: json_get_string_list(item, &["urls"]),
                        mode: json_get_string(item, &["mode"]).unwrap_or_else(default_mode),
                        subdir: json_get_subdir(item),
                        output_template: json_get_string(
                            item,
                            &["output_template", "outputTemplate"],
                        ),
                    };
                    Some((name, target))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_legacy_json_entries(root: &JsonValue) -> HashMap<String, Target> {
    root.get("entries")
        .and_then(JsonValue::as_object)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|(name, item)| {
                    let url = json_get_string(item, &["url"])?;
                    Some((
                        name.clone(),
                        Target {
                            url,
                            urls: None,
                            mode: json_get_string(item, &["sourceType", "source_type"])
                                .unwrap_or_else(|| "youtube".to_string()),
                            subdir: false,
                            output_template: json_get_string(
                                item,
                                &["outputTemplate", "output_template"],
                            ),
                        },
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn json_get_string(root: &JsonValue, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        root.get(*name)
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn json_get_string_list(root: &JsonValue, names: &[&str]) -> Option<Vec<String>> {
    let urls: Vec<String> = names
        .iter()
        .find_map(|name| root.get(*name).and_then(JsonValue::as_array))
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();

    if urls.is_empty() {
        None
    } else {
        Some(urls)
    }
}

fn json_get_subdir(root: &JsonValue) -> bool {
    match root.get("subdir") {
        Some(JsonValue::Bool(value)) => *value,
        Some(JsonValue::String(value)) => !value.trim().is_empty(),
        _ => false,
    }
}

fn json_get_toml_value(root: &JsonValue, names: &[&str]) -> Option<toml::Value> {
    names
        .iter()
        .find_map(|name| root.get(*name))
        .and_then(json_to_toml_value)
}

fn json_to_toml_value(value: &JsonValue) -> Option<toml::Value> {
    match value {
        JsonValue::Null => None,
        JsonValue::Bool(value) => Some(toml::Value::Boolean(*value)),
        JsonValue::Number(value) => {
            if let Some(value) = value.as_i64() {
                Some(toml::Value::Integer(value))
            } else {
                value.as_f64().map(toml::Value::Float)
            }
        }
        JsonValue::String(value) => Some(toml::Value::String(value.clone())),
        JsonValue::Array(values) => Some(toml::Value::Array(
            values.iter().filter_map(json_to_toml_value).collect(),
        )),
        JsonValue::Object(values) => {
            let table = values
                .iter()
                .filter_map(|(key, value)| {
                    json_to_toml_value(value).map(|value| (key.clone(), value))
                })
                .collect();
            Some(toml::Value::Table(table))
        }
    }
}

fn load_config(path: &Path) -> Result<Config, String> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = fs::read_to_string(path)
        .map_err(|error| format!("Failed to load config from '{}': {error}", path.display()))?;
    let mut config: Config = toml::from_str(&text)
        .or_else(|_| parse_legacy_toml_config(&text))
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

#[derive(Deserialize)]
struct LegacyTomlConfig {
    #[serde(default = "default_youtube_dir_string")]
    youtube_dir: String,
    #[serde(default = "default_podcast_dir_string")]
    podcast_dir: String,
    #[serde(default = "default_youtube_template")]
    default_youtube_template: String,
    #[serde(default = "default_podcast_template")]
    default_podcast_template: String,
    #[serde(default)]
    targets: Vec<NamedTarget>,
    #[serde(default)]
    yt_dlp_options: Option<toml::Value>,
    #[serde(default)]
    podcast_dl_options: Option<toml::Value>,
}

fn parse_legacy_toml_config(text: &str) -> Result<Config, toml::de::Error> {
    let legacy: LegacyTomlConfig = toml::from_str(text)?;
    Ok(Config {
        youtube_dir: legacy.youtube_dir,
        podcast_dir: legacy.podcast_dir,
        default_youtube_template: legacy.default_youtube_template,
        default_podcast_template: legacy.default_podcast_template,
        targets: legacy
            .targets
            .into_iter()
            .map(|named| (named.name, named.target))
            .collect(),
        yt_dlp_options: legacy.yt_dlp_options,
        podcast_dl_options: legacy.podcast_dl_options,
    })
}

fn save_config(path: &Path, config: &Config) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create config directory '{}': {error}",
                parent.display()
            )
        })?;
    }
    let text = toml::to_string_pretty(config)
        .map_err(|error| format!("Failed to serialize config: {error}"))?;
    fs::write(path, text)
        .map_err(|error| format!("Failed to save config to '{}': {error}", path.display()))
}

fn print_toml_option(value: Option<&toml::Value>) {
    match value {
        Some(value) => println!("{value}"),
        None => println!("null"),
    }
}

fn toml_value_to_json(value: Option<&toml::Value>) -> JsonValue {
    match value {
        Some(value) => serde_json::to_value(value).unwrap_or(JsonValue::Null),
        None => JsonValue::Null,
    }
}

fn print_json<T: Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => println!("{json}"),
        Err(error) => eprintln!("Failed to serialize JSON output: {error}"),
    }
}

fn config_directory() -> PathBuf {
    match env::var_os("ARCHIVIST_CONFIG_DIR") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => user_home_directory().join(".config").join(APP_NAME),
    }
}

fn config_file() -> PathBuf {
    config_directory().join("config.toml")
}

fn logs_directory() -> PathBuf {
    config_directory().join("logs")
}

fn user_home_directory() -> PathBuf {
    env::home_dir()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn default_youtube_dir() -> PathBuf {
    user_home_directory().join("Videos").join("YouTube")
}

fn default_podcast_dir() -> PathBuf {
    user_home_directory().join("Music").join("Podcasts")
}

fn youtube_archive_file(config: &Config, label: &str) -> PathBuf {
    Path::new(&config.youtube_dir)
        .join(label)
        .join(".download-archive.txt")
}

fn podcast_archive_template(config: &Config) -> PathBuf {
    Path::new(&config.podcast_dir)
        .join("{{podcast_title}}")
        .join("archive.json")
}

fn sync_log_file(label: &str) -> PathBuf {
    logs_directory().join(format!(
        "sync-{}-{}.log",
        sanitize_label(label),
        Local::now().format("%Y%m%d-%H%M%S")
    ))
}

fn prompt(message: &str) -> Option<String> {
    print!("{message}");
    io::stdout().flush().ok()?;
    let mut line = String::new();
    match io::stdin().read_line(&mut line) {
        Ok(0) => None,
        Ok(_) => Some(line.trim_end_matches(['\r', '\n']).to_string()),
        Err(_) => None,
    }
}

fn prompt_required(message: &str) -> String {
    loop {
        if let Some(value) = prompt(message) {
            let value = value.trim();
            if !value.is_empty() {
                return value.to_string();
            }
        }
    }
}

fn prompt_optional(message: &str) -> Option<String> {
    prompt(message).and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn confirm_yes_default(message: &str) -> bool {
    match prompt(message) {
        None => true,
        Some(value) if value.trim().is_empty() => true,
        Some(value) => matches!(value.trim().to_ascii_lowercase().as_str(), "y" | "yes"),
    }
}

fn confirm_no_default(message: &str) -> bool {
    match prompt(message) {
        None => false,
        Some(value) if value.trim().is_empty() => false,
        Some(value) => matches!(value.trim().to_ascii_lowercase().as_str(), "y" | "yes"),
    }
}

fn sanitize_label(label: &str) -> String {
    let invalid = Regex::new(r#"[<>:"/\\|?*\x00-\x1F]"#).expect("valid regex");
    let repeated_dash = Regex::new("-+").expect("valid regex");
    let value = invalid.replace_all(label.trim(), "-").replace(' ', "-");
    repeated_dash
        .replace_all(&value, "-")
        .trim_matches('-')
        .to_string()
}

fn quote_arg(arg: &str) -> String {
    if arg.contains(' ') || arg.contains('\t') || arg.contains('"') {
        format!("\"{}\"", arg.replace('"', "\\\""))
    } else {
        arg.to_string()
    }
}

fn format_command(executable: &str, args: &[String]) -> String {
    std::iter::once(executable.to_string())
        .chain(args.iter().map(|arg| quote_arg(arg)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn log_info(quiet: bool, message: &str) {
    if !quiet {
        println!("{message}");
    }
}
