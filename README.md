# Archivist

Archivist is a personal media archiving tool for downloading, organizing, and syncing media from supported sources.

The initial focus is command-line archiving using `yt-dlp`, with podcast support being added via `podcast-dl`. The long-term goal is to keep the core logic clean enough that it can later power both a CLI and a GUI application.

## Goals

* Archive YouTube channels, playlists, individual videos, and podcast feeds.
* Support multiple named archive targets.
* Keep folder and filename layout configurable.
* Avoid re-downloading items that have already been archived.
* Produce useful logs and machine-readable output.
* Keep the core logic independent of the command-line interface.
* Eventually support a GUI frontend over the same core engine.

## Non-goals, for now

* Replacing `yt-dlp`.
* Replacing `podcast-dl`.
* Building a database-heavy media manager.
* Building the GUI before the CLI/domain model is stable.

## Core concepts

### Target

A named archive source.

Examples:

* A YouTube channel.
* A YouTube playlist.
* A podcast feed.
* A one-off URL list.

A target should have a stable ID, a source URL, a source type, and an output/template configuration.

### Template

A template controls where downloaded files go and how they are named.

Templates should eventually support global defaults, per-target overrides, and possibly per-source-type defaults.

### Sync

A sync operation checks a target, determines which items are missing, downloads them, writes metadata, and updates archive state.

### Archive state

Archive state records what has already been downloaded so repeated syncs do not duplicate work.

For YouTube-style sources, this may use yt-dlp archive files.

For podcasts, this may use GUIDs, enclosure URLs, episode dates, or another stable identifier.

## Example commands

```bash
archivist sync all
archivist sync youtube-linux
archivist add-target youtube-linux https://www.youtube.com/example
archivist list-targets
archivist show-target youtube-linux
```

## Future GUI direction

The GUI should not directly contain download logic.

Preferred shape:

```text
Archivist.Core
    Domain model
    Sync planning
    Template resolution
    Result types
    Configuration model

Archivist.Infrastructure
    yt-dlp runner
    podcast-dl runner
    File system access
    Process execution
    Logging adapters

Archivist.Cli
    Command parsing
    Console output
    JSON output

Archivist.Gui
    Windows/macOS/Linux UI
    Calls into Core/Application services
```

The CLI is the proving ground. The GUI should reuse the same application services later.