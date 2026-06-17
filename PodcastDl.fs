module Archivist.PodcastDl

open System.IO
open System.Threading.Tasks
open Archivist.Domain
open Archivist.Paths
open Archivist.ProcessRunner

let executableName = "deno"

let private commandArgs = [ "x"; "podcast-dl" ]

let private defaultDirectoryTemplate (config: Config) =
    Path.Combine(config.podcastDir, "{{podcast_title}}")

let private episodeTemplate (config: Config) (target: Target) =
    match target.outputTemplate with
    | Some template when not (System.String.IsNullOrWhiteSpace template) -> template
    | _ -> config.defaultPodcastTemplate

let infoArgs (url: string) =
    commandArgs @ [ "--info"; "--url"; url ]

let probeLabel (url: string) : Task<Result<string, string>> =
    task {
        let! result = run executableName (infoArgs url)

        if result.exitCode <> 0 then
            return Error result.stderr
        else
            let firstLine =
                result.stdout.Replace("\r\n", "\n").Split('\n')
                |> Array.tryFind (fun line -> not (System.String.IsNullOrWhiteSpace line))

            match firstLine with
            | Some title -> return Ok(title.Trim())
            | None -> return Error "podcast-dl --info did not return a podcast title."
    }

let buildSyncArgs (config: Config) (_label: string) (target: Target) =
    commandArgs
    @ [ "--url"
        target.url
        "--out-dir"
        defaultDirectoryTemplate config
        "--threads"
        string 3
        "--episode-template"
        episodeTemplate config target
        "--archive"
        podcastArchiveTemplate config
        "--include-meta"
        "--include-episode-meta" ]

let sync (config: Config) (label: string) (target: Target) : Task<ProcessResult> =
    task {
        Directory.CreateDirectory(config.podcastDir) |> ignore
        let args = buildSyncArgs config label target
        return! run executableName args
    }
