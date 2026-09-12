//! M1 G4: `tetractl blame` classifies h3s refusals from the G5 lima run.
//! Fixtures are the exact kubectl stderr lines and Status bodies the pair
//! produced; the binary never evaluates Nickel or calls Facet.

use std::io::Write;
use std::process::{Command, Output, Stdio};

/// kubectl stderr from the v0.9.1 pair (G5c, 2026-09-12).
const LIMA_422_AUTOMOUNT: &str = r#"The request is invalid: error when creating "STDIN": Pod cannot run under the restricted-v1 runtime profile: service-account token projection is not implemented"#;
const LIMA_422_PVC: &str = r#"The request is invalid: error when creating "STDIN": Pod cannot run under the restricted-v1 runtime profile: only ConfigMap and Secret volume sources are implemented"#;
const LIMA_403_ROOT: &str = r#"Error from server (Forbidden): error when creating "STDIN": violates PodSecurity restricted:v1.34: spec.containers[0].securityContext must not request root"#;
/// Same refusal from an h3s built at the G4 branch, which quotes the contract set.
const G4_422_SUFFIXED: &str = r#"The request is invalid: error when creating "STDIN": Pod cannot run under the restricted-v1 runtime profile (k8s-1.34-h3s-0.9.1): service-account token projection is not implemented"#;

fn blame(bin: &str, args: &[&str], stdin: Option<&str>) -> Output {
    let exe = match bin {
        "tetra" => env!("CARGO_BIN_EXE_tetra"),
        _ => env!("CARGO_BIN_EXE_tetractl"),
    };
    let mut cmd = Command::new(exe);
    cmd.arg("blame").args(args);
    // No engine may be consulted: point FACET_BIN at nothing and prove it is not used.
    cmd.env("FACET_BIN", "/nonexistent/facet");
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    if let Some(text) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    } else {
        drop(child.stdin.take());
    }
    child.wait_with_output().unwrap()
}

