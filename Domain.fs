module Archivist.Domain

open System
open System.Text.Json
open System.Text.Json.Serialization

type SourceType =
    | YouTube
    | Podcast

let sourceTypeName sourceType =
    match sourceType with
    | YouTube -> "youtube"
    | Podcast -> "podcast"

let tryParseSourceType (value: string) =
    if String.IsNullOrWhiteSpace value then
        None
    else
        match value.Trim().ToLowerInvariant() with
        | "youtube"
        | "yt"
        | "yt-dlp" -> Some YouTube
        | "podcast"
        | "podcast-dl"
        | "rss" -> Some Podcast
        | _ -> None

type Target =
    { name: string
      url: string
      mode: string
      subdir: string option
      [<JsonPropertyName("output_template")>]
      outputTemplate: string option }

let private targetMode (target: Target) =
    if String.IsNullOrWhiteSpace target.mode then "auto"
    else target.mode.Trim().ToLowerInvariant()

let private inferSourceType (url: string) =
    let value = if isNull url then "" else url.ToLowerInvariant()

    if value.Contains("youtube.com")
       || value.Contains("youtu.be")
       || value.Contains("soundcloud.com") then
        YouTube
    elif value.Contains("feed")
         || value.Contains("rss")
         || value.EndsWith(".xml")
         || value.Contains("libsyn.com")
         || value.Contains("megaphone.fm")
         || value.Contains("supportingcast.fm") then
        Podcast
    else
        YouTube

let targetSourceType (target: Target) =
    match targetMode target with
    | "podcast"
    | "podcast-dl"
    | "rss" -> Podcast
    | "youtube"
    | "yt"
    | "yt-dlp" -> YouTube
    | _ -> inferSourceType target.url

type Config =
    { [<JsonPropertyName("base_dir")>]
      youtubeDir: string
      [<JsonPropertyName("podcast_dir")>]
      podcastDir: string
      [<JsonPropertyName("default_youtube_template")>]
      defaultYoutubeTemplate: string
      [<JsonPropertyName("default_podcast_template")>]
      defaultPodcastTemplate: string
      targets: Target list
      [<JsonPropertyName("yt_dlp")>]
      ytDlp: JsonElement option }

type ProcessResult =
    { exitCode: int
      stdout: string
      stderr: string }

type SyncTarget =
    | One of label: string
    | All

type ConfigProperty =
    | AllProperties
    | BaseDir
    | PodcastDir
    | DefaultOutputTemplate
    | DefaultPodcastTemplate
    | Targets
    | YtDlp

type ConfigAction =
    | Show of ConfigProperty
    | Set of ConfigProperty * value: string option

type AddRequest =
    { url: string option
      label: string option
      outputTemplate: string option
      sourceType: SourceType option }

type ResolvedAdd =
    { label: string
      target: Target }

type Command =
    | Add of AddRequest
    | Remove of label: string * removeArchive: bool
    | List
    | Sync of SyncTarget
    | Config of ConfigAction
    | Probe of name: string
    | Version
    | Usage of error: string option

type GlobalOptions =
    { configPath: string option
      emitJson: bool
      quiet: bool }

type ParsedInput =
    { options: GlobalOptions
      command: Command }

type ProbeInfo =
    { channel: string option
      channelId: string option
      channelHandle: string option
      uploader: string option
      uploaderId: string option }

type ProbeOutcome =
    | ProbeSuccess of ProbeInfo
    | ProbeFailed of error: string
