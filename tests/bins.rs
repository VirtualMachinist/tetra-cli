use std::process::Command;

#[test]
fn tetractl_and_tetra_share_help() {
    let tetractl = Command::new(env!("CARGO_BIN_EXE_tetractl"))
        .arg("--help")
        .output()
        .expect("tetractl --help");
    assert!(tetractl.status.success(), "tetractl --help failed");
    let tetra = Command::new(env!("CARGO_BIN_EXE_tetra"))
        .arg("--help")
        .output()
        .expect("tetra --help");
    assert!(tetra.status.success(), "tetra --help failed");
    let t_help = String::from_utf8_lossy(&tetractl.stdout);
    let e_help = String::from_utf8_lossy(&tetra.stdout);
    assert!(
        t_help.contains("does not reimplement"),
        "tetractl help should say wrap not reimplement"
    );
    assert!(
        e_help.contains("does not reimplement"),
        "tetra help should say wrap not reimplement"
    );
}
