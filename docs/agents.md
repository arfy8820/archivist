# AGENTS.md

## Project

Archivist is an F#/.NET media archiving CLI.

The current repo is a single executable project, not yet a multi-project `src/`/`tests/` solution. It uses `yt-dlp` for YouTube-style sources and `deno x podcast-dl` for podcast feeds.

## Current Files

```text
Domain.fs         Domain records, command model, source type parsing/inference
Paths.fs          Config/media/log/archive paths
ConfigStore.fs    JSON config load/save and legacy config parsing
ProcessRunner.fs  External process execution
YtDlp.fs          yt-dlp probe and sync arguments
PodcastDl.fs      podcast-dl probe and sync arguments
Cli.fs            Argu parser
Program.fs        Command handlers, prompts, orchestration, rendering
```

Compile order is controlled explicitly in `archivist.fsproj`. Keep that order in mind when adding modules.

## Important Design Rule

Keep core behavior independent from the user interface where practical.

`Program.fs` currently contains orchestration and console rendering because the CLI is still compact. Avoid putting new business rules directly into parser cases or print-only code paths. Prefer small functions or modules that can later move into application/core layers.

## Coding Style

* Prefer clear F# domain types.
* Prefer small modules with focused responsibilities.
* Prefer immutable data.
* Prefer `Result<'T, string>` or a specific error type for recoverable failures.
* Avoid throwing exceptions for expected user/configuration errors.
* Avoid adding heavy dependencies unless justified.
* Keep process execution behind `ProcessRunner` or similarly small adapter modules.
* Keep file system access out of pure domain logic.
* Preserve legacy config parsing where practical.

## CLI Behavior To Preserve

Implemented commands:

```text
list
config show [property]
config set <property> [value]
probe <name>
sync [--all|name]
add [--url URL] [--label LABEL] [--output TEMPLATE] [--type auto|youtube|podcast]
remove <name> [--delete-archive]
```

Implemented global options:

```text
--config-file, -c
--json, -j
--quiet
--version, -v
```

`sync` with no target means all targets. `add` may prompt for URL, output template, and label when options are omitted.

## Config Compatibility

Current config fields:

```text
youtube_dir
podcast_dir
default_youtube_template
default_podcast_template
targets
yt_dlp_options
podcast_dl_options
```

Legacy fields such as `base_dir`, `default_output_template`, `entries`, `sourceType`, and `outputTemplate` are still parsed. Do not break that compatibility casually.

`yt_dlp_options` and `podcast_dl_options` are currently persisted but not applied to downloader arguments.

## External Tools

Archivist may call:

* `yt-dlp`
* `deno x podcast-dl`

Do not scatter raw process calls throughout the codebase. Use `ProcessRunner.run` or a focused wrapper module.

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

The desired future split is still:

```text
Archivist.Core
Archivist.Application
Archivist.Infrastructure
Archivist.Cli
```

Do not force that split prematurely. When the code grows enough to justify it, move behavior along the boundaries described in `docs/architecture.md`.

## Testing Expectations

There is no test project yet. When adding tests, start with pure or near-pure behavior:

* Config parsing and legacy compatibility.
* Source type parsing and inference.
* Target validation.
* Template/argument construction.
* Command argument parsing where useful.

Avoid tests that require real network downloads unless they are explicitly integration tests.
