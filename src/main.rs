use std::io::{self, BufRead, Write, Seek, SeekFrom};
use std::process::Command;
use tempfile::NamedTempFile;
use std::path::PathBuf;

fn write_to_tempfile(content: &str) -> std::io::Result<(PathBuf, NamedTempFile)> {
    let mut tmp = NamedTempFile::new()?;
    tmp.write_all(content.as_bytes())?;
    tmp.seek(SeekFrom::Start(0))?;
    Ok((tmp.path().to_path_buf(), tmp))
}

fn main() -> std::io::Result<()> {
    let stdin = io::stdin();
    let lines: Vec<String> = stdin.lock().lines().collect::<Result<_, _>>()?;

    let mut file_paths = Vec::new();
    let mut handles = Vec::new();

    for line in &lines {
        let line_with_newline = format!("{line}\n");
        let (path, tmpfile) = write_to_tempfile(&line_with_newline)?;
        file_paths.push(path);
        handles.push(tmpfile); // keep alive
    }

    let output = Command::new("md5sum")
        .args(&file_paths)
        .output()
        .expect("failed to run md5sum");

    let stdout = String::from_utf8_lossy(&output.stdout);
    for (input, line) in lines.iter().zip(stdout.lines()) {
        let hash = line.split_whitespace().next().unwrap_or("<missing>");
        println!("{:<20} => {}", input.trim_end(), hash);
    }

    Ok(())
}
