use crate::types::ProcessResult;
use chrono::Local;
use std::fs;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command as ProcessCommand;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::thread;

pub fn run_process(executable: &str, args: &[String]) -> io::Result<ProcessResult> {
    let output = ProcessCommand::new(executable).args(args).output()?;
    Ok(ProcessResult {
        exit_code: output.status.code().unwrap_or(1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

pub fn run_process_with_log(
    executable: &str,
    args: &[String],
    log_path: &Path,
    command_line: &str,
) -> io::Result<ProcessResult> {
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = File::create(log_path)?;
    writeln!(file, "Timestamp: {}", Local::now().to_rfc3339())?;
    writeln!(file, "Command: {command_line}")?;
    writeln!(file)?;
    writeln!(
        file,
        "Output is streamed while the subprocess runs. Chunks are prefixed with their source stream."
    )?;
    writeln!(file)?;
    file.flush()?;

    let file = Arc::new(Mutex::new(file));
    let mut child = ProcessCommand::new(executable)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .map(|stdout| stream_process_output("STDOUT", stdout, Arc::clone(&file)));
    let stderr = child
        .stderr
        .take()
        .map(|stderr| stream_process_output("STDERR", stderr, Arc::clone(&file)));

    let status = child.wait()?;
    let stdout = join_stream_output(stdout)?;
    let stderr = join_stream_output(stderr)?;
    let exit_code = status.code().unwrap_or(1);

    {
        let mut file = lock_log_file(&file)?;
        writeln!(file)?;
        writeln!(file, "ExitCode: {exit_code}")?;
        file.flush()?;
    }

    Ok(ProcessResult {
        exit_code,
        stdout,
        stderr,
    })
}

fn stream_process_output<R>(
    label: &'static str,
    stream: R,
    file: Arc<Mutex<File>>,
) -> thread::JoinHandle<io::Result<String>>
where
    R: io::Read + Send + 'static,
{
    thread::spawn(move || {
        let mut collected = Vec::new();
        let mut reader = stream;
        let mut buffer = [0_u8; 8192];

        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }

            let chunk = &buffer[..bytes_read];
            collected.extend_from_slice(chunk);
            let mut file = lock_log_file(&file)?;
            write!(file, "[{label}] ")?;
            file.write_all(chunk)?;
            if !chunk.ends_with(b"\n") && !chunk.ends_with(b"\r") {
                writeln!(file)?;
            }
            file.flush()?;
        }

        Ok(String::from_utf8_lossy(&collected).into_owned())
    })
}

fn join_stream_output(
    handle: Option<thread::JoinHandle<io::Result<String>>>,
) -> io::Result<String> {
    match handle {
        Some(handle) => handle
            .join()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "stream reader thread panicked"))?,
        None => Ok(String::new()),
    }
}

fn lock_log_file(file: &Arc<Mutex<File>>) -> io::Result<std::sync::MutexGuard<'_, File>> {
    file.lock()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "log file lock was poisoned"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_process_with_log_streams_stdout_and_stderr_to_log() {
        let log_path = std::env::temp_dir().join(format!(
            "archivist-stream-test-{}-{}.log",
            std::process::id(),
            Local::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let args = vec![
            "-c".to_string(),
            "printf 'out1\n'; printf 'err1\n' >&2".to_string(),
        ];

        let result =
            run_process_with_log("sh", &args, &log_path, "sh -c test").expect("process should run");
        let log = fs::read_to_string(&log_path).expect("log should be readable");
        let _ = fs::remove_file(&log_path);

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "out1\n");
        assert_eq!(result.stderr, "err1\n");
        assert!(log.contains("Command: sh -c test"));
        assert!(log.contains("[STDOUT] out1"));
        assert!(log.contains("[STDERR] err1"));
        assert!(log.contains("ExitCode: 0"));
    }
}
