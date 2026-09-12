//! G3c: `tetractl status --json` schema.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn sandbox() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let pack = root.join("pack");
    let facet = std::env::var_os("FACET_BIN")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("facet"));
    let out = Command::new(facet)
        .args(["ncl", "pack", "--out", pack.to_str().unwrap(), "--json"])
        .current_dir(&root)
        .output()
        .expect("facet ncl pack");
    assert!(out.status.success(), "pack failed");
    let world_dir = root.join("worlds");
    std::fs::create_dir_all(&world_dir).unwrap();
    let world = world_dir.join("prod.ncl");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/world.ncl"),
        &world,
    )
    .unwrap();
    (dir, pack, world)
}

#[test]
fn status_json_schema_is_stable() {
    let (_dir, pack, world) = sandbox();
    let output = Command::new(env!("CARGO_BIN_EXE_tetractl"))
        .env_remove("FACET_KUBECONFIG")
        .env_remove("FACET_HEDRON_DB")
        .args([
            "status",
            world.to_str().unwrap(),
            "--import-path",
            pack.to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("status");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["schemaVersion"], 1);
    assert_eq!(json["cluster"]["profile"], "missing");
    assert_eq!(json["intent"]["profile"], "missing");
    assert!(json["cluster"]["items"].is_array());
    assert_eq!(json["intent"]["items"], "n/a");
}
