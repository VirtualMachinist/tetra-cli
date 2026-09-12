use std::process::ExitCode;

fn main() -> ExitCode {
    let bin = tetractl::bin_name();
    match tetractl::run(bin) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => error.exit(),
    }
}