fn json(out: &Output) -> serde_json::Value {
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn automount_true_is_422_runtime_owned_by_overlay() {
    let v = json(&blame("tetractl", &["--json"], Some(LIMA_422_AUTOMOUNT)));
    assert_eq!(v["schemaVersion"], 1);
    assert_eq!(v["engine"], "h3s");
    assert_eq!(v["code"], 422);
    assert_eq!(v["layer"], "runtime");
    assert_eq!(v["owner"], "overlay");
    assert_eq!(v["profile"], "restricted-v1");
    assert_eq!(
        v["contractSet"],
        serde_json::Value::Null,
        "v0.9.1 pair does not quote it"
    );
    assert_eq!(
        v["sentence"],
        "service-account token projection is not implemented"
    );
}

#[test]
fn g4_suffixed_sentence_carries_the_contract_set() {
    let v = json(&blame("tetra", &["--json"], Some(G4_422_SUFFIXED)));
    assert_eq!(v["code"], 422);
    assert_eq!(v["layer"], "runtime");
    assert_eq!(v["owner"], "overlay");
    assert_eq!(v["contractSet"], "k8s-1.34-h3s-0.9.1");
    assert_eq!(v["profile"], "restricted-v1");
    assert_eq!(
        v["sentence"],
        "service-account token projection is not implemented"
    );
}

#[test]
fn pvc_volume_is_422_overlay_and_root_is_403_pod_security() {
    let v = json(&blame("tetractl", &["--json"], Some(LIMA_422_PVC)));
    assert_eq!(v["code"], 422);
    assert_eq!(v["layer"], "runtime");
    assert_eq!(v["owner"], "overlay");
    let v = json(&blame("tetractl", &["--json"], Some(LIMA_403_ROOT)));
    assert_eq!(v["code"], 403);
    assert_eq!(v["layer"], "policy");
    assert_eq!(v["owner"], "PodSecurity");
    assert_eq!(v["profile"], "restricted:v1.34");
    assert_eq!(v["contractSet"], serde_json::Value::Null);
    assert_eq!(
        v["sentence"],
        "spec.containers[0].securityContext must not request root"
    );
}

#[test]
fn status_json_bodies_classify_by_code_and_message() {
    let status_422 = serde_json::json!({
        "kind": "Status", "apiVersion": "v1", "status": "Failure", "reason": "Invalid", "code": 422,
        "message": "Pod cannot run under the restricted-v1 runtime profile (k8s-1.34-h3s-0.9.1): invalid container count"
    });
    let v = json(&blame(
        "tetractl",
        &["--json"],
        Some(&status_422.to_string()),
    ));
    assert_eq!(v["code"], 422);
    assert_eq!(v["layer"], "runtime");
    assert_eq!(
        v["owner"], "runtime",
        "not a release gap: platform-shaped refusal"
    );
    assert_eq!(v["contractSet"], "k8s-1.34-h3s-0.9.1");
    let status_403 = serde_json::json!({
        "kind": "Status", "status": "Failure", "reason": "Forbidden", "code": 403,
        "message": "violates PodSecurity restricted:v1.34: spec.containers[0] must drop ALL capabilities"
    });
    let v = json(&blame(
        "tetractl",
        &["--json"],
        Some(&status_403.to_string()),
    ));
    assert_eq!(v["code"], 403);
    assert_eq!(v["layer"], "policy");
    assert_eq!(v["owner"], "PodSecurity");
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("status.json");
    std::fs::write(&file, status_403.to_string()).unwrap();
    let v = json(&blame(
        "tetractl",
        &["--json", file.to_str().unwrap()],
        None,
    ));
    assert_eq!(v["layer"], "policy");
}

#[test]
fn anything_else_is_unknown_never_collapsed() {
    let v = json(&blame(
        "tetractl",
        &["--json"],
        Some("Error from server (NotFound): pods \"x\" not found"),
    ));
    assert_eq!(v["layer"], "unknown");
    assert_eq!(v["owner"], "unknown");
    assert_eq!(v["code"], serde_json::Value::Null);
    assert!(v["sentence"].as_str().unwrap().contains("not found"));
    let human = blame("tetractl", &[], Some(LIMA_403_ROOT));
    assert_eq!(human.status.code(), Some(0));
    let text = String::from_utf8(human.stdout).unwrap();
    assert!(
        text.starts_with("h3s 403 layer=policy owner=PodSecurity"),
        "{text}"
    );
    let empty = blame("tetractl", &["--json"], Some("   "));
    assert_eq!(empty.status.code(), Some(2));
}

#[test]
fn blame_source_never_evaluates_nickel_or_calls_an_engine() {
    let src =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/blame.rs")).unwrap();
    for needle in ["nickel_lang", "hedron_ncl", "facet::exec", "Command::new"] {
        assert!(!src.contains(needle), "blame.rs must not use {needle}");
    }
}

#[test]
fn workload_template_sentence_is_runtime_and_execution_field_is_runtime_owned() {
    let template = r#"The request is invalid: error when creating "STDIN": template cannot run under the restricted-v1 runtime profile (k8s-1.34-h3s-0.9.1): service environment injection is not implemented"#;
    let v = json(&blame("tetractl", &["--json"], Some(template)));
    assert_eq!(v["code"], 422);
    assert_eq!(v["layer"], "runtime");
    assert_eq!(v["owner"], "overlay");
    assert_eq!(v["contractSet"], "k8s-1.34-h3s-0.9.1");
    let probe = r#"The request is invalid: error when creating "STDIN": Pod cannot run under the restricted-v1 runtime profile: unsupported execution field livenessProbe"#;
    let v = json(&blame("tetractl", &["--json"], Some(probe)));
    assert_eq!(v["layer"], "runtime");
    assert_eq!(
        v["owner"], "runtime",
        "field allowlist is the profile's own law, not an overlay gap"
    );
}
