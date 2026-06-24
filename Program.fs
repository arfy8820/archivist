module Archivist.Program

open System
open System.IO
open System.Text.RegularExpressions
open System.Text.Json
open System.Threading.Tasks
open Archivist.Domain
open Archivist.Paths
open Archivist.ConfigStore
open Archivist.YtDlp
open Archivist.PodcastDl

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
    { quiet: bool }

let private logInfo logger message =
    if not logger.quiet then
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

let private confirmNoDefault (message: string) =
    let value = prompt message

    match stringOptionOfNullable value with
    | None -> false
    | Some text when String.IsNullOrWhiteSpace text -> false
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
          urls = None
          mode = "auto"
          subdir = None
          outputTemplate = None }
        |> targetSourceType

let private targetUrlsForAdd (url: string) (sourceType: SourceType) =
    let playlistSuffix = "/playlists"
    let trimmed = url.TrimEnd('/')

    if sourceType = YouTube && trimmed.EndsWith(playlistSuffix, StringComparison.OrdinalIgnoreCase) then
        let baseUrl = trimmed.Substring(0, trimmed.Length - playlistSuffix.Length)
        let accepted = confirmNoDefault $"Also download '{baseUrl}' to capture videos not in a playlist? [y/N]: "

        if accepted then
            Some [ url; baseUrl ]
        else
            None
    else
        None

let private resolveSubdir (label: string) (request: AddRequest) =
    match request.subdir with
    | Some true -> Some label
    | Some false -> None
    | None ->
        if confirmNoDefault "Store target in subdirectory? [y/N]: " then
            Some label
        else
            None

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
            let urls = targetUrlsForAdd url sourceType
            let subdir = resolveSubdir label request
            return
                Ok
                    { label = label
                      target =
                        { name = label
                          url = url
                          urls = urls
                          mode = resolveMode request
                          subdir = subdir
                          outputTemplate = outputTemplate } }
    }

let private printTarget (target: Target) =
    printfn "%s" target.name
    printfn "  Type: %s" (target |> targetSourceType |> sourceTypeName)
    printfn "  Mode: %s" target.mode
    printfn "  URL: %s" target.url
    match target.urls with
    | Some urls when urls.Length > 1 ->
        urls |> List.iter (printfn "  Sync URL: %s")
    | _ -> ()
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

let private handleAdd (logger: Logger) (configPath: string) (config: Config) (request: AddRequest) : Task<int> =
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

            match saveTo configPath updated with
            | Error error ->
                eprintfn "%s" error
                return 1
            | Ok () ->
                printfn "Added mapping '%s'." add.label
                return 0
    }

let private handleRemove (configPath: string) (config: Config) (label: string) (removeArchive: bool) : int =
    let existingTarget = config.targets |> List.tryFind (fun target -> target.name = label)
    let exists = existingTarget |> Option.isSome

    let updated =
        { config with
            targets = config.targets |> List.filter (fun target -> target.name <> label) }

    let configResult = saveTo configPath updated

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

let private printJson value =
    let options = JsonSerializerOptions(WriteIndented = true)
    printfn "%s" (JsonSerializer.Serialize(value, options))

let private handleList (options: GlobalOptions) (config: Config) : int =
    if options.emitJson then
        printJson config.targets
    elif List.isEmpty config.targets then
        printfn "No archive mappings configured."
    else
        config.targets |> List.iter printTarget
    0

let private showConfigProperty (options: GlobalOptions) (config: Config) (property: ConfigProperty) =
    if options.emitJson then
        match property with
        | AllProperties -> printJson config
        | YoutubeDir ->  printJson {| youtube_dir = config.youtubeDir |}
        | PodcastDir -> printJson {| podcast_dir = config.podcastDir |}
        | DefaultYoutubeTemplate -> printJson {| default_youtube_template = config.defaultYoutubeTemplate |}
        | DefaultPodcastTemplate -> printJson {| default_podcast_template = config.defaultPodcastTemplate |}
        | Targets -> printJson config.targets
        | YtDlpOptions ->
            match config.ytDlpOptions with
            | Some value -> printfn "%s" (value.GetRawText())
            | None -> printfn "null"
        | PodcastDlOptions ->
            match config.podcastDlOptions with
            | Some value -> printfn "%s" (value.GetRawText())
            | None -> printfn "null"

    else
        match property with
        | AllProperties ->
            printfn "youtube_dir: %s" config.youtubeDir
            printfn "podcast_dir: %s" config.podcastDir
            printfn "default_youtube_template: %s" config.defaultYoutubeTemplate
            printfn "default_podcast_template: %s" config.defaultPodcastTemplate
            printfn "targets: %d" config.targets.Length
            printfn "yt_dlp_options: %s" (if config.ytDlpOptions.IsSome then "configured" else "unset")
            printfn "podcast_dl_options: %s" (if config.podcastDlOptions.IsSome then "configured" else "unset")

        | YoutubeDir -> printfn "%s" config.youtubeDir
        | PodcastDir -> printfn "%s" config.podcastDir
        | DefaultYoutubeTemplate -> printfn "%s" config.defaultYoutubeTemplate
        | DefaultPodcastTemplate -> printfn "%s" config.defaultPodcastTemplate
        | Targets -> printJson config.targets
        | YtDlpOptions ->
            match config.ytDlpOptions with
            | Some value -> printfn "%s" (value.GetRawText())
            | None -> printfn "null"

        | PodcastDlOptions ->
            match config.podcastDlOptions with
            | Some value -> printfn "%s" (value.GetRawText())
            | None -> printfn "null"

