module Archivist.Cli

open System
open Argu
open Archivist.Domain

let version = "0.1.0"

let private parseConfigProperty (value: string option) =
    match value |> Option.map (fun text -> text.Trim().ToLowerInvariant()) with
    | None
    | Some "" -> Ok AllProperties
    | Some "base_dir"
    | Some "base-dir"
    | Some "youtube_dir"
    | Some "youtube-dir" -> Ok BaseDir
    | Some "podcast_dir"
    | Some "podcast-dir" -> Ok PodcastDir
    | Some "default_output_template"
    | Some "default-output-template"
    | Some "default_youtube_template"
    | Some "default-youtube-template" -> Ok DefaultOutputTemplate
    | Some "podcast_template"
    | Some "podcast-template"
    | Some "default_podcast_template"
    | Some "default-podcast-template" -> Ok DefaultPodcastTemplate
    | Some "targets" -> Ok Targets
    | Some "yt_dlp"
    | Some "yt-dlp" -> Ok YtDlp
    | Some unknown -> Error $"Unknown config property: {unknown}"

type AddArgs =
    | Url of string
    | Label of string
    | Output of string
    | Type of string

    interface IArgParserTemplate with
        member arg.Usage =
            match arg with
            | Url _ -> "YouTube channel, playlist, SoundCloud, or podcast URL."
            | Label _ -> "Label used as the target name and default subdirectory."
            | Output _ -> "Optional per-target output template override."
            | Type _ -> "Target type: auto, youtube, or podcast."

type RemoveArgs =
    | [<MainCommand>] Name of string
    | Delete_Archive

    interface IArgParserTemplate with
        member arg.Usage =
            match arg with
            | Name _ -> "Target name to remove."
            | Delete_Archive -> "Also delete the target archive file if it exists."

type SyncArgs =
    | [<MainCommand>] Name of string
    | All

    interface IArgParserTemplate with
        member arg.Usage =
            match arg with
            | Name _ -> "Optional target name. Omit to sync all targets."
            | All -> "Sync all configured targets."

type ProbeArgs =
    | [<MainCommand>] Name of string

    interface IArgParserTemplate with
        member arg.Usage =
            match arg with
            | Name _ -> "Target name to probe."

type ConfigShowArgs =
    | [<MainCommand>] Property of string

    interface IArgParserTemplate with
        member arg.Usage =
            match arg with
            | Property _ -> "Optional config property to show."

type ConfigSetArgs =
    | [<MainCommand>] Arguments of string list

    interface IArgParserTemplate with
        member arg.Usage =
            match arg with
            | Arguments _ -> "Configuration property and optional value."

type ConfigArgs =
    | [<CliPrefix(CliPrefix.None)>] Show of ParseResults<ConfigShowArgs>
    | [<CliPrefix(CliPrefix.None)>] Set of ParseResults<ConfigSetArgs>

    interface IArgParserTemplate with
        member arg.Usage =
            match arg with
            | Show _ -> "Display global properties from the configuration."
            | Set _ -> "Set or clear a configuration option."

type CliArgs =
    | [<AltCommandLine("-c")>] Config_File of string
    | [<AltCommandLine("-j")>] Json
    | Quiet
    | [<AltCommandLine("-v")>] Version
    | [<CliPrefix(CliPrefix.None)>] List
    | [<CliPrefix(CliPrefix.None)>] Config of ParseResults<ConfigArgs>
    | [<CliPrefix(CliPrefix.None)>] Probe of ParseResults<ProbeArgs>
    | [<CliPrefix(CliPrefix.None)>] Sync of ParseResults<SyncArgs>
    | [<CliPrefix(CliPrefix.None)>] Add of ParseResults<AddArgs>
    | [<CliPrefix(CliPrefix.None)>] Remove of ParseResults<RemoveArgs>

    interface IArgParserTemplate with
        member arg.Usage =
            match arg with
            | Config_File _ -> "Path to config file. Defaults to the platform config directory."
            | Json -> "Emit JSON output."
            | Quiet -> "Reduce human-readable output where possible."
            | Version -> $"Emit version information."
            | List -> "List configured targets."
            | Config _ -> "Show or set global configuration options."
            | Probe _ -> "Probe a target and report detected mode."
            | Sync _ -> "Sync one target or all targets. Defaults to all targets."
            | Add _ -> "Add a target to the config."
            | Remove _ -> "Remove a target from the config."

