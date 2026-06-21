# Architecture Notes

## Overview

Archivist is built around a simple idea:

> Define archive targets, resolve their configuration, ask an external downloader for media, then store the result in a predictable layout without duplicating previous downloads.

The project should remain usable as a CLI while being structured so a GUI can later sit on top of the same core logic.

## Layers

### Core

The Core layer should contain pure or mostly pure logic.

Responsibilities:

* Domain types.
* Target definitions.
* Template definitions.
* Template resolution.
* Sync planning.
* Validation.
* Error types.

The Core layer should not know about Spectre.Console, Argu, System.CommandLine, Avalonia, WinUI, WPF, yt-dlp process flags, or podcast-dl process flags.

### Application

The Application layer coordinates use cases.

Responsibilities:

* Load configuration.
* Resolve targets.
* Build sync plans.
* Call downloader abstractions.
* Return structured results.
* Coordinate dry runs and real runs.

### Infrastructure

The Infrastructure layer talks to the outside world.

Responsibilities:

* Running `yt-dlp`.
* Running `podcast-dl`.
* Running `ffmpeg`, if needed.
* Reading/writing config files.
* Reading/writing archive state.
* File system operations.
* Logging adapters.

### CLI

The CLI layer should be thin.

Responsibilities:

* Parse commands.
* Call Application services.
* Print human-readable output.
* Print JSON output when requested.
* Return useful exit codes.

### GUI, future

The GUI should call the same Application layer as the CLI.

It should not shell out independently or duplicate sync logic.

## Suggested data flow

```text
CLI command
    -> Application use case
        -> Load config
        -> Resolve target/template
        -> Build sync plan
        -> Run downloader adapter
        -> Update archive state
        -> Return SyncResult
    -> CLI renderer
```

Future GUI flow:

```text
GUI action
    -> Application use case
        -> Same core sync logic
    -> GUI view model
```

## Target model

A target represents something Archivist can sync.

Possible fields:

```fsharp
type SourceKind =
    | YouTube
    | Podcast
    | UrlList

type Target =
    { Id: TargetId
      Name: string
      SourceKind: SourceKind
      Url: SourceUrl
      Template: TemplateName option
      OutputRoot: string option
      Enabled: bool }
```

## Template model

Templates should describe output structure without hard-coding source-specific assumptions.

Example concepts:

```fsharp
type Template =
    { Name: TemplateName
      DirectoryPattern: string
      FileNamePattern: string
      MetadataPattern: string option }
```

Possible placeholder values:

```text
{target_id}
{target_name}
{source_kind}
{channel}
{playlist}
{podcast}
{episode_title}
{upload_date}
{published_date}
{id}
{extension}
```

YouTube and podcast sources will not provide identical metadata, so template resolution should handle missing values deliberately.

## Sync result model

A sync operation should return structured information.

```fsharp
type DownloadStatus =
    | Downloaded
    | SkippedAlreadyArchived
    | Failed
    | Planned

type SyncItemResult =
    { Title: string option
      SourceUrl: string
      OutputPath: string option
      Status: DownloadStatus
      Message: string option }

type SyncResult =
    { TargetId: TargetId
      StartedAt: DateTimeOffset
      FinishedAt: DateTimeOffset option
      Items: SyncItemResult list }
```

This makes CLI output, JSON output, logging, and future GUI display much easier.

## Error handling

Prefer explicit errors for expected failures.

Examples:

```fsharp
type ArchivistError =
    | ConfigFileMissing of string
    | InvalidTargetId of string
    | UnknownTarget of string
    | TemplateNotFound of string
    | ExternalToolMissing of string
    | ExternalToolFailed of tool:string * exitCode:int * stderr:string
    | InvalidPodcastFeed of string
```

## Podcast design notes

Podcast support should not be treated as “YouTube but with different URLs.”

Important podcast-specific concerns:

* Episodes may have GUIDs.
* Episode titles may change.
* Enclosure URLs may change.
* Dates may be missing or inconsistent.
* Feeds may include old episodes, bonus episodes, trailers, or duplicates.
* Episode numbering is inconsistent across publishers.

Episode identity preference:

```text
1. GUID, if present and stable.
2. Enclosure URL.
3. Combination of title + published date + duration, as a last resort.
```

## Future GUI requirements to preserve now

To keep the GUI path open, the CLI implementation should avoid:

* Deep logic that prints directly to the console.
* Progress that is only available as text.
* Errors that only exist as formatted strings.
* Global mutable state.
* Hard-coded config paths without override support.

Useful future-friendly features:

* Dry-run mode.
* JSON output.
* Progress events.
* Cancellation token support.
* Structured logs.
* Clear separation between planning and execution.