let private parseJsonElement (value: string) =
    try
        use doc = JsonDocument.Parse(value)
        Ok(doc.RootElement.Clone())
    with ex ->
        Error ex.Message

let private setConfigProperty (config: Config) (property: ConfigProperty) (value: string option) =
    match property with
    | ConfigProperty.AllProperties -> Error "Cannot set all config properties at once."
    | ConfigProperty.YoutubeDir ->
        match value with
        | Some text when not (String.IsNullOrWhiteSpace text) -> Ok { config with youtubeDir = text }
        | _ -> Error "youtube_dir requires a value."
    | ConfigProperty.PodcastDir ->
        match value with
        | Some text when not (String.IsNullOrWhiteSpace text) -> Ok { config with podcastDir = text }
        | _ -> Error "podcast_dir requires a value."
    | ConfigProperty.DefaultYoutubeTemplate ->
        Ok { config with defaultYoutubeTemplate = value |> Option.defaultValue "" }
    | ConfigProperty.DefaultPodcastTemplate ->
        Ok { config with defaultPodcastTemplate = value |> Option.defaultValue "" }
    | ConfigProperty.Targets ->
        match value with
        | None
        | Some "" -> Ok { config with targets = [] }
        | Some text ->
            try
                let targets = JsonSerializer.Deserialize<Target list>(text)
                Ok { config with targets = targets }
            with ex ->
                Error $"targets must be a JSON array: {ex.Message}"
    | ConfigProperty.YtDlpOptions ->
        match value with
        | None
        | Some "" -> Ok { config with ytDlpOptions = None }
        | Some text ->
            parseJsonElement text
            |> Result.map (fun json -> { config with ytDlpOptions = Some json })

    | ConfigProperty.PodcastDlOptions ->
        match value with
        | None
        | Some "" -> Ok { config with podcastDlOptions = None }
        | Some text ->
            parseJsonElement text
            |> Result.map (fun json -> { config with podcastDlOptions = Some json })


let private handleConfig (options: GlobalOptions) (configPath: string) (config: Config) (action: ConfigAction) : int =
    match action with
    | ConfigAction.Show property ->
        showConfigProperty options config property
        0
    | ConfigAction.Set(property, value) ->
        match setConfigProperty config property value with
        | Error error ->
            eprintfn "%s" error
            1
        | Ok updated ->
            match saveTo configPath updated with
            | Error error ->
                eprintfn "%s" error
                1
            | Ok () ->
                printfn "Updated config."
                0

let private handleProbe (options: GlobalOptions) (config: Config) (name: string) =
    match config.targets |> List.tryFind (fun target -> target.name = name) with
    | None ->
        eprintfn "No entry found for label '%s'." name
        1
    | Some target ->
        let mode = target |> targetSourceType |> sourceTypeName
        if options.emitJson then
            printJson {| name = target.name; mode = mode; url = target.url |}
        else
            printfn "%s" mode
        0

let private getSyncEntries (config: Config) (target: SyncTarget) =
    match target with
    | SyncTarget.One label ->
        match config.targets |> List.tryFind (fun target -> target.name = label) with
        | Some target -> Ok [ target ]
        | None -> Error $"No entry found for label '{label}'."
    | SyncTarget.All ->
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

let private runCommand (logger: Logger) (options: GlobalOptions) (configPath: string) (config: Config) (command: Command) : Task<int> =
    task {
        match command with
        | Command.Add request ->
            return! handleAdd logger configPath config request
        | Command.Remove(label, removeArchive) ->
            return handleRemove configPath config label removeArchive
        | Command.List ->
            return handleList options config
        | Command.Sync target ->
            return! handleSync logger config target
        | Command.Config action ->
            return handleConfig options configPath config action
        | Command.Probe name ->
            return handleProbe options config name
        | Command.Version ->
            if options.emitJson then
                printJson {| version = Archivist.Cli.version |}
            else
                printfn "archivist version %s" Archivist.Cli.version
            return 0
        | Command.Usage error ->
            Archivist.Cli.printUsage error
            return 2
    }

let private runMain (argv: string array) : Task<int> =
    task {
        let parsed = Archivist.Cli.parseArgs argv
        let logger = { quiet = parsed.options.quiet }

        match parsed.command with
        | Command.Usage error ->
            Archivist.Cli.printUsage error
            return 2
        | Command.Version ->
            if parsed.options.emitJson then
                printJson {| version = Archivist.Cli.version |}
            else
                printfn "archivist version %s" Archivist.Cli.version
            return 0
        | _ ->
            let configPath = parsed.options.configPath |> Option.defaultValue (configFile ())
            logInfo logger $"Loading config from {configPath}"
            match loadFrom configPath with
            | Error error ->
                eprintfn "%s" error
                return 1
            | Ok config ->
                return! runCommand logger parsed.options configPath config parsed.command
    }

[<EntryPoint>]
let main argv =
    runMain(argv)
        .GetAwaiter()
        .GetResult()
