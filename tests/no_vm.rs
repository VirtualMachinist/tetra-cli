use std::fs;
use std::path::PathBuf;

#[test]
fn lockfile_has_no_nickel_vm() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let lock = fs::read_to_string(root.join("Cargo.lock")).expect("Cargo.lock");
    assert!(
        !lock.contains("name = \"nickel-lang-core\""),
        "tetra must not depend on nickel-lang-core"
    );
    assert!(
        !lock.contains("name = \"hedron-ncl\""),
        "tetra must not depend on hedron-ncl"
    );
}
