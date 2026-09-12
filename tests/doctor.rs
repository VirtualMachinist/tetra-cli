//! M1 G2a: `tetractl doctor` capabilities, host law, materialized pack, and
//! the rule that credentials are named, never printed. Fake secrets are
//! planted in the environment and must not leak into stdout or stderr.

use std::path::Path;
use std::process::{Command, Output};

const SECRET_KEY: &str = "SECRETKEYMATERIAL-9f3a";
const SECRET_TOKEN: &str = "TOKENVALUE-77c1";
const SECRET_PATH_PART: &str = "zzz-private-path-4e2b";

/// Run doctor with a scrubbed environment: PATH, HOME (temp), FACET_BIN if the
/// harness set it, plus `extra`.
fn doctor(cwd: &Path, extra: &[(&str, &str)], args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tetractl"));
    cmd.env_clear();
    cmd.env("PATH", std::env::var_os("PATH").unwrap_or_default());
    cmd.env("HOME", cwd);
    if let Some(bin) = std::env::var_os("FACET_BIN") {
        cmd.env("FACET_BIN", bin);
    }
    for (k, v) in extra {
        cmd.env(k, v);
    }
    cmd.arg("doctor").args(args).current_dir(cwd);
    cmd.output().expect("tetractl runs")
}

fn json(out: &Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "doctor --json must print JSON ({e}): {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

#[test]
fn castle_like_host_has_engine_pack_and_host_law_but_no_profiles() {
    let dir = tempfile::tempdir().unwrap();
    let out = doctor(dir.path(), &[], &["--json"]);
    let v = json(&out);
    assert_eq!(out.status.code(), Some(0), "{v}");
    assert_eq!(v["facet"]["reachable"], true);
    assert_eq!(v["capabilities"]["ncl"], true);
    assert_eq!(v["capabilities"]["pack"], true);
    assert_eq!(v["cluster"]["profile"], "missing");
    assert_eq!(v["intent"]["profile"], "missing");
    assert_eq!(v["env"]["tetraSession"], false);
    assert!(v["host"]["law"].as_str().unwrap().contains("tower/lima"));
    assert_eq!(v["problems"], serde_json::json!([]));
    // Pack materialized under cwd at .tetra/pack/<contractSet>/hedron-ncl/…
    assert_eq!(v["pack"]["contractSet"], "k8s-1.34-h3s-0.9.1");
    assert_eq!(v["pack"]["materialized"], true);
    assert_eq!(v["pack"]["markerMatches"], true);
    let import_path = v["pack"]["importPath"].as_str().unwrap().to_owned();
    assert_eq!(import_path, ".tetra/pack/k8s-1.34-h3s-0.9.1");
    let abs = dir.path().join(&import_path);
    assert!(abs.join("hedron-ncl/platform.ncl").is_file());
    assert!(abs
        .join("hedron-ncl/overlay/k8s-1.34-h3s-0.9.1.ncl")
        .is_file());
    let leftovers: Vec<_> = std::fs::read_dir(abs.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".staging"))
        .collect();
    assert!(leftovers.is_empty(), "staging left behind: {leftovers:?}");
    // The materialized pack lets a world import hedron-ncl/… through the wrap.
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/world.ncl"),
        dir.path().join("world.ncl"),
    )
    .unwrap();
    let mut check = Command::new(env!("CARGO_BIN_EXE_tetractl"));
    check
        .args(["check", "world.ncl", "--import-path", &import_path])
        .current_dir(dir.path());
    if let Some(bin) = std::env::var_os("FACET_BIN") {
        check.env("FACET_BIN", bin);
    }
    let check = check.output().unwrap();
    assert_eq!(
        check.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    // Idempotent.
    let again = json(&doctor(dir.path(), &[], &["--json"]));
    assert_eq!(again["pack"]["materialized"], true);
    assert_eq!(again["pack"]["markerMatches"], true);
}

#[test]
fn profiles_are_named_and_secrets_never_print() {
    let dir = tempfile::tempdir().unwrap();
    let private = dir.path().join(SECRET_PATH_PART);
    std::fs::create_dir_all(&private).unwrap();
    let kubeconfig = private.join("admin.conf");
    std::fs::write(
        &kubeconfig,
        format!("apiVersion: v1\nusers:\n- user:\n    client-key-data: {SECRET_KEY}\n"),
    )
    .unwrap();
    let hedron_db = private.join("intent.db");
    let extra = [
        ("FACET_KUBECONFIG", kubeconfig.to_str().unwrap()),
        ("FACET_HEDRON_DB", hedron_db.to_str().unwrap()),
        ("FACET_HEDRON_TOKEN", SECRET_TOKEN),
        ("TETRA_SESSION", "01TETRASESSIONULID"),
    ];
    for args in [&["--json"][..], &[][..]] {
        let out = doctor(dir.path(), &extra, args);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let all = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        for needle in [
            SECRET_KEY,
            SECRET_TOKEN,
            SECRET_PATH_PART,
            "admin.conf",
            "intent.db",
            "01TETRASESSIONULID",
        ] {
            assert!(!all.contains(needle), "doctor leaked {needle:?}:\n{all}");
        }
        if args.is_empty() {
            assert!(all.contains("cluster.profile: configured"), "{all}");
            assert!(all.contains("host law:"), "{all}");
        } else {
            let v = json(&out);
            assert_eq!(v["cluster"]["profile"], "configured");
            assert_eq!(v["intent"]["profile"], "configured");
            assert_eq!(v["env"]["tetraSession"], true);
            assert_eq!(v["env"]["facetSession"], false);
        }
    }
}

#[test]
fn missing_engine_is_reported_and_exits_five() {
    let dir = tempfile::tempdir().unwrap();
    let out = doctor(
        dir.path(),
        &[("FACET_BIN", "/nonexistent/facet")],
        &["--json"],
    );
    assert_eq!(out.status.code(), Some(5));
    let v = json(&out);
    assert_eq!(v["facet"]["reachable"], false);
    assert_eq!(v["capabilities"]["ncl"], false);
    assert_eq!(v["capabilities"]["pack"], false);
    assert_eq!(v["pack"]["materialized"], false);
    assert!(v["problems"][0].as_str().unwrap().contains("FACET_BIN"));
    assert!(
        !dir.path().join(".tetra").exists(),
        "no pack without an engine"
    );
    let human = doctor(dir.path(), &[("FACET_BIN", "/nonexistent/facet")], &[]);
    assert_eq!(human.status.code(), Some(5));
    assert!(String::from_utf8_lossy(&human.stdout).contains("problem:"));
}

#[test]
fn no_materialize_probes_without_writing_cwd() {
    let dir = tempfile::tempdir().unwrap();
    let out = doctor(dir.path(), &[], &["--json", "--no-materialize"]);
    assert_eq!(out.status.code(), Some(0));
    let v = json(&out);
    assert_eq!(v["capabilities"]["pack"], true);
    assert_eq!(v["pack"]["contractSet"], "k8s-1.34-h3s-0.9.1");
    assert_eq!(v["pack"]["materialized"], false);
    assert_eq!(v["pack"]["markerMatches"], true);
    assert_eq!(v["pack"]["importPath"], serde_json::Value::Null);
    assert!(!dir.path().join(".tetra").exists());
}
