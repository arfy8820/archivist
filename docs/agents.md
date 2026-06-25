# AGENTS.md

## Project

Archivist is a Rust media archiving CLI.

The current repo is a Cargo binary crate. It uses `yt-dlp` for YouTube-style sources and `deno x podcast-dl` for podcast feeds.

## Current Files

```text
Cargo.toml          Package metadata and dependencies
src/main.rs         CLI parser, prompts, rendering, and command handlers
src/types.rs        Domain model and defaults
src/config.rs       TOML config loading, saving, and config command helpers
src/yt_dlp.rs       yt-dlp probing, playlist URL expansion, and sync arguments
src/podcast_dl.rs   podcast-dl probing and sync arguments
src/process.rs      Subprocess execution and streamed log writing
src/paths.rs        Config, archive, and log path helpers
src/util.rs         Small formatting and label helpers
```

## Important Design Rule

Keep core behavior independent from the user interface where practical.

`src/main.rs` contains orchestration and console rendering. Keep new business rules in focused modules where practical instead of parser cases or print-only code paths.

## Coding Style

* Prefer clear Rust structs and enums.
* Prefer `Result<T, String>` or specific error types for recoverable failures.
* Avoid panics for expected user/configuration errors.
* Avoid adding heavy dependencies unless justified.
* Keep process execution behind a focused helper.
* Keep file system access out of pure domain logic.
* Use TOML for persisted config.

## CLI Behavior To Preserve

Implemented commands:

```text
list
config show [property]
config set <property> [value]
probe <name>
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

`sync` with no target means all targets. `add` may prompt for URL, output template, label, YouTube playlist expansion, and subdirectory behavior when options are omitted. `--url` may be supplied multiple times. In interactive mode, `add` asks whether to add another URL to the target. `--subdir` stores the target under a key-named subdirectory by setting `subdir = true`. `--include-all` adds the base URL for YouTube URLs ending in `/playlists` without prompting.

## Config Shape

Current TOML config fields:

```text
youtube_dir
podcast_dir
default_youtube_template
default_podcast_template
targets
yt_dlp_options
podcast_dl_options
```

`yt_dlp_options` and `podcast_dl_options` are TOML arrays of strings. YouTube options are prepended to `yt-dlp` sync arguments. Podcast options are inserted after `deno x podcast-dl` and before Archivist's generated podcast-dl sync arguments.

## External Tools

Archivist may call:

* `yt-dlp`
* `deno x podcast-dl`

Do not scatter raw process calls throughout the codebase. Use or extend the existing process helper.

Log external tool commands clearly, but avoid leaking credentials or private tokens if authenticated URLs or headers are later supported.

During sync, subprocess stdout and stderr should stream into the per-target log file while the subprocess runs. Keep terminal output focused on command start and end-of-run summaries unless a later feature explicitly adds live terminal progress.

## Sync Behavior

Sync should be predictable and safe.

* Do not delete downloaded media unless explicitly requested.
* Do not silently rewrite unrelated config fields.
* Keep target names stable.
* Preserve archive files unless the user passes `remove --delete-archive`.
* Keep YouTube and podcast template behavior separate.

Current archive paths:

```text
YouTube: <youtube_dir>/<label>/.download-archive.txt
Podcast: <podcast_dir>/{{podcast_title}}/archive.json
```

## Podcast Support

Podcast support is first-class but currently delegates identity/archive behavior to podcast-dl.

When adding podcast features:

* Keep podcast naming templates separate from yt-dlp templates.
* Avoid assuming every episode has a clean title, date, duration, season, or episode number.
* Prefer stable episode identity using GUID if Archivist later owns podcast state.
* Fall back carefully to enclosure URL or another stable value.

## Future Architecture

Possible future layering, if the code grows further:

```text
src/cli.rs
src/config.rs
src/domain.rs
src/downloaders/yt_dlp.rs
src/downloaders/podcast_dl.rs
src/process.rs
src/main.rs
```

Do not force deeper layering prematurely. When the code grows enough to justify it, move behavior along the boundaries described in `docs/architecture.md`.

## Testing Expectations

There are focused unit tests in the config, downloader, and process modules. When adding tests, prefer pure or near-pure behavior:

* Config parsing.
* Source type parsing and inference.
* Target validation.
* Template/argument construction.
* Command argument parsing where useful.

Avoid tests that require real network downloads unless they are explicitly integration tests.
