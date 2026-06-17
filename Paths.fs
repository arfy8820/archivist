module Archivist.Paths

open System
open System.IO
open Archivist.Domain

let private nonEmptyOr fallback (value: string) =
    if String.IsNullOrWhiteSpace value then fallback else value

let appName = "archivist"

let private envVar (name: string) =
    Environment.GetEnvironmentVariable(name)
    |> fun value ->
        if String.IsNullOrWhiteSpace value then None
        else Some value

let userHomeDirectory () =
    Environment.GetFolderPath(Environment.SpecialFolder.UserProfile)
    |> nonEmptyOr "."

let defaultBaseDir () =
    Path.Combine(userHomeDirectory (), "Videos", "YouTube")

let defaultPodcastDir () =
    Path.Combine(userHomeDirectory (), "Music", "Podcasts")

let configDirectory () =
    match envVar "ARCHIVIST_CONFIG_DIR" with
    | Some directory -> directory
    | None ->
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData)
        |> nonEmptyOr (Path.Combine(userHomeDirectory (), ".config"))
        |> fun root -> Path.Combine(root, appName)

let configFile () =
    Path.Combine(configDirectory (), "config.json")

let logsDirectory () =
    Path.Combine(configDirectory (), "logs")

let archiveDirectory (config: Config) =
    Path.Combine(config.baseDir, "archive")

let archiveFile (config: Config) (label: string) =
    Path.Combine(archiveDirectory config, $"{label}.txt")

let podcastArchiveTemplate (config: Config) =
    Path.Combine(config.podcastDir, "{{podcast_title}}", "archive.json")
