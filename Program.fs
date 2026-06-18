module Archivist.Program

open System
open System.IO
open System.Text.RegularExpressions
open System.Threading.Tasks
open Archivist.Domain
open Archivist.Paths
open Archivist.ConfigStore
open Archivist.YtDlp
open Archivist.PodcastDl
open Archivist.Cli

let private stringOptionOfNullable (value: string | null) =
    if isNull value then None else Some value

let private quoteArg (arg: string) =
    if arg.Contains(" ") || arg.Contains("\t") || arg.Contains("\"") then
        let escaped = arg.Replace("\"", "\\\"")
        $"\"{escaped}\""
    else
        arg

let private formatCommand fileName args =
    String.concat " " (fileName :: (args |> List.map quoteArg))

type private Logger =
    { verbose: bool }

let private logInfo logger message =
    if logger.verbose then
        printfn "%s" message

let private prompt (message: string) =
    Console.Write(message)
    Console.ReadLine()

let private promptRequired (message: string) =
    let rec loop () =
        let value = prompt message

        match stringOptionOfNullable value with
        | Some text when not (String.IsNullOrWhiteSpace text) -> text.Trim()
        | _ -> loop ()

    loop ()

let private promptOptional (message: string) =
    let value = prompt message

    match stringOptionOfNullable value with
    | Some text when not (String.IsNullOrWhiteSpace text) -> Some(text.Trim())
    | _ -> None

let private confirmYesDefault (message: string) =
    let value = prompt message

    match stringOptionOfNullable value with
    | None -> true
    | Some text when String.IsNullOrWhiteSpace text -> true
    | Some text ->
        text.Trim().Equals("y", StringComparison.OrdinalIgnoreCase)
        || text.Trim().Equals("yes", StringComparison.OrdinalIgnoreCase)

let private sanitizeLabel (label: string) =
    let invalid = Regex.Escape(String(Path.GetInvalidFileNameChars()))
    let invalidPattern = $"[{invalid}]"

    label.Trim()
        .Replace(" ", "-")
        |> fun value -> Regex.Replace(value, invalidPattern, "-")
        |> fun value -> Regex.Replace(value, "-+", "-")
        |> fun value -> value.Trim('-')

let private normalizeHandle (value: string option) =
    value
    |> Option.bind (fun raw ->
        let trimmed = raw.Trim()

        if String.IsNullOrWhiteSpace trimmed then
            None
        elif trimmed.StartsWith("@") then
            Some(sanitizeLabel trimmed)
        elif trimmed.StartsWith("UC", StringComparison.OrdinalIgnoreCase) then
            None
        elif trimmed.Contains(" ") then
            None
        else
            Some(sanitizeLabel ("@" + trimmed)))

let private suggestedLabel (probe: ProbeInfo) =
    [ probe.channelHandle |> normalizeHandle
      probe.uploaderId |> normalizeHandle
      probe.channel |> Option.map sanitizeLabel
      probe.uploader |> Option.map sanitizeLabel ]
    |> List.tryPick id

let private stableSuggestedLabel (probe: ProbeInfo) =
    [ probe.channelHandle |> normalizeHandle
      probe.uploaderId |> normalizeHandle ]
    |> List.tryPick id

let private ensureYoutubeDirectory (config: Config) =
    try
        Directory.CreateDirectory(config.youtubeDir) |> ignore
        Directory.CreateDirectory(youtubeArchiveDirectory config) |> ignore
        Ok ()
    with ex ->
        Error $"Failed to create directories under '{config.youtubeDir}': {ex.Message}"

let private ensureLogDirectory () =
    try
        Directory.CreateDirectory(logsDirectory ()) |> ignore
        Ok ()
    with ex ->
        Error $"Failed to create log directory '{logsDirectory ()}': {ex.Message}"

let private resolveUrl (request: AddRequest) =
    match request.url with
    | Some url when not (String.IsNullOrWhiteSpace url) -> url.Trim()
    | _ -> promptRequired "URL: "

let private resolveOutputTemplate (request: AddRequest) =
    match request.outputTemplate with
    | Some template when not (String.IsNullOrWhiteSpace template) -> Some(template.Trim())
    | _ -> promptOptional "Output template (optional): "

let private resolveSourceType (request: AddRequest) =
    match request.sourceType with
    | Some sourceType -> sourceType
    | None -> YouTube

let private resolveMode (request: AddRequest) =
    request.sourceType
    |> Option.map sourceTypeName
    |> Option.defaultValue "auto"

let private sourceTypeForAdd (url: string) (request: AddRequest) =
    match request.sourceType with
    | Some sourceType -> sourceType
    | None ->
        { name = ""
          url = url
          mode = "auto"
          subdir = None
          outputTemplate = None }
        |> targetSourceType

