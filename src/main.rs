use clap::Parser;
use std::process;
use std::fmt;
use std::process::Stdio;
use shell_escape::escape;
use std::io::{self, BufRead, BufReader, Write, Seek, SeekFrom};
use std::process::Command;
use tempfile::NamedTempFile;

/// xcopr: batch stream lines into temp files and run a shell command
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Shell command to run with all temp files substituted in place of replstr
    #[arg(short)]
    cmd: String,

    /// Number of lines to batch together per command invocation
    #[arg(short = 'n', long)]
    batch_size: Option<usize>,

    /// Replacement string for batch mode
    #[arg(short = 'J', long)]
    batch_replstr: Option<String>,

    /// Replacement string for streaming mode
    #[arg(short = 'I', long)]
    replstr: Option<String>,

}

#[derive(Debug)]
pub enum XcoprError {
    InvalidBatchMode,
    InvalidUtf8(std::io::Error),
    FailedToWrite(std::io::Error),
    SubprocessFailed(String),
    MissingArgs(&'static str),
}

impl fmt::Display for XcoprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use XcoprError::*;
        match self {
            InvalidBatchMode => write!(f, "Invalid batch mode: both -n and -J are required"),
            InvalidUtf8(e) => write!(f, "Input contains invalid UTF-8: {}", e),
            FailedToWrite(e) => write!(f, "Could not write to output stream: {}", e),
            SubprocessFailed(msg) => write!(f, "Subprocess failed: {}", msg),
            MissingArgs(arg) => write!(f, "Missing required argument: {}", arg),
        }
    }
}

pub type Result<T> = std::result::Result<T, XcoprError>;

fn run(args: Args) -> Result<()> {
    match (&args.batch_size, &args.batch_replstr) {
        (Some(n), Some(r)) => run_batch_mode(*n, r, &args.cmd),
        (None, None)       => Ok(()),//run_streaming_mode(args),
        _ => Err(XcoprError::MissingArgs("both -n and -J")),
    }

}

fn main() {
    let args = Args::parse();
    let result = run(args);

    match result {
        Ok(_) => {}
        Err(e) => {
            eprintln!("xcopr: {}", e);
            process::exit(1);
        }
    }

}

fn run_batch_mode(batch_size: usize, batch_replstr: &str, cmd: &str) -> Result<()> {
    let stdin = io::stdin();
    let lines: Vec<String> = stdin.lock().lines()
        .collect::<std::result::Result<_, _>>()
        .map_err(|e| XcoprError::InvalidUtf8(e))?;

    let mut temp_pool: Vec<NamedTempFile> = (0..batch_size)
        .map(|_| NamedTempFile::new().map_err(|e| XcoprError::FailedToWrite(e)))
        .collect::<Result<_>>()?;

    for chunk in lines.chunks(batch_size) {
        let mut file_paths = Vec::new();

        // Reuse temp files from the pool
        for (i, line) in chunk.iter().enumerate() {
            let tmpfile = &mut temp_pool[i];
            let file = tmpfile.as_file_mut();
            // TODO consolidate this
            file.set_len(0).map_err(|e| XcoprError::FailedToWrite(e))?;
            file.seek(SeekFrom::Start(0)).map_err(|e| XcoprError::FailedToWrite(e))?;
            writeln!(file, "{}", line).map_err(|e| XcoprError::FailedToWrite(e))?;
            file.flush().map_err(|e| XcoprError::FailedToWrite(e))?;
            file_paths.push(tmpfile.path().to_path_buf());
        }

        // Replace replstr with all paths (escaped)
        let files_str = file_paths
            .iter()
            .map(|p| escape(p.to_string_lossy()))
            .collect::<Vec<_>>()
            .join(" ");

        let shell_cmd = cmd.replace(&batch_replstr, &files_str);

        let mut child = Command::new("sh")
            .arg("-euo")
            .arg("pipefail")
            .arg("-c")
            .arg(&shell_cmd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| XcoprError::SubprocessFailed(e.to_string()))?;

        let stdout = child.stdout.take().ok_or_else(|| {
            XcoprError::SubprocessFailed("subprocess has no stdout".into())
        })?;

        let reader = BufReader::new(stdout);

        for (_, line) in chunk.iter().zip(reader.lines()) {
            let line = line.map_err(|e| XcoprError::InvalidUtf8(e))?;
            println!("{}", line);
        }

        let status = child.wait().map_err(|_| {
            XcoprError::SubprocessFailed("failed to wait for command".into())
        })?;

        if !status.success() {
            return Err(XcoprError::SubprocessFailed(format!(
                "command exited with code {}",
                status.code().unwrap_or(-1)
            )));
        }
    }
    Ok(())
}
