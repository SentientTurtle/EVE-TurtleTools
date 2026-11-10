use std::process::Command;
use std::error::Error;

fn main() {
    match set_git_hash_env() {
        Ok(()) => {},
        Err(err) => println!("cargo::warning=Could not retrieve GIT hash ({})", err)
    }
}

fn set_git_hash_env() -> Result<(), Box<dyn Error>> {
    let output = Command::new("git").args(&["rev-parse", "--short", "HEAD"]).output()?;
    let git_hash = String::from_utf8(output.stdout)?;
    println!("cargo::rustc-env=GIT_HASH={}", git_hash);
    println!("cargo::rerun-if-changed=README.md");   // Cargo must receive a rerun-if-changed directive or it will recompile every time. We don't need to monitor any files as cargo already monitors the source files. So just point at README.
    Ok(())
}