let private chooseLabelFromProbe (probe: ProbeInfo) =
    match stableSuggestedLabel probe with
    | Some stable ->
        let accepted = confirmYesDefault $"Use detected label '{stable}'? [Y/n]: "
        if accepted then stable else promptRequired "Label: " |> sanitizeLabel
    | None ->
        match suggestedLabel probe with
        | Some suggestion ->
            let entered = prompt $"Label [{suggestion}]: "
            match stringOptionOfNullable entered with
            | None -> suggestion
            | Some text when String.IsNullOrWhiteSpace text -> suggestion
            | Some text -> sanitizeLabel text
        | None ->
            promptRequired "Label: " |> sanitizeLabel

let private chooseLabelFromSuggestion (suggestion: string) =
    let sanitized = sanitizeLabel suggestion
    let entered = prompt $"Label [{sanitized}]: "

    match stringOptionOfNullable entered with
    | None -> sanitized
    | Some text when String.IsNullOrWhiteSpace text -> sanitized
    | Some text -> sanitizeLabel text

let private resolveLabel (logger: Logger) (url: string) (sourceType: SourceType) (request: AddRequest) : Task<Result<string, string>> =
    task {
        match request.label with
        | Some label when not (String.IsNullOrWhiteSpace label) ->
            return Ok(sanitizeLabel label)
        | _ ->
            match sourceType with
            | Podcast ->
                let args = PodcastDl.infoArgs url
                printfn "No label supplied. Probing podcast-dl for feed info..."
                logInfo logger $"Running: {formatCommand PodcastDl.executableName args}"
                let! probeResult = PodcastDl.probeLabel url

                match probeResult with
                | Ok title ->
                    let label = chooseLabelFromSuggestion title
                    return Ok label
                | Error error ->
                    eprintfn "Could not probe podcast label automatically."
                    if not (String.IsNullOrWhiteSpace error) then
                        eprintfn "%s" error
                    let label = promptRequired "Label: " |> sanitizeLabel
                    return Ok label
            | YouTube ->
                let args = YtDlp.probeArgs url
                printfn "No label supplied. Probing yt-dlp for metadata..."
                logInfo logger $"Running: {formatCommand YtDlp.executableName args}"
                let! probeResult = YtDlp.probe url

                match probeResult with
                | ProbeSuccess info ->
                    let label = chooseLabelFromProbe info
                    return Ok label
                | ProbeFailed error ->
                    eprintfn "Could not probe label automatically."
                    if not (String.IsNullOrWhiteSpace error) then
                        eprintfn "%s" error
                    let label = promptRequired "Label: " |> sanitizeLabel
                    return Ok label
    }

let private resolveAddRequest (logger: Logger) (request: AddRequest) : Task<Result<ResolvedAdd, string>> =
    task {
        let url = resolveUrl request
        let outputTemplate = resolveOutputTemplate request
        let sourceType = sourceTypeForAdd url request
        let! labelResult = resolveLabel logger url sourceType request

        match labelResult with
        | Error error ->
            return Error error
        | Ok label when String.IsNullOrWhiteSpace label ->
            return Error "Label cannot be empty."
        | Ok label ->
            return
                Ok
                    { label = label
                      target =
                        { name = label
                          url = url
                          mode = resolveMode request
                          subdir = Some label
                          outputTemplate = outputTemplate } }
    }

let private printTarget (target: Target) =
    printfn "%s" target.name
    printfn "  Type: %s" (target |> targetSourceType |> sourceTypeName)
    printfn "  Mode: %s" target.mode
    printfn "  URL: %s" target.url
    match target.subdir with
    | Some subdir -> printfn "  Subdir: %s" subdir
    | None -> ()
    match target.outputTemplate with
    | Some template -> printfn "  Output: %s" template
    | None -> printfn "  Output: default"

let private timestampForFileName () =
    DateTime.Now.ToString("yyyyMMdd-HHmmss")

let private syncLogFile (label: string) =
    Path.Combine(logsDirectory (), $"sync-{sanitizeLabel label}-{timestampForFileName ()}.log")

let private writeProcessLog (path: string) (commandLine: string) (result: ProcessResult) =
    let lines =
        [ $"Timestamp: {DateTime.Now:O}"
          $"Command: {commandLine}"
          $"ExitCode: {result.exitCode}"
          ""
          "STDOUT:"
          result.stdout
          ""
          "STDERR:"
          result.stderr ]

    File.WriteAllLines(path, lines)

let private handleAdd (logger: Logger) (config: Config) (request: AddRequest) : Task<int> =
    task {
        let! resolved = resolveAddRequest logger request

        match resolved with
        | Error error ->
            eprintfn "%s" error
            return 1
        | Ok add ->
            let updated =
                { config with
                    targets =
                        config.targets
                        |> List.filter (fun target -> target.name <> add.label)
                        |> fun targets -> targets @ [ add.target ] }

            match save updated with
            | Error error ->
                eprintfn "%s" error
                return 1
            | Ok () ->
                printfn "Added mapping '%s'." add.label
                return 0
    }

