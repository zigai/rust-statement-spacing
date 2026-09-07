//! Capture the compiler ABI identity in the verification handshake.

use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=RUSTC");
    let output = Command::new(env::var_os("RUSTC").ok_or("missing RUSTC")?)
        .arg("-vV")
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other("could not identify the lint compiler").into());
    }
    let directory = PathBuf::from(env::var_os("OUT_DIR").ok_or("missing OUT_DIR")?);
    fs::write(directory.join("compiler.txt"), output.stdout)?;
    return Ok(());
}
