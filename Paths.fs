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

let defaultYoutubeDir () =
    Path.Combine(userHomeDirectory (), "Videos", "YouTube")

let defaultPodcastDir () =
    Path.Combine(userHomeDirectory (), "Music", "Podcasts")

let configDirectory () =
    match envVar "ARCHIVIST_CONFIG_DIR" with
    | Some directory -> directory
    | None ->
        Path.Combine(userHomeDirectory (), ".config")
        |> fun root -> Path.Combine(root, appName)

let configFile () =
    Path.Combine(configDirectory (), "config.json")

let logsDirectory () =
    Path.Combine(configDirectory (), "logs")

let youtubeArchiveDirectory (config: Config) =
    config.youtubeDir

let youtubeArchiveFile (config: Config) (label: string) =
    Path.Combine(youtubeArchiveDirectory config, label, ".download-archive.txt")

let podcastArchiveTemplate (config: Config) =
    Path.Combine(config.podcastDir, "{{podcast_title}}", "archive.json")
