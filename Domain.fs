module Archivist.Domain

type Tool =
    | YtDlp
    | PodcastDl

type Target =
    { name: string
      url: string
      tool: Tool
      outputTemplate: string option
      options: Map<string, string> option }

type Config =
    { youtubeDir: string
      podcastDir: string
      targets: list Target
      ytDlpOptions: Map<string, string>
      PodcastDlOptions: Map<string, string>
}

type ProcessResult =
    { exitCode: int
      stdout: string
      stderr: string }

type SyncTarget =
    | One of label: string
    | All

type AddRequest =
    { url: string option
      label: string option
      outputTemplate: string option }

type ResolvedAdd =
    { label: string
      target: Target }

type Command =
    | Add of AddRequest
    | Remove of label: string * removeArchive: bool
    | List
    | Sync of SyncTarget
    | Config of newBaseDir: string option
    | Usage of error: string option

type GlobalOptions =
    { verbose: bool }

type ParsedInput =
    { options: GlobalOptions
      command: Command }

type YoutubeInfo =
    { channel: string option
      channelId: string option
      channelHandle: string option
      uploader: string option
      uploaderId: string option }

type PodcastInfo =
    { title: string option
	  description: string option }


type ProbeOutcome =
    | YoutubeProbe of YoutubeInfo
	| podcastProbe of PodcastInfo
    | ProbeFailed of error: string
