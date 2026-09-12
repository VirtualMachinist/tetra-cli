use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::cli::CliError;

/// `check` → `facet ncl check`, `export` → `facet ncl export`, `apply` → `facet ncl apply`.
pub fn run(sub: &str, path: &Path, rest: &[String], json: bool) -> Result<(), CliError> {
    let facet = facet_bin();
    let mut command = Command::new(&facet);
    command
        .arg("ncl")
        .arg(sub)
        .arg(path)
        .args(rest);
    if json {
        command.arg("--json");
    }
    forward_session(&mut command);

    let child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            CliError::engine(
                format!("failed to exec {facet} ncl {sub}: {error}"),
                127,
            )
        })?;

    let output = child
        .wait_with_output()
        .map_err(|error| CliError::engine(error.to_string(), 127))?;

    io::stderr().write_all(&output.stderr).ok();
    io::stdout().write_all(&output.stdout).ok();

    let code = output.status.code().unwrap_or(1) as u8;
    if output.status.success() {
        Ok(())
    } else {
        Err(CliError::engine(
            format!("{facet} ncl {sub} exited with status {code}"),
            code,
        ))
    }
}

fn facet_bin() -> String {
    std::env::var("FACET_BIN").unwrap_or_else(|_| "facet".to_owned())
}

fn forward_session(command: &mut Command) {
    if let Ok(session) = std::env::var("TETRA_SESSION") {
        command.env("FACET_SESSION", session);
    } else if let Ok(session) = std::env::var("FACET_SESSION") {
        command.env("FACET_SESSION", session);
    }
}
