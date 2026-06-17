module YtArchive.ConfigStore

open System.IO
open System.Text.Json
open YtArchive.Domain
open YtArchive.Paths

let private serializerOptions =
    let o = JsonSerializerOptions(WriteIndented = true)
    o.PropertyNamingPolicy <- null
    o

let defaultConfig () : Config =
    { baseDir = defaultBaseDir ()
      entries = Map.empty }

let load () : Result<Config, string> =
    let path = configFile ()

    try
        if File.Exists path then
            let json = File.ReadAllText path
            JsonSerializer.Deserialize<Config>(json, serializerOptions) |> Ok
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
