//! G4c: `tetractl blame --json` shape.

use std::process::Command;

use serde_json::Value;

fn blame_json(message: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_tetractl"))
        .args(["blame", message, "--json"])
        .output()
        .expect("blame");
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).expect("blame JSON")
}

#[test]
fn blame_json_has_required_fields() {
    let json = blame_json("violates PodSecurity restricted:v1.34: pod must not request root");
    assert_eq!(json["schemaVersion"], 1);
    assert_eq!(json["engine"], "h3s");
    assert_eq!(json["code"], 403);
    assert_eq!(json["layer"], "policy");
    assert_eq!(json["owner"], "PodSecurity");
    assert!(json["sentence"].is_string());
}

#[test]
fn blame_json_422_runtime_overlay_owner() {
    let json = blame_json(
        "Pod cannot run under the restricted-v1 runtime profile (k8s-1.34-h3s-0.9.1): token projection is not allowed",
    );
    assert_eq!(json["code"], 422);
    assert_eq!(json["layer"], "runtime");
    assert_eq!(json["contractSet"], "k8s-1.34-h3s-0.9.1");
    assert_eq!(json["owner"], "overlay");
}
