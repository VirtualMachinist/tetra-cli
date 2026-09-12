//! G3c: `tetractl diff --json` and `--yaml` view flag (not apply).

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
fn diff_json_reports_drift_when_profiles_missing() {
    let (_dir, pack, world) = sandbox();
    let output = Command::new(env!("CARGO_BIN_EXE_tetractl"))
        .env_remove("FACET_KUBECONFIG")
        .env_remove("FACET_HEDRON_DB")
        .args([
            "diff",
            world.to_str().unwrap(),
            "--import-path",
            pack.to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("diff");
    assert_eq!(output.status.code(), Some(1));
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["schemaVersion"], 1);
    assert_eq!(json["equal"], false);
    assert!(!json["drift"].as_array().unwrap().is_empty());
    assert!(json["desiredExportHash"].as_str().unwrap().len() == 64);
}

#[test]
fn diff_yaml_is_view_only_and_exits_zero() {
    let (_dir, pack, world) = sandbox();
    let output = Command::new(env!("CARGO_BIN_EXE_tetractl"))
        .env_remove("FACET_KUBECONFIG")
        .args([
            "diff",
            world.to_str().unwrap(),
            "--import-path",
            pack.to_str().unwrap(),
            "--yaml",
        ])
        .output()
        .expect("diff --yaml");
    assert!(
        output.status.success(),
        "yaml view must not apply or fail on drift: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("exportHash:"));
    assert!(text.contains("cluster:"));
}
