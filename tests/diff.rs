//! M1 G3b: `tetractl diff <world.ncl>` — desired is the wrapped export;
//! observed is status by names plus the exportHash facet last applied
//! (its own ledger). Exit 1 on drift. Needs a real facet; fails loudly
//! without one. On a host without cluster/intent profiles the leg drift
//! never clears (host law), so this proves the hash rule and the plumbing.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn tetractl(args: &[&str], cwd: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tetractl"))
        .args(args)
        .env_remove("FACET_KUBECONFIG")
        .env_remove("FACET_HEDRON_DB")
        .current_dir(cwd)
        .output()
        .expect("tetractl runs")
}

struct Sandbox {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

const IMPORT: &str = ".tetra/pack/k8s-1.34-h3s-0.9.1";

fn sandbox() -> Sandbox {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let doctor = tetractl(&["doctor", "--json"], &root);
    assert_eq!(
        doctor.status.code(),
        Some(0),
        "doctor must materialize the pack: {}{}",
        String::from_utf8_lossy(&doctor.stdout),
        String::from_utf8_lossy(&doctor.stderr)
    );
    std::fs::create_dir_all(root.join("worlds")).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/world.ncl"),
        root.join("worlds/prod.ncl"),
    )
    .unwrap();
    Sandbox { _dir: dir, root }
}

fn json(out: &Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "diff --json must print JSON ({e}): {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn drift(v: &serde_json::Value) -> Vec<String> {
    v["drift"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn hash_rule_never_applied_then_applied_then_drift() {
    let sb = sandbox();
    let args = ["diff", "worlds/prod.ncl", "--import-path", IMPORT, "--json"];
    // 1. Never applied.
    let out = tetractl(&args, &sb.root);
    let v = json(&out);
    assert_eq!(out.status.code(), Some(1), "{v}");
    assert_eq!(v["schemaVersion"], 1);
    assert_eq!(v["equal"], false);
    assert!(drift(&v).contains(&"never-applied".to_owned()), "{v}");
    assert_eq!(v["observedExportHash"], serde_json::Value::Null);
    let desired = v["desiredExportHash"].as_str().unwrap().to_owned();
    assert_eq!(desired.len(), 64);
    // 2. Apply through the wrap: facet writes the row; the hash rule clears.
    let apply = tetractl(
        &[
            "--json",
            "apply",
            "worlds/prod.ncl",
            "--import-path",
            IMPORT,
        ],
        &sb.root,
    );
    assert_eq!(
        apply.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&apply.stderr)
    );
    let out = tetractl(&args, &sb.root);
    let v = json(&out);
    let d = drift(&v);
    assert!(!d.contains(&"never-applied".to_owned()), "{v}");
    assert!(!d.contains(&"exportHash".to_owned()), "{v}");
    assert_eq!(v["observedExportHash"], desired);
    assert!(v["observedRunId"].as_str().unwrap().len() >= 26);
    // Host law: no cluster/intent profile here, so those legs still drift.
    assert!(
        d.contains(&"cluster".to_owned()) && d.contains(&"intent".to_owned()),
        "{v}"
    );
    assert_eq!(out.status.code(), Some(1));
    // 3. Desired changes via --var: the hash rule drifts again.
    let out = tetractl(
        &[
            "diff",
            "worlds/prod.ncl",
            "--import-path",
            IMPORT,
            "--var",
            "calls.info.name=\"renamed\"",
            "--json",
        ],
        &sb.root,
    );
    let v = json(&out);
    assert_eq!(out.status.code(), Some(1));
    assert!(drift(&v).contains(&"exportHash".to_owned()), "{v}");
    assert_ne!(v["desiredExportHash"], v["observedExportHash"]);
    // 4. Editing the module drifts too; human mode names both hashes.
    let module = sb.root.join("worlds/prod.ncl");
    let edited = std::fs::read_to_string(&module)
        .unwrap()
        .replace("name = \"supported-pod\"", "name = \"supported-pod-2\"");
    std::fs::write(&module, edited).unwrap();
    let human = tetractl(
        &["diff", "worlds/prod.ncl", "--import-path", IMPORT],
        &sb.root,
    );
    assert_eq!(human.status.code(), Some(1));
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(
        text.contains("desired  ") && text.contains("observed ") && text.contains("exportHash"),
        "{text}"
    );
}

#[test]
fn diff_never_applies_and_engine_errors_pass_through() {
    let sb = sandbox();
    let apply = tetractl(
        &[
            "--json",
            "apply",
            "worlds/prod.ncl",
            "--import-path",
            IMPORT,
        ],
        &sb.root,
    );
    assert_eq!(apply.status.code(), Some(0));
    let facet = std::env::var_os("FACET_BIN").unwrap_or_else(|| "facet".into());
    let rows = |dir: &Path| -> usize {
        let out = Command::new(&facet)
            .args(["history", "worlds", "--json"])
            .current_dir(dir)
            .output()
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        v["runs"]
            .as_array()
            .map(|a| a.iter().filter(|r| r["requestPath"] == "ncl:apply").count())
            .unwrap_or(0)
    };
    let before = rows(&sb.root);
    for _ in 0..3 {
        tetractl(
            &["diff", "worlds/prod.ncl", "--import-path", IMPORT, "--json"],
            &sb.root,
        );
        tetractl(
            &["diff", "worlds/prod.ncl", "--import-path", IMPORT, "--yaml"],
            &sb.root,
        );
    }
    assert_eq!(rows(&sb.root), before, "diff must not apply");
    // A module facet cannot read: facet's own envelope and exit code.
    let out = tetractl(&["diff", "worlds/missing.ncl", "--json"], &sb.root);
    assert_eq!(out.status.code(), Some(3));
    let v = json(&out);
    assert_eq!(v["error"]["category"], "invalid_workspace");
}
