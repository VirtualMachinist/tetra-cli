//! `tetractl session start|end` — one join key for Facet, Hedron, and kubectl.

use std::io::{self, Write};

use crate::cli::CliError;
use crate::facet::{
    clear_session, current_session_id, exec, passthrough_json, stamp_session, FacetOutput,
};

#[cfg(test)]
const MOCK_ULID: &str = "01SESSMOCKULID00000000001X";

/// Start a Lattice session via `facet session start`; stamp `TETRA_SESSION` = `FACET_SESSION`.
pub fn start(rest: &[String], json: bool) -> Result<(), CliError> {
    let mut args = vec!["session".to_owned(), "start".to_owned()];
    args.extend(rest.iter().cloned());
    let output = exec(args, json, false)?;
    let id = parse_session_id(&output, json)?;
    stamp_session(&id);
    emit(&output)?;
    if output.exit_code == 0 {
        Ok(())
    } else {
        Err(CliError::engine(String::new(), output.exit_code))
    }
}

/// End a session via `facet session end` (default: current `TETRA_SESSION`).
pub fn end(id: Option<&str>, rest: &[String], json: bool) -> Result<(), CliError> {
    let session_id = match id {
        Some("current") | None => current_session_id()?,
        Some(other) => other.to_owned(),
    };
    stamp_session(&session_id);

    let mut args = vec!["session".to_owned(), "end".to_owned(), session_id];
    args.extend(rest.iter().cloned());
    let output = exec(args, json, true)?;
    emit(&output)?;
    if output.exit_code == 0 {
        clear_session();
        Ok(())
    } else {
        Err(CliError::engine(String::new(), output.exit_code))
    }
}

fn emit(output: &FacetOutput) -> Result<(), CliError> {
    io::stderr().write_all(&output.stderr).ok();
    io::stdout().write_all(&output.stdout).ok();
    Ok(())
}

fn parse_session_id(output: &FacetOutput, json: bool) -> Result<String, CliError> {
    if json {
        let value = passthrough_json(&output.stdout)?;
        let id = value
            .get("session")
            .and_then(|session| session.get("id"))
            .and_then(|id| id.as_str())
            .ok_or_else(|| {
                CliError::engine("facet session JSON missing session.id", output.exit_code)
            })?;
        validate_ulid(id)?;
        Ok(id.to_owned())
    } else {
        let id = std::str::from_utf8(&output.stdout)
            .map_err(|error| CliError::engine(format!("facet session stdout: {error}"), 127))?
            .trim()
            .to_owned();
        validate_ulid(&id)?;
        Ok(id)
    }
}

fn validate_ulid(id: &str) -> Result<(), CliError> {
    if id.len() != 26 {
        return Err(CliError::engine(
            format!("facet session id is not a ULID: {id}"),
            127,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Mutex;

    use crate::facet::{FACET_BIN_ENV, FACET_SESSION_ENV, TETRA_SESSION_ENV};

    static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn mock_facet_session() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("mock-facet");
        fs::write(
            &script,
            format!(
                r#"#!/bin/sh
if [ "$1" = "session" ] && [ "$2" = "start" ]; then
  if [ "$3" = "--json" ]; then
    printf '{{"schemaVersion":1,"session":{{"id":"{id}"}}}}\n' '{id}'
  else
    printf '%s\n' '{id}'
  fi
  exit 0
fi
if [ "$1" = "session" ] && [ "$2" = "end" ]; then
  if [ "$4" = "--json" ]; then
    printf '{{"schemaVersion":1,"session":{{"id":"%s"}},"alreadyEnded":false}}\n' "$3"
  else
    printf 'session %s ended\n' "$3"
  fi
  exit 0
fi
echo "unexpected: $*" >&2
exit 1
"#,
                id = MOCK_ULID
            ),
        )
        .unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();
        dir
    }

    fn restore_env(
        saved_bin: Option<std::ffi::OsString>,
        saved_tetra: Option<std::ffi::OsString>,
        saved_facet: Option<std::ffi::OsString>,
    ) {
        for (key, val) in [
            (FACET_BIN_ENV, saved_bin),
            (TETRA_SESSION_ENV, saved_tetra),
            (FACET_SESSION_ENV, saved_facet),
        ] {
            if let Some(v) = val {
                std::env::set_var(key, v);
            } else {
                std::env::remove_var(key);
            }
        }
    }

    #[test]
    fn session_start_stamps_tetra_and_facet_session() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        let mock = mock_facet_session();
        let saved_bin = std::env::var_os(FACET_BIN_ENV);
        let saved_tetra = std::env::var_os(TETRA_SESSION_ENV);
        let saved_facet = std::env::var_os(FACET_SESSION_ENV);
        std::env::set_var(FACET_BIN_ENV, mock.path().join("mock-facet"));
        std::env::remove_var(TETRA_SESSION_ENV);
        std::env::remove_var(FACET_SESSION_ENV);

        start(&[], false).unwrap();
        assert_eq!(std::env::var(TETRA_SESSION_ENV).unwrap(), MOCK_ULID);
        assert_eq!(std::env::var(FACET_SESSION_ENV).unwrap(), MOCK_ULID);

        restore_env(saved_bin, saved_tetra, saved_facet);
    }

    #[test]
    fn session_end_clears_env() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        let mock = mock_facet_session();
        let saved_bin = std::env::var_os(FACET_BIN_ENV);
        let saved_tetra = std::env::var_os(TETRA_SESSION_ENV);
        let saved_facet = std::env::var_os(FACET_SESSION_ENV);
        std::env::set_var(FACET_BIN_ENV, mock.path().join("mock-facet"));
        stamp_session(MOCK_ULID);

        end(None, &[], false).unwrap();
        assert!(std::env::var_os(TETRA_SESSION_ENV).is_none());
        assert!(std::env::var_os(FACET_SESSION_ENV).is_none());

        restore_env(saved_bin, saved_tetra, saved_facet);
    }

    #[test]
    fn session_start_json_passthrough_schema_version() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        let mock = mock_facet_session();
        let saved_bin = std::env::var_os(FACET_BIN_ENV);
        let saved_tetra = std::env::var_os(TETRA_SESSION_ENV);
        let saved_facet = std::env::var_os(FACET_SESSION_ENV);
        std::env::set_var(FACET_BIN_ENV, mock.path().join("mock-facet"));
        std::env::remove_var(TETRA_SESSION_ENV);
        std::env::remove_var(FACET_SESSION_ENV);

        start(&[], true).unwrap();
        assert_eq!(std::env::var(TETRA_SESSION_ENV).unwrap(), MOCK_ULID);

        restore_env(saved_bin, saved_tetra, saved_facet);
    }
}
