use clap::Parser;
use shell_escape::escape;
use std::io::{self, BufRead, Write, Seek, SeekFrom};
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
    #[arg(short = 'n', default_value_t = 1)]
    batch_size: usize,

    /// Replacement string for batch mode
    #[arg(short = 'J')]
    batch_replstr: String,
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();

    let stdin = io::stdin();
    let lines: Vec<String> = stdin.lock().lines().collect::<Result<_, _>>()?;

    // Create a reusable pool of temp files
    let mut temp_pool: Vec<NamedTempFile> = (0..args.batch_size)
        .map(|_| NamedTempFile::new().expect("failed to create temp file"))
        .collect();

    for chunk in lines.chunks(args.batch_size) {
        let mut file_paths = Vec::new();

        // Reuse temp files from the pool
        for (i, line) in chunk.iter().enumerate() {
            let tmpfile = &mut temp_pool[i];
            let file = tmpfile.as_file_mut();
            file.set_len(0)?;
            file.seek(SeekFrom::Start(0))?;
            writeln!(file, "{}", line)?;
            file.flush()?;
            file_paths.push(tmpfile.path().to_path_buf());
        }

        // Build command: replace replstr with all paths (escaped)
        let files_str = file_paths
            .iter()
            .map(|p| escape(p.to_string_lossy()))
            .collect::<Vec<_>>()
            .join(" ");

        let shell_cmd = args.cmd.replace(&args.batch_replstr, &files_str);

        let output = Command::new("sh")
            .arg("-c")
            .arg(&shell_cmd)
            .output()
            .expect("failed to run shell command");

        if !output.status.success() {
            eprintln!("Command failed:\n{}", String::from_utf8_lossy(&output.stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            println!("{}", line);
        }
    }

    Ok(())
}
