//! G2c: `tetractl doctor --json` schema and no secret path fields.

use std::process::Command;

use serde_json::Value;

fn doctor_json() -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_tetractl"))
        .args(["doctor", "--json"])
        .output()
        .expect("tetractl doctor --json");
    assert!(
        output.status.success(),
        "doctor failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("doctor stdout is JSON")
}

fn assert_no_secret_fields(value: &Value, path: &str) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                let lower = key.to_ascii_lowercase();
                assert!(
                    !lower.contains("token")
                        && !lower.contains("kubeconfig")
                        && !lower.contains("secret")
                        && !lower.contains("password"),
                    "secret-like key at {child_path}"
                );
                assert_no_secret_fields(child, &child_path);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                assert_no_secret_fields(child, &format!("{path}[{index}]"));
            }
        }
        Value::String(text) => {
            let lower = text.to_ascii_lowercase();
            assert!(
                !lower.contains("/.kube/") && !lower.ends_with(".pem"),
                "secret-like value at {path}"
            );
        }
        _ => {}
    }
}

#[test]
fn doctor_json_schema_is_stable() {
    let json = doctor_json();
    assert_eq!(json["schemaVersion"], 1);
    assert!(json["capabilities"]["ncl"].is_boolean());
    assert!(json["capabilities"]["pack"].is_boolean());
    assert!(json["cluster"]["profile"].is_string());
    assert!(json["intent"]["profile"].is_string());
    assert!(json["facet"]["bin"].is_string());
    assert!(json["facet"]["reachable"].is_boolean());
    assert!(json["env"]["facetSession"].is_boolean());
    assert!(json["env"]["tetraSession"].is_boolean());
}

#[test]
fn doctor_json_has_no_secret_path_fields() {
    let json = doctor_json();
    assert_no_secret_fields(&json, "");
    let text = json.to_string().to_ascii_lowercase();
    assert!(!text.contains("kubeconfig\":\""));
    assert!(!text.contains("token\":\""));
}

#[test]
fn castle_like_host_reports_cluster_profile_missing() {
    let output = Command::new(env!("CARGO_BIN_EXE_tetractl"))
        .env_remove("FACET_KUBECONFIG")
        .args(["doctor", "--json"])
        .output()
        .expect("doctor");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["cluster"]["profile"], "missing");
}
