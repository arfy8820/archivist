# Archivist

Archivist is a personal media archiving CLI for downloading and organizing media from YouTube-style sources and podcast feeds.

The current implementation is a Rust/Cargo binary. It stores named targets in a TOML config file, shells out to external downloaders, and writes per-sync process logs.

## Requirements

* Rust stable toolchain
* `yt-dlp` on `PATH` for YouTube, YouTube playlist, SoundCloud, and other yt-dlp-supported sources
* `deno` on `PATH` for podcast targets, because podcast sync runs `deno x podcast-dl ...`

## Build

```bash
cargo build
```

Run locally with:

```bash
cargo run -- --help
```

The executable reports version `0.5.0`.

## Configuration

By default, Archivist reads and writes:

```text
~/.config/archivist/config.toml
```

Set `ARCHIVIST_CONFIG_DIR` to change the config directory, or pass `--config-file`/`-c` to use a specific config file:

```bash
archivist --config-file ./config.toml list
```

To create a new default config at an explicit path:

```bash
archivist --config-file ./config.toml
```

Default media directories are:

```text
~/Videos/YouTube
~/Music/Podcasts
```

Current config shape:

```toml
youtube_dir = "/Users/me/Videos/YouTube"
podcast_dir = "/Users/me/Music/Podcasts"
default_youtube_template = "%(playlist)s/%(upload_date>%Y-%m-%d)s - %(title)s.%(ext)s"
default_podcast_template = "{{release_year}}-{{release_month}}-{{release_day}} - {{title}}"

[targets.youtube-linux]
urls = ["https://www.youtube.com/example"]
source = "youtube"
subdir = true

[targets.my-podcast]
urls = ["https://example.com/feed.xml"]
source = "podcast"
subdir = true
```

Targets may store one or more URLs:

```toml
[targets.example]
urls = [
  "https://www.youtube.com/@example/playlists",
  "https://www.youtube.com/@example",
]
source = "youtube"
```

`yt_dlp_options` and `podcast_dl_options` are TOML arrays of strings. They are added before Archivist's generated sync arguments for each downloader.

## Commands

Global options:

```bash
archivist --json list
archivist --quiet sync my-target
archivist --config-file ./config.toml config show
archivist --version
```

Target management:

```bash
archivist add --url https://www.youtube.com/example --label youtube-linux --type youtube
archivist add --url https://www.youtube.com/example --label youtube-linux --type youtube --subdir
archivist add --url https://www.youtube.com/@example/playlists --label example --type youtube --include-all
archivist add --url https://example.com/feed.xml --label my-podcast --type podcast
archivist add --url https://example.com/feed.xml --type podcast --output "{{title}}"
archivist list
archivist list --all
archivist list my-podcast
archivist probe my-podcast
archivist remove my-podcast
archivist remove my-podcast --delete-archive
```

`list` without a target lists all configured targets. `list <target>` lists one target, and `list --all` explicitly lists all targets.

If `--label` is omitted, Archivist probes the first URL and prompts for a label. YouTube labels are probed with `yt-dlp --dump-json`; podcast labels are probed with `deno x podcast-dl --info`. `add` prompts whether to store downloads in a target subdirectory unless `--subdir` is passed. When no `--url` is supplied, interactive add asks whether to add another URL to the target.

When adding a YouTube URL ending in `/playlists`, Archivist asks whether to also add the base URL to capture videos not in a playlist. `--include-all` answers yes without prompting.

Sync:

```bash
archivist sync
archivist sync --all
archivist sync youtube-linux
```

`sync` without a target syncs all configured targets.

Configuration:

```bash
archivist config show
archivist config show youtube_dir
archivist config set youtube_dir /Volumes/archive/youtube
archivist config set podcast_dir /Volumes/archive/podcasts
archivist config set default_youtube_template "%(playlist)s/%(upload_date>%Y-%m-%d)s - %(title)s.%(ext)s"
archivist config set default_podcast_template "{{release_year}}-{{release_month}}-{{release_day}} - {{title}}"
archivist config set yt_dlp_options '["--ignore-errors", "--no-warnings"]'
archivist config set podcast_dl_options '["--debug"]'
archivist config set podcast_dl_options
archivist config edit
```

Passing no value to `yt_dlp_options`, `podcast_dl_options`, or `targets` clears that property. Directory properties require a value.

`config edit` opens the config file in `$VISUAL` or `$EDITOR` if set. Otherwise, Archivist falls back to the platform text editor opener.

Recognized config property aliases include:

```text
youtube_dir, youtube-dir
podcast_dir, podcast-dir
default_youtube_template, default-youtube-template
default_podcast_template, default-podcast-template
targets
yt_dlp_options, yt_dlp_opts, yt_dlp, yt-dlp
podcast_dl_options, podcast_dl_opts, podcast_dl, podcast-dl
```

## Download Layout

YouTube sync runs:

```text
yt-dlp --download-archive <youtube_dir>/<label>/.download-archive.txt --paths <youtube_dir> -o <template> <url> [<url>...]
```

If a target has `subdir = true`, YouTube `--paths` points at `<youtube_dir>/<target key>`. If `subdir` is false or omitted, `--paths` points at `<youtube_dir>`.

Podcast sync runs:

```text
deno x podcast-dl --url <url> --out-dir <podcast_dir>/{{podcast_title}} --threads 3 --episode-template <template> --archive <podcast_dir>/{{podcast_title}}/archive.json --include-meta --include-episode-meta
```

For podcast targets, `--output` overrides only the episode filename template. The podcast directory remains `{{podcast_title}}` under `podcast_dir`.

Sync logs are written to:

```text
~/.config/archivist/logs/sync-<label>-<timestamp>.log
```

During sync, downloader stdout and stderr are streamed into the log file as the subprocess runs. The terminal still shows the per-target summary after the subprocess exits.

## F# vs Rust Approach

The F# version used multiple small modules and immutable records to make the CLI flow explicit inside a .NET executable. JSON config came from `System.Text.Json`, CLI parsing came from Argu, and async process execution used .NET tasks.

The Rust version uses a Cargo binary with `clap` for command parsing, `serde` for TOML and JSON output, and `std::process::Command` for downloader execution. The domain model is represented by Rust structs and enums with explicit ownership, and recoverable failures are handled with `Result`.

The main behavior difference is configuration: Rust writes TOML at `config.toml` by default rather than JSON at `config.json`. JSON is still supported for CLI output via `--json`, but config persistence is TOML-first.

## Current Implementation

The code is organized as a compact Rust project:

```text
Cargo.toml          Package metadata and dependencies
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

See [docs/architecture.md](docs/architecture.md) for architecture notes and future layering direction.
