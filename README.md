# Archivist

Archivist is a personal media archiving CLI for downloading and organizing media from YouTube-style sources and podcast feeds.

The current implementation is a single F#/.NET executable. It stores named targets in a JSON config file, shells out to external downloaders, and writes per-sync process logs. The long-term direction is still to keep the core logic clean enough that the same behavior can later power a GUI.

## Requirements

* .NET 10 SDK
* `yt-dlp` on `PATH` for YouTube, YouTube playlist, SoundCloud, and other yt-dlp-supported sources
* `deno` on `PATH` for podcast targets, because podcast sync runs `deno x podcast-dl ...`

## Build

```bash
dotnet build
```

Run locally with:

```bash
dotnet run -- --help
```

The executable reports version `0.1.0`.

## Configuration

By default, Archivist reads and writes:

```text
~/.config/archivist/config.json
```

Set `ARCHIVIST_CONFIG_DIR` to change the config directory, or pass `--config-file`/`-c` to use a specific config file:

```bash
archivist --config-file ./config.json list
```

Default media directories are:

```text
~/Videos/YouTube
~/Music/Podcasts
```

Current config shape:

```json
{
  "youtube_dir": "/Users/me/Videos/YouTube",
  "podcast_dir": "/Users/me/Music/Podcasts",
  "default_youtube_template": "%(playlist)s/%(upload_date>%Y-%m-%d)s - %(title)s.%(ext)s",
  "default_podcast_template": "{{release_year}}-{{release_month}}-{{release_day}} - {{title}}",
  "targets": [
    {
      "name": "youtube-linux",
      "url": "https://www.youtube.com/example",
      "mode": "youtube",
      "subdir": "youtube-linux",
      "output_template": null
    },
    {
      "name": "my-podcast",
      "url": "https://example.com/feed.xml",
      "mode": "podcast",
      "subdir": "my-podcast",
      "output_template": null
    }
  ],
  "yt_dlp_options": null,
  "podcast_dl_options": null
}
```

`yt_dlp_options` and `podcast_dl_options` can be stored with `config set`, but the current sync code does not yet apply those option blocks to generated downloader arguments.

Legacy configs with an `entries` object and fields such as `base_dir`, `default_output_template`, and `outputTemplate` are still parsed.

## Commands

Global options:

```bash
archivist --json list
archivist --quiet sync my-target
archivist --config-file ./config.json config show
archivist --version
```

Target management:

```bash
archivist add --url https://www.youtube.com/example --label youtube-linux --type youtube
archivist add --url https://example.com/feed.xml --label my-podcast --type podcast
archivist add --url https://example.com/feed.xml --type podcast --output "{{title}}"
archivist list
archivist probe my-podcast
archivist remove my-podcast
archivist remove my-podcast --delete-archive
```

If `--label` is omitted, Archivist probes the source and prompts for a label. YouTube labels are probed with `yt-dlp --dump-json`; podcast labels are probed with `deno x podcast-dl --info`.

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
archivist config set yt_dlp_options '{"ignoreerrors":true}'
archivist config set podcast_dl_options
```

Passing no value to `yt_dlp_options`, `podcast_dl_options`, or `targets` clears that property. Directory properties require a value.

Recognized config property aliases include:

```text
youtube_dir, youtube-dir, base_dir, base-dir
podcast_dir, podcast-dir
default_youtube_template, default-youtube-template, default_output_template
default_podcast_template, default-podcast-template, podcast_template
targets
yt_dlp_options, yt_dlp_opts, yt_dlp, yt-dlp
podcast_dl_options, podcast_dl_opts, podcast_dl, podcast-dl
```

## Download layout

YouTube sync runs:

```text
yt-dlp --download-archive <youtube_dir>/<label>/.download-archive.txt --paths <youtube_dir> -o <template> <url>
```

If a target has no `output_template`, its output template is:

```text
<target subdir>/<default_youtube_template>
```

Podcast sync runs:

```text
deno x podcast-dl --url <url> --out-dir <podcast_dir>/{{podcast_title}} --threads 3 --episode-template <template> --archive <podcast_dir>/{{podcast_title}}/archive.json --include-meta --include-episode-meta
```

For podcast targets, `--output` overrides only the episode filename template. The podcast directory remains `{{podcast_title}}` under `podcast_dir`.

Sync logs are written to:

```text
~/.config/archivist/logs/sync-<label>-<timestamp>.log
```

## Current implementation

The code is organized as a compact single project:

```text
Domain.fs         Domain records, command model, source type inference
Paths.fs          Default directories, config path, archive paths
ConfigStore.fs    JSON config parsing and persistence
ProcessRunner.fs  External process wrapper
YtDlp.fs          yt-dlp probing and sync argument construction
PodcastDl.fs      podcast-dl probing and sync argument construction
Cli.fs            Argu command-line parser
Program.fs        Command handlers and orchestration
```

See [docs/architecture.md](docs/architecture.md) for the current architecture notes and the future layering direction.
