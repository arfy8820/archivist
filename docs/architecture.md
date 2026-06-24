# Architecture Notes

## Current Shape

Archivist is currently one F# executable project targeting `net10.0`.

The implementation is intentionally small and module-oriented rather than split into multiple projects:

```text
archivist.fsproj
Domain.fs
Paths.fs
ConfigStore.fs
ProcessRunner.fs
YtDlp.fs
PodcastDl.fs
Cli.fs
Program.fs
```

The project uses Argu for CLI parsing and shells out to external download tools. There are no dedicated test projects yet.

## Implemented Data Flow

```text
CLI argv
    -> Cli.parseArgs
    -> Program.runMain
        -> resolve config path
        -> ConfigStore.loadFrom
        -> command handler
            -> optional config write
            -> optional external process call
            -> optional process log write
    -> exit code
```

`Program.fs` is currently the orchestration layer. It still contains user prompts, command handling, sync orchestration, and console rendering. That is acceptable for the current compact CLI, but new domain behavior should be kept out of the parser and isolated in focused modules where practical.

## Current Modules

### Domain

`Domain.fs` defines:

* `SourceType` with `YouTube` and `Podcast`.
* `Target`, including `name`, `url`, optional `urls`, `mode`, `subdir`, and `output_template`.
* `Config`, including YouTube and podcast roots, default templates, targets, and optional JSON option blocks.
* CLI command and parsed input types.
* Source type parsing and inference.

Source inference is intentionally simple:

* YouTube mode is selected for YouTube, youtu.be, SoundCloud, and unknown URLs.
* Podcast mode is selected for feed/RSS/XML-looking URLs and a few known podcast hosts.
* Explicit target mode wins over inference.

### Paths

`Paths.fs` owns default paths:

```text
youtube_dir default: ~/Videos/YouTube
podcast_dir default: ~/Music/Podcasts
config dir default: ~/.config/archivist
config file: <config dir>/config.json
logs dir: <config dir>/logs
```

`ARCHIVIST_CONFIG_DIR` overrides the config directory. The global `--config-file` CLI option overrides the config file path for a run.

### Config Store

`ConfigStore.fs` loads and saves JSON config.

It accepts the current config shape:

```json
{
  "youtube_dir": "...",
  "podcast_dir": "...",
  "default_youtube_template": "...",
  "default_podcast_template": "...",
  "targets": [],
  "yt_dlp_options": null,
  "podcast_dl_options": null
}
```

It also reads legacy fields such as:

```text
base_dir
baseDir
default_output_template
defaultOutputTemplate
entries
outputTemplate
sourceType
```

`yt_dlp_options` and `podcast_dl_options` are parsed and persisted as raw JSON, but are not yet applied by `YtDlp.fs` or `PodcastDl.fs`.

### Process Runner

`ProcessRunner.fs` wraps external process execution and returns:

```fsharp
type ProcessResult =
    { exitCode: int
      stdout: string
      stderr: string }
```

It redirects stdout and stderr, waits asynchronously, and does not currently support cancellation, streaming progress, or environment customization.

### yt-dlp Integration

`YtDlp.fs` provides:

* Metadata probing with `yt-dlp --dump-json --skip-download --playlist-end 1 <url>`.
* Label suggestions from channel handle, uploader id, channel, or uploader.
* Sync arguments using `--download-archive`, `--paths`, and `-o`.

The archive file for a YouTube target is:

```text
<youtube_dir>/<label>/.download-archive.txt
```

### podcast-dl Integration

`PodcastDl.fs` runs podcast-dl through Deno:

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
list
config show [property]
config set <property> [value]
probe <name>
sync [--all|name]
add [--url URL] [--label LABEL] [--output TEMPLATE] [--type auto|youtube|podcast] [--subdir]
remove <name> [--delete-archive]
```

Implemented global options:

```text
--config-file, -c
--json, -j
--quiet
--version, -v
```

JSON output is implemented for `list`, `config show`, `probe`, and `version`. Sync still prints human-readable process status and writes process logs.

## Error Handling

Expected errors are mostly represented as `Result<_, string>` at module boundaries. Command handlers convert those results to messages and exit codes.

Current exit code conventions:

* `0` for success.
* `1` for config, external process, or command execution failures.
* `2` for usage errors.
* Sync returns the last non-zero downloader exit code if any target fails.

## Future Layering Direction

The desired long-term shape remains:

```text
Archivist.Core
    Domain model
    Template resolution
    Sync planning
    Validation

Archivist.Application
    Use cases
    Target management
    Sync orchestration
    Result shaping

Archivist.Infrastructure
    yt-dlp integration
    podcast-dl integration
    File system
    Process runner
    Config persistence

Archivist.Cli
    Command-line parser
    Human-readable output
    JSON output
```

That split should happen when the code size or test surface justifies it. Until then, prefer keeping modules focused and avoiding new dependencies or broad abstractions.

## Future GUI Requirements To Preserve

To keep a GUI path open, avoid adding behavior that depends on console output as the only state channel.

Useful future work:

* Structured sync result values.
* Dry-run support.
* Cancellation tokens for process execution.
* Progress events or streaming process output.
* Tests for config parsing, target validation, template resolution, and argument construction.
* Applying `yt_dlp_options` and `podcast_dl_options` consistently.
* Moving prompt/rendering code out of orchestration once application services exist.
