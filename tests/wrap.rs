//! M1 G1a: `tetractl check|eval|apply` exec `facet ncl check|export|apply`.
//! Parity is byte parity on stdout, stderr and exit code, and Facet records
//! exactly one Lattice row per wrapped call. These tests need a real `facet`
//! (`$FACET_BIN` or `facet` on PATH) and **fail** when it is absent: a
//! silent skip would grade a wrapper that wraps nothing.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn facet_bin() -> PathBuf {
    std::env::var_os("FACET_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("facet"))
}

fn facet(args: &[&str], cwd: &Path) -> Output {
    Command::new(facet_bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("facet must be runnable for wrap tests ({e}); set FACET_BIN"))
}

fn tetractl(bin: &str, args: &[&str], cwd: &Path) -> Output {
    let exe = match bin {
        "tetra" => env!("CARGO_BIN_EXE_tetra"),
        _ => env!("CARGO_BIN_EXE_tetractl"),
    };
    Command::new(exe)
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("tetractl binary runs")
}

/// A sandbox with the pack materialized and the fixture copied beside it.
struct Sandbox {
    _dir: tempfile::TempDir,
    root: PathBuf,
    pack: PathBuf,
    world: PathBuf,
}

fn sandbox() -> Sandbox {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let pack = root.join("pack");
    let out = facet(
        &["ncl", "pack", "--out", pack.to_str().unwrap(), "--json"],
        &root,
    );
    assert!(
        out.status.success(),
        "facet ncl pack failed: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let world_dir = root.join("worlds");
    std::fs::create_dir_all(&world_dir).unwrap();
    let world = world_dir.join("prod.ncl");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/world.ncl"),
        &world,
    )
    .unwrap();
    Sandbox {
        _dir: dir,
        root,
        pack,
        world,
    }
}

fn assert_parity(tetra: &Output, facet: &Output, what: &str) {
    assert_eq!(
        tetra.status.code(),
        facet.status.code(),
        "{what}: exit code"
    );
    assert_eq!(
        String::from_utf8_lossy(&tetra.stdout),
        String::from_utf8_lossy(&facet.stdout),
        "{what}: stdout bytes"
    );
    assert_eq!(
        String::from_utf8_lossy(&tetra.stderr),
        String::from_utf8_lossy(&facet.stderr),
        "{what}: stderr bytes"
    );
}

