use errors::*;
use std::fs::OpenOptions;
use std::io::prelude::*;

pub fn read_file(blockname: &str, path: &str) -> Result<String> {
    let mut f = OpenOptions::new()
        .read(true)
        .open(path)
        .block_error(blockname, &format!("failed to open file {}", path))?;
    let mut content = String::new();
    f.read_to_string(&mut content)
        .block_error(blockname, &format!("failed to read {}", path))?;
    // Removes trailing newline
    content.pop();
    Ok(content)
}

pub fn file_exists(path: &str) -> bool {
    ::std::path::Path::new(path).exists()
}