let private parser =
    ArgumentParser.Create<CliArgs>(programName = "archivist")

let private typeFromText (value: string option) =
    match value with
    | None -> Ok None
    | Some text when text.Trim().Equals("auto", StringComparison.OrdinalIgnoreCase) -> Ok None
    | Some text ->
        match tryParseSourceType text with
        | Some sourceType -> Ok(Some sourceType)
        | None -> Error "Unknown target type. Use 'auto', 'youtube', or 'podcast'."

let private commandFromConfig (args: ParseResults<ConfigArgs>) =
    match args.GetSubCommand() with
    | Show showArgs ->
        showArgs.TryGetResult Property
        |> parseConfigProperty
        |> Result.map (fun property -> Command.Config(ConfigAction.Show property))
    | Set setArgs ->
        match setArgs.TryGetResult Arguments with
        | None
        | Some [] -> Error "Usage: archivist config set <property> [value]"
        | Some (propertyName :: valueParts) ->
            match parseConfigProperty (Some propertyName) with
            | Error error -> Error error
            | Ok AllProperties -> Error "The 'set' action requires a specific property."
            | Ok property ->
                let value =
                    match valueParts with
                    | [] -> None
                    | parts -> Some(String.concat " " parts)

                Ok(Command.Config(ConfigAction.Set(property, value)))

let private commandFromParsed (results: ParseResults<CliArgs>) =
    try
        match results.GetSubCommand() with
        | List -> Ok Command.List
        | Config configArgs -> commandFromConfig configArgs
        | Probe probeArgs ->
            match probeArgs.TryGetResult Name with
            | Some name -> Ok(Command.Probe name)
            | None -> Error "Usage: archivist probe <name>"
        | Sync syncArgs ->
            match syncArgs.Contains All, syncArgs.TryGetResult Name with
            | true, Some _ -> Error "Use either 'sync --all' or 'sync <name>', not both."
            | true, None -> Ok(Command.Sync SyncTarget.All)
            | false, Some name -> Ok(Command.Sync(SyncTarget.One name))
            | false, None -> Ok(Command.Sync SyncTarget.All)
        | Add addArgs ->
            match typeFromText (addArgs.TryGetResult Type) with
            | Error error -> Error error
            | Ok sourceType ->
                Ok(
                    Command.Add
                        { url = addArgs.TryGetResult Url
                          label = addArgs.TryGetResult Label
                          outputTemplate = addArgs.TryGetResult Output
                          sourceType = sourceType }
                )
        | Remove removeArgs ->
            match removeArgs.TryGetResult Name with
            | Some name -> Ok(Command.Remove(name, removeArgs.Contains Delete_Archive))
            | None -> Error "Usage: archivist remove <name> [--delete-archive]"
    with _ ->
        Error(parser.PrintUsage())

let parseArgs (argv: string array) : ParsedInput =
    try
        let results = parser.ParseCommandLine(inputs = argv, raiseOnUsage = true)

        let options =
            { configPath = results.TryGetResult Config_File
              emitJson = results.Contains Json
              quiet = results.Contains Quiet }

        let command =
            if results.Contains Version then
                Command.Version
            else
                match commandFromParsed results with
                | Ok command -> command
                | Error error -> Command.Usage(Some error)

        { options = options
          command = command }
    with
    | :? ArguParseException as ex ->
        { options =
            { configPath = None
              emitJson = false
              quiet = false }
          command = Command.Usage(Some ex.Message) }

let printUsage (error: string option) =
    match error with
    | Some message -> eprintfn "%s" message
    | None -> ()

    printfn "%s" (parser.PrintUsage())
