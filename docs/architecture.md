# Architecture Notes

## Current Shape

Archivist is currently one Rust binary crate split into focused modules:

```text
Cargo.toml
src/main.rs         CLI parser, prompts, rendering, and command handlers
src/input.rs        Interactive input with readline editing and stdin fallback
src/types.rs        Domain model and defaults
src/config.rs       TOML config loading, saving, and config command helpers
src/yt_dlp.rs       yt-dlp probing, playlist URL expansion, and sync arguments
src/podcast_dl.rs   podcast-dl probing and sync arguments
src/process.rs      Subprocess execution and streamed log writing
src/paths.rs        Config, archive, and log path helpers
src/util.rs         Small formatting and label helpers
```

The project uses `clap` for CLI parsing, `rustyline` for editable interactive prompts, `serde` for serialization, `toml` for config persistence, `serde_json` for JSON output and yt-dlp probe parsing, and `std::process::Command` for external tools. Unit tests live beside the config, downloader, and process helpers they cover.

## Implemented Data Flow

```text
CLI argv
    -> clap parser
    -> run
        -> resolve config path
        -> load_config
        -> command handler
            -> optional config write
            -> optional external process call
            -> optional process log write
    -> exit code
```

`src/main.rs` owns command handling, prompts, and console rendering. Domain types, config persistence, process execution, paths, and downloader argument construction live in separate modules.

## Domain Model

The Rust domain model defines:

* `SourceType` with `YouTube` and `Podcast`.
* `Target`, keyed by target name, including `urls`, `source`, boolean `subdir`, and `output_template`.
* `Config`, including YouTube and podcast roots, default templates, targets, and optional TOML option blocks.
* `ProcessResult` for external process outcomes.

Source inference is intentionally simple:

* YouTube mode is selected for YouTube, youtu.be, SoundCloud, and unknown URLs.
* Podcast mode is selected for feed/RSS/XML-looking URLs and a few known podcast hosts.
* Explicit target source wins over inference.

## Paths

Default paths:

```text
youtube_dir default: ~/Videos/YouTube
podcast_dir default: ~/Music/Podcasts
config dir default: ~/.config/archivist
config file: <config dir>/config.toml
logs dir: <config dir>/logs
```

`ARCHIVIST_CONFIG_DIR` overrides the config directory. The global `--config-file` CLI option overrides the config file path for a run.

## Config Store

Config is loaded and saved as TOML.

Current config fields:

```toml
youtube_dir = "..."
podcast_dir = "..."
default_youtube_template = "..."
default_podcast_template = "..."

[targets.example]
urls = ["..."]
source = "youtube"
subdir = true
```

`yt_dlp_options` and `podcast_dl_options` are parsed and persisted as TOML arrays of strings. YouTube options are prepended to `yt-dlp` sync arguments. Podcast options are inserted after `deno x podcast-dl` and before Archivist's generated podcast-dl sync arguments.

## Process Runner

External process execution returns:

```rust
struct ProcessResult {
    exit_code: i32,
    stdout: String,
    stderr: String,
}
```

Sync redirects stdout and stderr into the per-target log file as the subprocess runs, waits synchronously, and then reports the same end-of-run summary to the terminal. It does not currently support cancellation, terminal progress streaming, or environment customization.

## yt-dlp Integration

The yt-dlp path provides:

* Metadata probing with `yt-dlp --dump-json --skip-download --playlist-end 1 <url>`.
* Label suggestions from channel handle, uploader id, channel, or uploader.
* Sync arguments using `--download-archive`, `--paths`, and `-o`.

The archive file for a YouTube target is:

```text
<youtube_dir>/<label>/.download-archive.txt
```

Targets can store one or more URLs. Sync passes all stored YouTube URLs to one yt-dlp invocation sharing the same archive file. When adding a YouTube URL ending in `/playlists`, Archivist can add the base URL to the target URL list as well.

## podcast-dl Integration

Podcast support runs podcast-dl through Deno:

```text
deno x podcast-dl ...
```

It provides:

* Feed title probing with `deno x podcast-dl --info --url <url>`.
* Sync arguments using `--out-dir`, `--episode-template`, `--archive`, `--include-meta`, and `--include-episode-meta`.

Podcast output is rooted at:

```text
<podcast_dir>/{{podcast_title}}
```

The podcast archive template is:

```text
<podcast_dir>/{{podcast_title}}/archive.json
```

## CLI Behavior

Implemented commands:

```text
list [--all|name]
config show [property]
config set <property> [value]
config edit
probe <name>
info <name>
sync [--all|name]
add [--url URL]... [--label LABEL] [--output TEMPLATE] [--type auto|youtube|podcast] [--subdir] [--include-all]
remove <name> [--delete-archive]
```

Implemented global options:

```text
--config-file, -c
--json, -j
--quiet
--version, -v
```

JSON output is implemented for `list`, `config show`, and `probe`. `info` prints raw downloader output. Human-readable `list` output starts with the configured default templates. `config edit` opens the config without parsing it first, so it can be used to repair invalid TOML. Sync still prints human-readable process status and streams subprocess output to process logs.

## Error Handling

Expected errors are mostly represented as `Result<_, String>` at function boundaries. Command handlers convert those results to messages and exit codes.

Current exit code conventions:

* `0` for success.
* `1` for config, external process, or command execution failures.
* `2` for usage errors.
* Sync returns the last non-zero downloader exit code if any target fails.

## F# vs Rust Notes

The old F# version leaned on small modules, discriminated unions, records, Argu, `System.Text.Json`, and .NET tasks. That made command flow concise and naturally expression-oriented.

The Rust version uses explicit structs/enums, `clap` derive macros, `serde`, TOML persistence, and ownership-aware data flow. The tradeoff is more boilerplate, but the compiled binary has no .NET runtime dependency and config serialization is strongly tied to the Rust data model.

## Future Layering Direction

Possible deeper layering, if the code grows further:

```text
src/cli.rs
src/config.rs
src/domain.rs
src/downloaders/yt_dlp.rs
src/downloaders/podcast_dl.rs
src/process.rs
src/main.rs
```

That split should happen only when the code size or test surface justifies it. Until then, prefer keeping functions focused and avoiding broad abstractions.

## Future GUI Requirements To Preserve

To keep a GUI path open, avoid adding behavior that depends on console output as the only state channel.

Useful future work:

* Structured sync result values.
* Dry-run support.
* Cancellation and progress events for process execution.
* Tests for config parsing, target validation, template resolution, and argument construction.
* Moving prompt/rendering code out of orchestration once application services exist.
