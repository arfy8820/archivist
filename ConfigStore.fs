module Archivist.ConfigStore

open System
open System.IO
open System.Text.Encodings.Web
open System.Text.Json
open Archivist.Domain
open Archivist.Paths

let private serializerOptions =
    let o = JsonSerializerOptions(WriteIndented = true)
    o.PropertyNamingPolicy <- null
    o.PropertyNameCaseInsensitive <- true
    o.Encoder <- JavaScriptEncoder.UnsafeRelaxedJsonEscaping
    o

let private stringOptionOfNullable (value: string | null) =
    if isNull value || String.IsNullOrWhiteSpace value then None else Some value

let private tryGetProperty (names: string list) (json: JsonElement) =
    names
    |> List.tryPick (fun name ->
        match json.TryGetProperty name with
        | true, value -> Some value
        | _ -> None)

let private tryGetString names json =
    match tryGetProperty names json with
    | Some value when value.ValueKind = JsonValueKind.String ->
        value.GetString() |> stringOptionOfNullable
    | _ -> None

let private tryGetJson names json =
    tryGetProperty names json |> Option.map (fun value -> value.Clone())

let defaultConfig () : Config =
    { youtubeDir = defaultYoutubeDir ()
      podcastDir = defaultPodcastDir ()
      defaultYoutubeTemplate = "%(playlist)s/%(upload_date>%Y-%m-%d)s - %(title)s.%(ext)s"
      defaultPodcastTemplate = "{{release_year}}-{{release_month}}-{{release_day}} - {{title}}"
      targets = []
      ytDlp = None }

let private parseNewTargets (root: JsonElement) =
    match tryGetProperty [ "targets" ] root with
    | Some targets when targets.ValueKind = JsonValueKind.Array ->
        targets.EnumerateArray()
        |> Seq.choose (fun item ->
            match tryGetString [ "name" ] item, tryGetString [ "url" ] item with
            | Some name, Some url ->
                Some
                    { name = name
                      url = url
                      mode = tryGetString [ "mode" ] item |> Option.defaultValue "auto"
                      subdir = tryGetString [ "subdir" ] item
                      outputTemplate = tryGetString [ "output_template"; "outputTemplate" ] item }
            | _ -> None)
        |> Seq.toList
    | _ -> []

let private parseLegacyTargets (root: JsonElement) =
    match tryGetProperty [ "entries" ] root with
    | Some entries when entries.ValueKind = JsonValueKind.Object ->
        entries.EnumerateObject()
        |> Seq.choose (fun property ->
            let item = property.Value
            match tryGetString [ "url" ] item with
            | Some url ->
                let mode =
                    tryGetString [ "sourceType"; "source_type" ] item
                    |> Option.defaultValue "youtube"

                Some
                    { name = property.Name
                      url = url
                      mode = mode
                      subdir = None
                      outputTemplate = tryGetString [ "outputTemplate"; "output_template" ] item }
            | None -> None)
        |> Seq.toList
    | _ -> []

let private parseConfig (json: string) =
    use doc = JsonDocument.Parse(json)
    let root = doc.RootElement
    let defaults = defaultConfig ()
    let targets = parseNewTargets root
    let targets = if List.isEmpty targets then parseLegacyTargets root else targets

    { youtubeDir = tryGetString [ "base_dir"; "youtube_dir"; "baseDir"; "youtubeDir" ] root |> Option.defaultValue defaults.youtubeDir
      podcastDir = tryGetString [ "podcast_dir"; "podcastDir" ] root |> Option.defaultValue defaults.podcastDir
      defaultYoutubeTemplate =
        tryGetString [ "default_output_template"; "defaultOutputTemplate"; "defaultYoutubeTemplate"; "default_youtube_template"] root
        |> Option.defaultValue defaults.defaultYoutubeTemplate
      defaultPodcastTemplate =
        tryGetString [ "default_podcast_template"; "defaultPodcastTemplate" ] root
        |> Option.defaultValue defaults.defaultPodcastTemplate
      targets = targets
      ytDlp = tryGetJson [ "yt_dlp"; "ytDlp" ] root }

let load () : Result<Config, string> =
    let path = configFile ()

    try
        if File.Exists path then
            let json = File.ReadAllText path
            parseConfig json |> Ok
        else
            Ok(defaultConfig ())
    with ex ->
        Error $"Failed to load config from '{path}': {ex.Message}"

let save (config: Config) : Result<unit, string> =
    let directory = configDirectory ()
    let path = configFile ()

    try
        Directory.CreateDirectory(directory) |> ignore
        let json = JsonSerializer.Serialize(config, serializerOptions)
        File.WriteAllText(path, json)
        Ok ()
    with ex ->
        Error $"Failed to save config to '{path}': {ex.Message}"
