mod cli;
mod facet;
mod session;
mod doctor;

pub use cli::CliError;

/// Binary name (`tetractl` or `tetra`) from argv0.
pub fn bin_name() -> &'static str {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| {
        let argv0 = std::env::args()
            .next()
            .unwrap_or_else(|| "tetractl".to_owned());
        std::path::Path::new(&argv0)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("tetractl")
            .to_owned()
    })
    .as_str()
}

pub fn run(invoked_as: &str) -> Result<(), CliError> {
    cli::execute(invoked_as)
}
