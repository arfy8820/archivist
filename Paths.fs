module YtArchive.Paths

open System
open System.IO
open YtArchive.Domain

let private nonEmptyOr fallback (value: string) =
    if String.IsNullOrWhiteSpace value then fallback else value

let appName = "yt-archive"

let userHomeDirectory () =
    Environment.GetFolderPath(Environment.SpecialFolder.UserProfile)
    |> nonEmptyOr "."

let defaultBaseDir () =
    Path.Combine(userHomeDirectory (), "Videos", "YouTube")

let configDirectory () =
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
