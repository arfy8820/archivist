mod config;
mod paths;
mod podcast_dl;
mod process;
mod types;
mod util;
mod yt_dlp;

use clap::{Args, Parser, Subcommand};
use config::{
    ConfigAction, ConfigCommand, ConfigProperty, load_config, parse_config_property, print_json,
    save_config, set_config_property, show_config_property,
};
use paths::{
    config_file, logs_directory, podcast_archive_template, sync_log_file, youtube_archive_file,
};
use process::run_process_with_log;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use types::{Config, ProbeInfo, ProcessResult, SourceType, Target, infer_source_type};
use util::{format_command, log_info, sanitize_label};

const APP_NAME: &str = "archivist";
const VERSION: &str = env!("CARGO_PKG_VERSION");

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
    let mut urls = resolve_urls(args.urls);
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
    yt_dlp::expand_playlist_urls(&mut urls, source_type, args.include_all, |base_url| {
        confirm_no_default(&format!(
            "Also add '{base_url}' to capture videos not in a playlist? [y/N]: "
        ))
    });
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
    } else {
        confirm_no_default("Store target in subdirectory? [y/N]: ")
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
            SourceType::YouTube => match yt_dlp::build_sync_args(config, &name, &target) {
                Ok(args) => ("yt-dlp", args),
                Err(error) => {
                    eprintln!("{error}");
                    final_code = 1;
                    continue;
                }
            },
            SourceType::Podcast => match podcast_dl::build_sync_args(config, &target) {
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
                show_config_property(cli.json, &config, property);
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
            let args = podcast_dl::info_args(url);
            println!("No label supplied. Probing podcast-dl for feed info...");
            log_info(
                cli.quiet,
                &format!("Running: {}", format_command("deno", &args)),
            );
            match podcast_dl::probe_label(url) {
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
            let args = yt_dlp::probe_args(url);
            println!("No label supplied. Probing yt-dlp for metadata...");
            log_info(
                cli.quiet,
                &format!("Running: {}", format_command("yt-dlp", &args)),
            );
            match yt_dlp::probe(url) {
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
    } else {
        Some(sanitize_label(trimmed.strip_prefix('@').unwrap_or(trimmed)))
    }
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
