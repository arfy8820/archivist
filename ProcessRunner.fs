module YtArchive.ProcessRunner

open System.Diagnostics
open System.Threading.Tasks
open YtArchive.Domain

let run (fileName: string) (args: string list) : Task<ProcessResult> =
    task {
        let startInfo = ProcessStartInfo()
        startInfo.FileName <- fileName
        startInfo.UseShellExecute <- false
        startInfo.RedirectStandardOutput <- true
        startInfo.RedirectStandardError <- true
        startInfo.CreateNoWindow <- true

        for arg in args do
            startInfo.ArgumentList.Add(arg)

        use proc = new Process()
        proc.StartInfo <- startInfo

        let started = proc.Start()

        if not started then
            return
                { exitCode = -1
                  stdout = ""
                  stderr = $"Failed to start process '{fileName}'." }
        else
            let stdoutTask = proc.StandardOutput.ReadToEndAsync()
            let stderrTask = proc.StandardError.ReadToEndAsync()

            do! proc.WaitForExitAsync()
            let! stdout = stdoutTask
            let! stderr = stderrTask

            return
                { exitCode = proc.ExitCode
                  stdout = stdout
                  stderr = stderr }
    }
