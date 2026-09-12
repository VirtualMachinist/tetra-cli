use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand};

use crate::facet;

pub const WRAP_ABOUT: &str =
    "World command for the Hedronite platform — a verbiage wrapper, not a second Nickel VM.";

pub const WRAP_LONG_ABOUT: &str = "\
Nickel is the language. tetractl (and the tetra binary alias) is the world command.

Tetractl wraps Facet, HedronDB, and kubectl as they are. It does not reimplement eval, \
put, admission, or Lattice history. facet ncl check|export|apply remains the engine; \
tetractl check|eval|apply execs that surface.

Tetra does not embed nickel-lang-core or hedron-ncl. There is no second VM.";

#[derive(Parser, Debug)]
#[command(
    name = "tetractl",
    version,
    about = WRAP_ABOUT,
    long_about = WRAP_LONG_ABOUT,
    disable_help_subcommand = true,
)]
pub struct Cli {
    /// Emit Facet JSON (schemaVersion 1) on stdout.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Parse and typecheck a Nickel world module (wraps `facet ncl check`).
    Check {
        /// Path to a .ncl module.
        path: PathBuf,
        /// Extra flags forwarded to facet (`--var`, `--import-path`, …).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        rest: Vec<String>,
    },
    /// Export a Nickel world module (wraps `facet ncl export`).
    Eval {
        /// Path to a .ncl module.
        path: PathBuf,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        rest: Vec<String>,
    },
    /// Apply a Nickel world module (wraps `facet ncl apply`).
    Apply {
        /// Path to a .ncl module.
        path: PathBuf,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        rest: Vec<String>,
    },
}

#[derive(Debug)]
pub struct CliError {
    message: String,
    code: u8,
}

impl CliError {
    pub fn usage(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: 2,
        }
    }

    pub fn engine(message: impl Into<String>, code: u8) -> Self {
        Self {
            message: message.into(),
            code,
        }
    }

    pub fn exit(self) -> ExitCode {
        if !self.message.is_empty() {
            eprintln!("{}", self.message);
        }
        ExitCode::from(self.code)
    }
}

pub fn execute(invoked_as: &str) -> Result<(), CliError> {
    let cli = Cli::parse_from(prepend_name(invoked_as));
    match cli.command {
        None => {
            let mut cmd = Cli::command();
            cmd.print_help()
                .map_err(|error| CliError::usage(error.to_string()))?;
            println!();
            Ok(())
        }
        Some(Command::Check { path, rest }) => facet::run("check", &path, &rest, cli.json),
        Some(Command::Eval { path, rest }) => facet::run("export", &path, &rest, cli.json),
        Some(Command::Apply { path, rest }) => facet::run("apply", &path, &rest, cli.json),
    }
}

fn prepend_name(invoked_as: &str) -> Vec<String> {
    let mut argv: Vec<String> = std::env::args().collect();
    if argv.is_empty() {
        argv.push(invoked_as.to_owned());
    } else {
        argv[0] = invoked_as.to_owned();
    }
    argv
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn help_mentions_wrap_not_reimplement() {
        let help = Cli::command().render_long_help().to_string();
        assert!(
            help.contains("wrap"),
            "help should describe wrapping engines"
        );
        assert!(
            help.contains("does not reimplement"),
            "help should say wrap does not reimplement eval"
        );
        assert!(
            help.contains("nickel-lang-core"),
            "help should mention no second VM"
        );
    }

    #[test]
    fn global_json_flag_is_documented() {
        let help = Cli::command().render_help().to_string();
        assert!(help.contains("--json"), "help should document --json");
    }
}
