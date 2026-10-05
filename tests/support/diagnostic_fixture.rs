//! Shared test-only fixture encoder. Never included by a product target.
//! A diagnostic report is model data, not live evidence or an operation token.
use std::fs::{File, OpenOptions};
use std::io::Write;

pub fn output() -> Result<File, Box<dyn std::error::Error>> {
    let path = std::env::var("DUSTROUTE_DIAGNOSTIC_OUTPUT")?;
    new_output(&path)
}

pub fn new_output(path: &str) -> Result<File, Box<dyn std::error::Error>> {
    if !std::path::Path::new(&path).is_absolute() {
        return Err("DUSTROUTE_DIAGNOSTIC_OUTPUT must be an absolute new fixture path".into());
    }
    Ok(OpenOptions::new().write(true).create_new(true).open(path)?)
}

pub fn input<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, Box<dyn std::error::Error>> {
    if !std::path::Path::new(path).is_absolute() {
        return Err("fixture input must be an absolute path".into());
    }
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

pub fn row(
    file: &mut File,
    value: &impl serde::Serialize,
) -> Result<(), Box<dyn std::error::Error>> {
    serde_json::to_writer(&mut *file, value)?;
    file.write_all(b"\n")?;
    Ok(())
}

pub fn report(
    file: &mut File,
    value: &impl serde::Serialize,
) -> Result<(), Box<dyn std::error::Error>> {
    serde_json::to_writer_pretty(&mut *file, value)?;
    file.write_all(b"\n")?;
    Ok(())
}
