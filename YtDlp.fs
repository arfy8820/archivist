module YtArchive.YtDlp

open System
open System.IO
open System.Text.Json
open System.Threading.Tasks
open YtArchive.Domain
open YtArchive.Paths
open YtArchive.ProcessRunner

let executableName = "yt-dlp"

let private stringOptionOfNullable (value: string | null) =
    if isNull value || String.IsNullOrWhiteSpace value then None else Some value

let private tryGetString (name: string) (json: JsonElement) =
    match json.TryGetProperty name with
    | true, value when value.ValueKind = JsonValueKind.String ->
        value.GetString() |> stringOptionOfNullable
    | _ ->
        None

let probeArgs (url: string) = [ "--dump-json"; "--skip-download"; "--playlist-end"; string 1; url ]

let probe (url: string) : Task<ProbeOutcome> =
    task {
        let args = probeArgs url
        let! result = run executableName args

        if result.exitCode <> 0 then
            return ProbeFailed result.stderr
        else
            try
                use doc = JsonDocument.Parse(result.stdout)
                let root = doc.RootElement

                return
                    ProbeSuccess
                        { channel = tryGetString "channel" root
                          channelId = tryGetString "channel_id" root
                          channelHandle = tryGetString "channel_handle" root
                          uploader = tryGetString "uploader" root
                          uploaderId = tryGetString "uploader_id" root }
            with ex ->
                return ProbeFailed $"Failed to parse yt-dlp metadata: {ex.Message}"
    }

let buildSyncArgs (config: Config) (label: string) (entry: ArchiveEntry) =
    let archivePath = archiveFile config label

    [ "--download-archive"; archivePath; "--paths"; config.baseDir]
    @
    match entry.outputTemplate with
    | Some template -> [ "-o"; template ]
    | None -> []
    @ [ entry.url ]

let sync (config: Config) (label: string) (entry: ArchiveEntry) : Task<ProcessResult> =
    task {
        Directory.CreateDirectory(archiveDirectory config) |> ignore
        let args = buildSyncArgs config label entry
        return! run executableName args
    }
