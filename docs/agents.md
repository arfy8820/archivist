# AGENTS.md

## Project

Archivist is an F# media archiving tool.

It currently focuses on command-line workflows using `yt-dlp`, with manual `podcast-dl` integration being developed. The long-term direction is to expose the same core archiving engine through both a CLI and a GUI application.

## Important design rule

Keep the core logic independent from the user interface.

Do not put business logic directly in CLI command handlers. Command handlers should parse input, call application services, and render results.

## Preferred architecture

```text
src/
    Archivist.Core/
        Domain types
        Template resolution
        Sync planning
        Validation
        Pure logic

    Archivist.Application/
        Use cases
        Target management
        Sync orchestration
        Result shaping

    Archivist.Infrastructure/
        yt-dlp integration
        podcast-dl integration
        File system
        Process runner
        Config persistence

    Archivist.Cli/
        Command-line parser
        Human-readable output
        JSON output

tests/
    Archivist.Tests/
```

This structure can be adjusted to match the existing repository, but preserve the separation of concerns.

## Coding style

* Prefer clear F# domain types.
* Prefer small modules with focused responsibilities.
* Prefer immutable data.
* Prefer `Result<'T, 'Error>` for recoverable failures.
* Avoid throwing exceptions for expected user/configuration errors.
* Avoid adding heavy dependencies unless justified.
* Keep process execution wrapped behind interfaces or small adapter modules.
* Keep file system access out of pure domain logic.

## Domain modelling preferences

Use explicit types for important concepts.

Good:

```fsharp
type TargetId = TargetId of string
type SourceUrl = SourceUrl of string
type TemplateName = TemplateName of string
```

Avoid passing raw strings everywhere once a concept is important.

## Sync behaviour

Sync should be predictable and safe.

* Do not silently rewrite existing archive metadata.
* Do not delete downloaded media unless explicitly requested.
* Prefer dry-run support for dangerous or large operations.
* Log external tool commands clearly, but avoid leaking credentials or private tokens.
* Keep target IDs stable.
* Preserve backward compatibility for existing config files where practical.

## External tools

Archivist may call external tools such as:

* `yt-dlp`
* `podcast-dl`
* `ffmpeg`

External process execution should be wrapped so it can be tested.

Do not scatter raw process calls throughout the codebase.

## Podcast support

Podcast support is being integrated manually as a learning exercise.

When adding podcast features:

* Treat podcast feeds as first-class targets.
* Prefer stable episode identity using GUID when available.
* Fall back carefully to enclosure URL or another stable value.
* Keep podcast naming templates separate from YouTube assumptions.
* Avoid assuming every episode has a clean title, date, duration, or season/episode number.

## GUI future

Design with a future GUI in mind.

The GUI will need:

* A way to list targets.
* A way to inspect planned downloads.
* Progress reporting.
* Cancellation.
* Structured errors.
* Machine-readable sync results.

Therefore, avoid printing directly from deep application logic. Return structured results and let the CLI render them.

## Testing expectations

Add tests for:

* Template resolution.
* Config parsing.
* Target validation.
* Sync planning.
* Podcast episode identity selection.
* Command argument parsing where useful.

Prefer tests around pure functions first.

## Accessibility

The eventual GUI should be screen-reader friendly.

Prefer native controls where possible. Avoid custom UI widgets unless they expose proper accessibility information.
