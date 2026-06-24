# AGENTS.md

## Project

Archivist is a Rust media archiving CLI.

The current repo is a single Cargo binary crate. It uses `yt-dlp` for YouTube-style sources and `deno x podcast-dl` for podcast feeds.

## Current Files

```text
Cargo.toml      Package metadata and dependencies
src/main.rs     CLI parser, domain model, TOML config, process calls, prompts, rendering
```

## Important Design Rule

Keep core behavior independent from the user interface where practical.

`src/main.rs` currently contains orchestration and console rendering because the CLI is still compact. Avoid putting new business rules directly into parser cases or print-only code paths. Prefer small functions that can later move into application/core modules.

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
import-json <input> [--output PATH] [--force]
probe <name>
sync [--all|name]
add [--url URL] [--label LABEL] [--output TEMPLATE] [--type auto|youtube|podcast] [--subdir] [--include-all]
remove <name> [--delete-archive]
```

Implemented global options:

```text
--config-file, -c
--json, -j
--quiet
--version, -v
```

`sync` with no target means all targets. `add` may prompt for URL, output template, label, playlist expansion, and subdirectory behavior when options are omitted. `--subdir` stores the target under a label-named subdirectory. `--include-all` stores the URL without `/playlists` for matching YouTube playlist collection URLs.

`import-json` converts the old Archivist JSON config to the current TOML shape. It supports the recent `targets` array shape and the legacy `entries` object shape.

## Config Compatibility

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

`yt_dlp_options` and `podcast_dl_options` are currently persisted but not applied to downloader arguments.

## External Tools

Archivist may call:

* `yt-dlp`
* `deno x podcast-dl`

Do not scatter raw process calls throughout the codebase. Use or extend the existing process helper.

Log external tool commands clearly, but avoid leaking credentials or private tokens if authenticated URLs or headers are later supported.

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

The likely future split is:

```text
src/domain.rs
src/config.rs
src/downloaders/ytdlp.rs
src/downloaders/podcast_dl.rs
src/process.rs
src/cli.rs
src/main.rs
```

Do not force that split prematurely. When the code grows enough to justify it, move behavior along the boundaries described in `docs/architecture.md`.

## Testing Expectations

There is no test module yet. When adding tests, start with pure or near-pure behavior:

* Config parsing.
* Source type parsing and inference.
* Target validation.
* Template/argument construction.
* Command argument parsing where useful.

Avoid tests that require real network downloads unless they are explicitly integration tests.