fn rows(sb: &Sandbox) -> Vec<serde_json::Value> {
    let out = facet(&["history", "worlds", "--json"], &sb.root);
    if !out.status.success() {
        return Vec::new();
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v["runs"].as_array().cloned().unwrap_or_default()
}

#[test]
fn eval_json_is_facet_export_json_byte_for_byte() {
    let sb = sandbox();
    let world = sb.world.to_str().unwrap();
    let pack = sb.pack.to_str().unwrap();
    // `--json` as the global flag before the verb …
    let t = tetractl(
        "tetractl",
        &["--json", "eval", world, "--import-path", pack],
        &sb.root,
    );
    let f = facet(
        &["ncl", "export", world, "--import-path", pack, "--json"],
        &sb.root,
    );
    assert_parity(&t, &f, "eval --json (global)");
    // … and as a trailing flag forwarded verbatim.
    let t2 = tetractl(
        "tetractl",
        &["eval", world, "--import-path", pack, "--json"],
        &sb.root,
    );
    assert_parity(&t2, &f, "eval --json (trailing)");
    let v: serde_json::Value = serde_json::from_slice(&t.stdout).unwrap();
    assert_eq!(v["schemaVersion"], 1);
    assert_eq!(v["contractSet"], "k8s-1.34-h3s-0.9.1");
    assert_eq!(v["exportHash"].as_str().map(str::len), Some(64));
    assert_eq!(v["cluster"][0]["kind"], "Pod");
}

#[test]
fn check_and_human_output_have_parity_and_var_passes_through() {
    let sb = sandbox();
    let world = sb.world.to_str().unwrap();
    let pack = sb.pack.to_str().unwrap();
    let t = tetractl(
        "tetractl",
        &["check", world, "--import-path", pack],
        &sb.root,
    );
    let f = facet(&["ncl", "check", world, "--import-path", pack], &sb.root);
    assert_parity(&t, &f, "check human");
    assert_eq!(t.status.code(), Some(0));
    let var = "calls.info.name=\"overridden\"";
    let t = tetractl(
        "tetra",
        &["eval", world, "--import-path", pack, "--var", var, "--json"],
        &sb.root,
    );
    let f = facet(
        &[
            "ncl",
            "export",
            world,
            "--import-path",
            pack,
            "--var",
            var,
            "--json",
        ],
        &sb.root,
    );
    assert_parity(&t, &f, "eval --var");
    let v: serde_json::Value = serde_json::from_slice(&t.stdout).unwrap();
    assert_eq!(v["calls"]["info"]["name"], "overridden");
}

#[test]
fn apply_execs_facet_and_adds_exactly_one_lattice_row() {
    let sb = sandbox();
    let world = sb.world.to_str().unwrap();
    let pack = sb.pack.to_str().unwrap();
    let before = rows(&sb).len();
    let t = tetractl(
        "tetractl",
        &["--json", "apply", world, "--import-path", pack],
        &sb.root,
    );
    assert_eq!(
        t.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&t.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&t.stdout).unwrap();
    assert_eq!(v["schemaVersion"], 1);
    assert!(
        v["action"].is_object(),
        "facet apply envelope passes through: {v}"
    );
    let after = rows(&sb);
    assert_eq!(
        after.len(),
        before + 1,
        "exactly one Lattice row, written by facet"
    );
    assert_eq!(after[0]["requestPath"], "ncl:apply");
    // The same apply through facet directly adds one more, and the envelopes agree.
    let f = facet(
        &["ncl", "apply", world, "--import-path", pack, "--json"],
        &sb.root,
    );
    assert_parity(&t, &f, "apply --json");
    assert_eq!(rows(&sb).len(), before + 2);
}

#[test]
fn engine_errors_pass_through_unchanged() {
    let sb = sandbox();
    let t = tetractl(
        "tetractl",
        &["--json", "eval", "/nonexistent/world.ncl"],
        &sb.root,
    );
    let f = facet(
        &["ncl", "export", "/nonexistent/world.ncl", "--json"],
        &sb.root,
    );
    assert_parity(&t, &f, "missing module --json");
    let v: serde_json::Value = serde_json::from_slice(&t.stdout).unwrap();
    assert_eq!(v["error"]["category"], "invalid_workspace");
    assert_eq!(t.status.code(), Some(3));
    let t = tetractl("tetractl", &["check", "/nonexistent/world.ncl"], &sb.root);
    let f = facet(&["ncl", "check", "/nonexistent/world.ncl"], &sb.root);
    assert_parity(&t, &f, "missing module human");
    assert!(String::from_utf8_lossy(&t.stderr).starts_with("error[invalid_workspace]"));
}

#[test]
fn missing_facet_fails_loudly_not_silently() {
    let sb = sandbox();
    for bin in ["tetractl", "tetra"] {
        let exe = match bin {
            "tetra" => env!("CARGO_BIN_EXE_tetra"),
            _ => env!("CARGO_BIN_EXE_tetractl"),
        };
        let out = Command::new(exe)
            .args(["--json", "eval", sb.world.to_str().unwrap()])
            .env("FACET_BIN", "/nonexistent/facet")
            .current_dir(&sb.root)
            .output()
            .unwrap();
        assert_ne!(out.status.code(), Some(0), "{bin}");
        assert!(
            out.stdout.is_empty(),
            "{bin}: nothing on stdout without an engine"
        );
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("/nonexistent/facet"), "{bin}: {err}");
    }
}

#[test]
fn both_bins_wrap_and_unknown_verbs_do_not_reach_facet() {
    let sb = sandbox();
    for bin in ["tetractl", "tetra"] {
        let out = tetractl(bin, &["recon", "x.ncl", "--json"], &sb.root);
        assert_eq!(out.status.code(), Some(2), "{bin}");
        assert!(out.stdout.is_empty(), "{bin}");
        let help = tetractl(bin, &["--help"], &sb.root);
        assert_eq!(help.status.code(), Some(0));
        let text = String::from_utf8(help.stdout).unwrap();
        assert!(text.contains("does not reimplement"), "{bin}: {text}");
        assert!(
            text.contains("facet ncl check|export|apply"),
            "{bin}: {text}"
        );
    }
}
