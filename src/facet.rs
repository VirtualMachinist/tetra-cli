//! Facet engine exec — `FACET_BIN`, session env forwarding, `schemaVersion: 1` passthrough.

use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;

use crate::cli::CliError;

/// Facet JSON envelope version (must match `facet` / `probe-cli`).
pub const SCHEMA_VERSION: u64 = 1;

/// Environment variable naming the Facet binary (default `facet` on PATH).
pub const FACET_BIN_ENV: &str = "FACET_BIN";

/// Tetra session id; forwarded to [`FACET_SESSION_ENV`] when that is unset.
pub const TETRA_SESSION_ENV: &str = "TETRA_SESSION";

/// Facet Lattice session id consumed by `facet ncl *`.
pub const FACET_SESSION_ENV: &str = "FACET_SESSION";

/// Resolve which Facet binary to exec.
pub fn facet_bin() -> String {
    std::env::var(FACET_BIN_ENV).unwrap_or_else(|_| "facet".to_owned())
}

/// Stamp [`FACET_SESSION_ENV`] from [`TETRA_SESSION_ENV`] when the child would not
/// already inherit a Facet session from the parent environment.
pub fn forward_session(command: &mut Command) {
    if std::env::var_os(FACET_SESSION_ENV).is_some() {
        return;
    }
    if let Ok(session) = std::env::var(TETRA_SESSION_ENV) {
        if !session.is_empty() {
            command.env(FACET_SESSION_ENV, session);
        }
    }
}

/// Validate Facet stdout JSON and return it without reshaping the envelope.
pub fn passthrough_json(stdout: &[u8]) -> Result<Value, CliError> {
    let value: Value = serde_json::from_slice(stdout)
        .map_err(|error| CliError::engine(format!("facet stdout is not JSON: {error}"), 127))?;
    let version = value.get("schemaVersion").and_then(|v| v.as_u64());
    if version != Some(SCHEMA_VERSION) {
        return Err(CliError::engine(
            format!("facet JSON schemaVersion must be {SCHEMA_VERSION}, got {version:?}"),
            127,
        ));
    }
    Ok(value)
}

/// `check` → `facet ncl check`, `export` → `facet ncl export`, `apply` → `facet ncl apply`.
pub fn run(sub: &str, path: &Path, rest: &[String], json: bool) -> Result<(), CliError> {
    let facet = facet_bin();
    let mut command = Command::new(&facet);
    command.arg("ncl").arg(sub).arg(path).args(rest);
    if json {
        command.arg("--json");
    }
    forward_session(&mut command);

    let child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            CliError::engine(format!("failed to exec {facet} ncl {sub}: {error}"), 127)
        })?;

    let output = child
        .wait_with_output()
        .map_err(|error| CliError::engine(error.to_string(), 127))?;

    if json {
        passthrough_json(&output.stdout)?;
    }

    io::stderr().write_all(&output.stderr).ok();
    io::stdout().write_all(&output.stdout).ok();

    // Parity: Facet already reported the failure on its own stdout/stderr.
    // Propagate the exit code without adding a second line.
    let code = output.status.code().unwrap_or(1) as u8;
    if output.status.success() {
        Ok(())
    } else {
        Err(CliError::engine(String::new(), code))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn mock_facet() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("mock-facet");
        fs::write(
            &script,
            r#"#!/bin/sh
session="${FACET_SESSION:-}"
printf '{"schemaVersion":1,"argv":%s,"facetSession":%s}\n' \
  "$(printf '%s' "$*" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')" \
  "$(printf '%s' "$session" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')"
"#,
        )
        .unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();
        dir
    }

    #[test]
    fn facet_bin_defaults_to_facet() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        let saved = std::env::var_os(FACET_BIN_ENV);
        std::env::remove_var(FACET_BIN_ENV);
        assert_eq!(facet_bin(), "facet");
        if let Some(value) = saved {
            std::env::set_var(FACET_BIN_ENV, value);
        }
    }

    #[test]
    fn facet_bin_honors_env() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        let saved = std::env::var_os(FACET_BIN_ENV);
        std::env::set_var(FACET_BIN_ENV, "/tmp/custom-facet");
        assert_eq!(facet_bin(), "/tmp/custom-facet");
        if let Some(value) = saved {
            std::env::set_var(FACET_BIN_ENV, value);
        } else {
            std::env::remove_var(FACET_BIN_ENV);
        }
    }

    #[test]
    fn forwards_tetra_session_when_facet_session_unset() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        let mock = mock_facet();
        let saved_bin = std::env::var_os(FACET_BIN_ENV);
        let saved_tetra = std::env::var_os(TETRA_SESSION_ENV);
        let saved_facet = std::env::var_os(FACET_SESSION_ENV);
        std::env::set_var(FACET_BIN_ENV, mock.path().join("mock-facet"));
        std::env::set_var(TETRA_SESSION_ENV, "01TETRAULID");
        std::env::remove_var(FACET_SESSION_ENV);

        let mut command = Command::new(facet_bin());
        command
            .arg("ncl")
            .arg("export")
            .arg("world.ncl")
            .arg("--json");
        forward_session(&mut command);
        let output = command.output().unwrap();
        let json = passthrough_json(&output.stdout).unwrap();
        assert_eq!(json["schemaVersion"], 1);
        assert_eq!(json["facetSession"], "01TETRAULID");

        if let Some(v) = saved_bin {
            std::env::set_var(FACET_BIN_ENV, v);
        } else {
            std::env::remove_var(FACET_BIN_ENV);
        }
        if let Some(v) = saved_tetra {
            std::env::set_var(TETRA_SESSION_ENV, v);
        } else {
            std::env::remove_var(TETRA_SESSION_ENV);
        }
        if let Some(v) = saved_facet {
            std::env::set_var(FACET_SESSION_ENV, v);
        }
    }

    #[test]
    fn does_not_override_existing_facet_session() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        let mock = mock_facet();
        let saved_bin = std::env::var_os(FACET_BIN_ENV);
        let saved_tetra = std::env::var_os(TETRA_SESSION_ENV);
        let saved_facet = std::env::var_os(FACET_SESSION_ENV);
        std::env::set_var(FACET_BIN_ENV, mock.path().join("mock-facet"));
        std::env::set_var(TETRA_SESSION_ENV, "01TETRAULID");
        std::env::set_var(FACET_SESSION_ENV, "01FACETULID");

        let mut command = Command::new(facet_bin());
        command
            .arg("ncl")
            .arg("check")
            .arg("world.ncl")
            .arg("--json");
        forward_session(&mut command);
        let output = command.output().unwrap();
        let json = passthrough_json(&output.stdout).unwrap();
        assert_eq!(json["facetSession"], "01FACETULID");

        if let Some(v) = saved_bin {
            std::env::set_var(FACET_BIN_ENV, v);
        } else {
            std::env::remove_var(FACET_BIN_ENV);
        }
        if let Some(v) = saved_tetra {
            std::env::set_var(TETRA_SESSION_ENV, v);
        } else {
            std::env::remove_var(TETRA_SESSION_ENV);
        }
        if let Some(v) = saved_facet {
            std::env::set_var(FACET_SESSION_ENV, v);
        } else {
            std::env::remove_var(FACET_SESSION_ENV);
        }
    }

    #[test]
    fn passthrough_requires_schema_version_one() {
        let err = passthrough_json(br#"{"ok":true}"#).unwrap_err();
        let debug = format!("{err:?}");
        assert!(debug.contains("schemaVersion"), "{debug}");
    }
}