let private handleRemove (config: Config) (label: string) (removeArchive: bool) : int =
    let existingTarget = config.targets |> List.tryFind (fun target -> target.name = label)
    let exists = existingTarget |> Option.isSome

    let updated =
        { config with
            targets = config.targets |> List.filter (fun target -> target.name <> label) }

    let configResult = save updated

    match configResult with
    | Error error ->
        eprintfn "%s" error
        1
    | Ok () ->
        if exists then
            printfn "Removed mapping '%s'." label
        else
            printfn "No mapping found for '%s'. Config unchanged." label

        if removeArchive then
            let path =
                existingTarget
                |> Option.map targetSourceType
                |> Option.defaultValue YouTube
                |> function
                    | YouTube -> youtubeArchiveFile config label
                    | Podcast -> podcastArchiveTemplate config
            try
                if File.Exists path then
                    File.Delete path
                    printfn "Removed archive file '%s'." path
                    0
                else
                    printfn "Archive file '%s' did not exist." path
                    1
            with ex ->
                eprintfn "Failed to remove archive file '%s': %s" path ex.Message
                1
        else
            0

let private handleList (config: Config) : int =
    if List.isEmpty config.targets then
        printfn "No archive mappings configured."
    else
        config.targets |> List.iter printTarget
    0

let private handleConfig (config: Config) (newYoutubeDir: string option) : int =
    match newYoutubeDir with
    | None ->
        printfn "%s" config.youtubeDir
        0
    | Some youtubeDir ->
        let updated = { config with youtubeDir = youtubeDir }

        match ensureYoutubeDirectory updated with
        | Error error ->
            eprintfn "%s" error
            1
        | Ok () ->
            match save updated with
            | Error error ->
                eprintfn "%s" error
                1
            | Ok () ->
                printfn "Youtube directory updated to '%s'." youtubeDir
                0

let private getSyncEntries (config: Config) (target: SyncTarget) =
    match target with
    | One label ->
        match config.targets |> List.tryFind (fun target -> target.name = label) with
        | Some target -> Ok [ target ]
        | None -> Error $"No entry found for label '{label}'."
    | All ->
        Ok config.targets

let private printSyncResult (label: string) (result: ProcessResult) =
    if result.exitCode = 0 then
        printfn "Sync succeeded for '%s'." label
    else
        eprintfn "Warning: Sync  for '%s' returned with exit code %d." label result.exitCode

    if result.exitCode <> 0 && not (String.IsNullOrWhiteSpace result.stderr) then
        eprintfn "%s" result.stderr

let private handleSync (logger: Logger) (config: Config) (target: SyncTarget) : Task<int> =
    task {
        match ensureYoutubeDirectory config with
        | Error error ->
            eprintfn "%s" error
            return 1
        | Ok () ->
            match ensureLogDirectory () with
            | Error error ->
                eprintfn "%s" error
                return 1
            | Ok () ->
                match getSyncEntries config target with
                | Error error ->
                    eprintfn "%s" error
                    return 1
                | Ok [] ->
                    printfn "No archive mappings configured."
                    return 0
                | Ok entries ->
                    let mutable finalExitCode = 0

                    for target in entries do
                        let label = target.name
                        let sourceType = targetSourceType target
                        let executableName, args, sync =
                            match sourceType with
                            | YouTube ->
                                YtDlp.executableName, YtDlp.buildSyncArgs config label target, YtDlp.sync config label target
                            | Podcast ->
                                PodcastDl.executableName, PodcastDl.buildSyncArgs config label target, PodcastDl.sync config label target

                        let commandLine = formatCommand executableName args
                        printfn "Syncing '%s'..." label
                        logInfo logger $"Running: {commandLine}"
                        let! result = sync
                        let logPath = syncLogFile label
                        writeProcessLog logPath commandLine result
                        logInfo logger $"Wrote log to {logPath}"
                        printSyncResult label result
                        if result.exitCode <> 0 then
                            finalExitCode <- result.exitCode

                    return finalExitCode
    }

let private runCommand (logger: Logger) (config: Config) (command: Command) : Task<int> =
    task {
        match command with
        | Add request ->
            return! handleAdd logger config request
        | Remove(label, removeArchive) ->
            return handleRemove config label removeArchive
        | List ->
            return handleList config
        | Sync target ->
            return! handleSync logger config target
        | Config newYoutubeDir ->
            return handleConfig config newYoutubeDir
        | Usage error ->
            printUsage error
            return 2
    }

let private runMain (argv: string array) : Task<int> =
    task {
        let parsed = parseArgs argv
        let logger = { verbose = parsed.options.verbose }

        match parsed.command with
        | Usage error ->
            printUsage error
            return 2
        | _ ->
            let configPath = configFile ()
            logInfo logger $"Loading config from {configPath}"
            match load () with
            | Error error ->
                eprintfn "%s" error
                return 1
            | Ok config ->
                return! runCommand logger config parsed.command
    }

[<EntryPoint>]
let main argv =
    runMain(argv)
        .GetAwaiter()
        .GetResult()
