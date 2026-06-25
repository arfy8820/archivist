use chrono::Local;
use clap::{Args, Parser, Subcommand};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::thread;

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
    urls: Vec<String>,
    #[serde(default = "default_source")]
    source: String,
    #[serde(default, skip_serializing_if = "is_false")]
    subdir: bool,
    #[serde(
        default,
        rename = "output_template",
        skip_serializing_if = "Option::is_none"
    )]
    output_template: Option<String>,
}

fn default_source() -> String {
    "auto".to_string()
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Target {
    fn source_type(&self) -> SourceType {
        match self.source.trim().to_ascii_lowercase().as_str() {
            "podcast" | "podcast-dl" | "rss" => SourceType::Podcast,
            "youtube" | "yt" | "yt-dlp" => SourceType::YouTube,
            _ => infer_source_type(self.primary_url()),
        }
    }

    fn primary_url(&self) -> &str {
        self.urls.first().map(String::as_str).unwrap_or("")
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
            yt_dlp_options: Some(toml::Value::Array(vec![toml::Value::String(
                "--no-progress".to_string(),
            )])),
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
}

#[derive(Args, Debug, Clone)]
struct AddArgs {
    #[arg(long = "url")]
    urls: Vec<String>,
    #[arg(long)]
    label: Option<String>,
    #[arg(long)]
    output: Option<String>,
    #[arg(long = "type")]
    source_type: Option<String>,
    #[arg(long)]
    subdir: bool,
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

    let config_path = cli.config_file.clone().unwrap_or_else(config_file);

    let Some(command) = cli.command.clone() else {
        if cli.config_file.is_some() {
            return handle_default_config(&config_path);
        }

        println!("archivist version {VERSION}");
        println!("use -h or --help for usage information.");
        return 0;
    };

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
    }
}

fn handle_default_config(config_path: &Path) -> i32 {
    if config_path.exists() {
        println!("Config already exists at '{}'.", config_path.display());
        return 0;
    }

    let config = Config::default();
    match save_config(config_path, &config) {
        Ok(()) => {
            println!("Created new default config at '{}'.", config_path.display());
            0
        }
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

fn handle_add(cli: &Cli, config_path: &Path, mut config: Config, args: AddArgs) -> i32 {
    let urls = resolve_urls(args.urls);
    let primary_url = urls.first().cloned().unwrap_or_default();

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
    let source_type = requested_source.unwrap_or_else(|| infer_source_type(&primary_url));
    let label = match resolve_label(cli, &primary_url, source_type, args.label.as_deref()) {
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

    let subdir = if args.subdir {
        true
    } else if confirm_no_default("Store target in subdirectory? [y/N]: ") {
        true
    } else {
        false
    };
    let source = requested_source
        .map(SourceType::name)
        .unwrap_or("auto")
        .to_string();
    let target = Target {
        urls,
        source,
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

    let source = target.source_type().name();
    if cli.json {
        print_json(
            &serde_json::json!({ "name": args.name, "source": source, "url": target.primary_url() }),
        );
    } else {
        println!("{source}");
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
            SourceType::YouTube => match build_ytdlp_sync_args(config, &name, &target) {
                Ok(args) => ("yt-dlp", args),
                Err(error) => {
                    eprintln!("{error}");
                    final_code = 1;
                    continue;
                }
            },
            SourceType::Podcast => match build_podcast_sync_args(config, &target) {
                Ok(args) => ("deno", args),
                Err(error) => {
                    eprintln!("{error}");
                    final_code = 1;
                    continue;
                }
            },
        };

        let command_line = format_command(executable, &command_args);
        println!("Syncing '{name}'...");
        log_info(cli.quiet, &format!("Running: {command_line}"));
        let log_path = sync_log_file(&name);
        let result = run_process_with_log(executable, &command_args, &log_path, &command_line);

        match &result {
            Ok(result) => {
                log_info(cli.quiet, &format!("Wrote log to {}", log_path.display()));
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

fn parse_source_arg(value: Option<&str>) -> Result<Option<SourceType>, String> {
    match value {
        None => Ok(None),
        Some(value) if value.trim().eq_ignore_ascii_case("auto") => Ok(None),
        Some(value) => SourceType::parse(value)
            .map(Some)
            .ok_or_else(|| "Unknown target type. Use 'auto', 'youtube', or 'podcast'.".to_string()),
    }
}

fn resolve_urls(urls: Vec<String>) -> Vec<String> {
    let urls = urls
        .into_iter()
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty())
        .collect::<Vec<_>>();

    if !urls.is_empty() {
        return urls;
    }

    let mut urls = vec![prompt_required("URL: ")];

    while confirm_no_default("Add another URL to this target? [y/N]: ") {
        urls.push(prompt_required("URL: "));
    }

    urls
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

fn build_ytdlp_sync_args(
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

fn build_podcast_sync_args(config: &Config, target: &Target) -> Result<Vec<String>, String> {
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
    ]);
    Ok(args)
}

fn config_option_args(
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

fn run_process(executable: &str, args: &[String]) -> io::Result<ProcessResult> {
    let output = ProcessCommand::new(executable).args(args).output()?;
    Ok(ProcessResult {
        exit_code: output.status.code().unwrap_or(1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn run_process_with_log(
    executable: &str,
    args: &[String],
    log_path: &Path,
    command_line: &str,
) -> io::Result<ProcessResult> {
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = File::create(log_path)?;
    writeln!(file, "Timestamp: {}", Local::now().to_rfc3339())?;
    writeln!(file, "Command: {command_line}")?;
    writeln!(file)?;
    writeln!(
        file,
        "Output is streamed while the subprocess runs. Chunks are prefixed with their source stream."
    )?;
    writeln!(file)?;
    file.flush()?;

    let file = Arc::new(Mutex::new(file));
    let mut child = ProcessCommand::new(executable)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .map(|stdout| stream_process_output("STDOUT", stdout, Arc::clone(&file)));
    let stderr = child
        .stderr
        .take()
        .map(|stderr| stream_process_output("STDERR", stderr, Arc::clone(&file)));

    let status = child.wait()?;
    let stdout = join_stream_output(stdout)?;
    let stderr = join_stream_output(stderr)?;
    let exit_code = status.code().unwrap_or(1);

    {
        let mut file = lock_log_file(&file)?;
        writeln!(file)?;
        writeln!(file, "ExitCode: {exit_code}")?;
        file.flush()?;
    }

    Ok(ProcessResult {
        exit_code,
        stdout,
        stderr,
    })
}

fn stream_process_output<R>(
    label: &'static str,
    stream: R,
    file: Arc<Mutex<File>>,
) -> thread::JoinHandle<io::Result<String>>
where
    R: io::Read + Send + 'static,
{
    thread::spawn(move || {
        let mut collected = Vec::new();
        let mut reader = stream;
        let mut buffer = [0_u8; 8192];

        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }

            let chunk = &buffer[..bytes_read];
            collected.extend_from_slice(chunk);
            let mut file = lock_log_file(&file)?;
            write!(file, "[{label}] ")?;
            file.write_all(chunk)?;
            if !chunk.ends_with(b"\n") && !chunk.ends_with(b"\r") {
                writeln!(file)?;
            }
            file.flush()?;
        }

        Ok(String::from_utf8_lossy(&collected).into_owned())
    })
}

fn join_stream_output(
    handle: Option<thread::JoinHandle<io::Result<String>>>,
) -> io::Result<String> {
    match handle {
        Some(handle) => handle
            .join()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "stream reader thread panicked"))?,
        None => Ok(String::new()),
    }
}

fn lock_log_file(file: &Arc<Mutex<File>>) -> io::Result<std::sync::MutexGuard<'_, File>> {
    file.lock()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "log file lock was poisoned"))
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

fn print_target(name: &str, target: &Target) {
    println!("{name}");
    println!("  inferred source: {}", target.source_type().name());
    println!("  Requested source: {}", target.source);
    match target.urls.as_slice() {
        [] => println!("  URLs: none"),
        [url] => println!("  URL: {url}"),
        urls => {
            for url in urls {
                println!("  URL: {url}");
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

fn parse_config_property(value: Option<&str>) -> Result<ConfigProperty, String> {
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

fn load_config(path: &Path) -> Result<Config, String> {
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

fn toml_value_to_json(value: Option<&toml::Value>) -> serde_json::Value {
    match value {
        Some(value) => serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
        None => serde_json::Value::Null,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_process_with_log_streams_stdout_and_stderr_to_log() {
        let log_path = env::temp_dir().join(format!(
            "archivist-stream-test-{}-{}.log",
            std::process::id(),
            Local::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let args = vec![
            "-c".to_string(),
            "printf 'out1\n'; printf 'err1\n' >&2".to_string(),
        ];

        let result =
            run_process_with_log("sh", &args, &log_path, "sh -c test").expect("process should run");
        let log = fs::read_to_string(&log_path).expect("log should be readable");
        let _ = fs::remove_file(&log_path);

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "out1\n");
        assert_eq!(result.stderr, "err1\n");
        assert!(log.contains("Command: sh -c test"));
        assert!(log.contains("[STDOUT] out1"));
        assert!(log.contains("[STDERR] err1"));
        assert!(log.contains("ExitCode: 0"));
    }

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

        let args = build_ytdlp_sync_args(&config, "example", &target).expect("valid options");

        assert_eq!(args[0], "--ignore-errors");
        assert_eq!(args[1], "--no-warnings");
        assert_eq!(args[2], "--download-archive");
    }

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

        let args = build_podcast_sync_args(&config, &target).expect("valid options");

        assert_eq!(args[0], "x");
        assert_eq!(args[1], "podcast-dl");
        assert_eq!(args[2], "--debug");
        assert_eq!(args[3], "--retry");
        assert_eq!(args[4], "--url");
    }

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
}
