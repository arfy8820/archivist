module Archivist.Cli

open Archivist.Domain

let private tryGetOption (name: string) (args: string list) =
    let rec loop remaining =
        match remaining with
        | optionName :: value :: _ when optionName = name ->
            Ok(Some value)
        | optionName :: [] when optionName = name ->
            Error $"Missing value for {name}"
        | _ :: tail ->
            loop tail
        | [] ->
            Ok None

    loop args

let private validateKnownOptions (knownOptions: string list) (args: string list) =
    let rec loop remaining =
        match remaining with
        | [] ->
            Ok ()
        | (optionName: string) :: _value :: tail when optionName.StartsWith("--") ->
            if knownOptions |> List.contains optionName then
                loop tail
            else
                Error $"Unknown option: {optionName}"
        | [ optionName ] when optionName.StartsWith("--") ->
            if knownOptions |> List.contains optionName then
                Error $"Missing value for {optionName}"
            else
                Error $"Unknown option: {optionName}"
        | token :: _ ->
            Error $"Unexpected argument: {token}"

    loop args

let private parseAdd (args: string list) : Command =
    match validateKnownOptions [ "--url"; "--label"; "--output"; "--type" ] args with
    | Error message -> Usage(Some message)
    | Ok () ->
        match tryGetOption "--url" args, tryGetOption "--label" args, tryGetOption "--output" args, tryGetOption "--type" args with
        | Error message, _, _, _
        | _, Error message, _, _
        | _, _, Error message, _
        | _, _, _, Error message ->
            Usage(Some message)
        | Ok url, Ok label, Ok outputTemplate, Ok sourceType ->
            match sourceType with
            | Some value when value.Trim().ToLowerInvariant() = "auto" ->
                Add
                    { url = url
                      label = label
                      outputTemplate = outputTemplate
                      sourceType = None }
            | _ ->
                match sourceType |> Option.map tryParseSourceType with
                | Some None -> Usage(Some "Unknown target type. Use 'auto', 'youtube', or 'podcast'.")
                | parsedSourceType ->
                    Add
                        { url = url
                          label = label
                          outputTemplate = outputTemplate
                          sourceType = parsedSourceType |> Option.flatten }

let private parseRemove (args: string list) : Command =
    match args with
    | [] ->
        Usage(Some "Usage: archivist remove <label> [--remove-archive]")
    | label :: rest ->
        let removeArchive = rest |> List.contains "--remove-archive"
        let unknown = rest |> List.filter (fun arg -> arg <> "--remove-archive")

        match unknown with
        | [] -> Remove(label, removeArchive)
        | _ -> Usage(Some "Usage: archivist remove <label> [--remove-archive]")

let private parseList (args: string list) : Command =
    match args with
    | [] -> List
    | _ -> Usage(Some "The 'list' command takes no arguments.")

let private parseSync (args: string list) : Command =
    match args with
    | [] -> Sync All
    | [ label ] -> Sync(One label)
    | _ -> Usage(Some "Usage: archivist sync [label]")

let private parseConfig (args: string list) : Command =
    match args with
    | [] -> Config None
    | [ baseDir ] -> Config(Some baseDir)
    | _ -> Usage(Some "Usage: archivist config [baseDir]")

let private parseCommand (commandName: string) (args: string list) : Command =
    match commandName with
    | "add" -> parseAdd args
    | "remove" -> parseRemove args
    | "list" -> parseList args
    | "sync" -> parseSync args
    | "config" -> parseConfig args
    | unknown -> Usage(Some $"Unknown command: {unknown}")

let parseArgs (argv: string array) : ParsedInput =
    let args = argv |> Array.toList
    let verbose = args |> List.contains "--verbose"
    let remaining = args |> List.filter ((<>) "--verbose")

    let command =
        match remaining with
        | [] -> Usage None
        | commandName :: commandArgs -> parseCommand commandName commandArgs

    { options = { verbose = verbose }
      command = command }

let printUsage (error: string option) =
    match error with
    | Some message -> eprintfn "Error: %s" message
    | None -> ()

    printfn "archivist - manage named media archive targets"
    printfn ""
    printfn "Usage:"
    printfn "  archivist [--verbose] add"
    printfn "  archivist [--verbose] add --url <url> [--label <label>] [--type auto|youtube|podcast] [--output <output>]"
    printfn "  archivist [--verbose] remove <label> [--remove-archive]"
    printfn "  archivist [--verbose] list"
    printfn "  archivist [--verbose] sync [label]"
    printfn "  archivist [--verbose] config [<baseDir>]"
