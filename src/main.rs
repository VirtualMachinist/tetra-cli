fn main() {
    let argv0 = std::env::args().next().unwrap_or_else(|| "tetractl".into());
    let bin = std::path::Path::new(&argv0)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("tetractl");
    println!(
        "{bin} 0.0.1 — Hedronite world command (charter).\n\
         Nickel is the language. This binary will wrap facet/hedron/kubectl.\n\
         Canon: https://github.com/VirtualMachinist/tetra-cli\n\
         Nothing to apply yet."
    );
}
