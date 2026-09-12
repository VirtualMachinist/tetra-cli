//! `tetractl status`: read cluster, intent, and calls legs by **names** from a World export.
//!
//! Intent leg is `n/a` without `FACET_HEDRON_DB` (Hedron via `hedron hql` subprocess only — never SQLite).

use std::path::Path;
use std::process::{Command, Stdio};

use serde::Serialize;
use serde_json::Value;

use crate::cli::CliError;
use crate::facet::{exec as facet_exec, export_json, passthrough_json, SCHEMA_VERSION};
use crate::profile::{
    env_configured, hedron_bin, kubectl_bin, FACET_HEDRON_DB_ENV, FACET_KUBECONFIG_ENV,
    ProfileStatus,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NamedObject {
    pub kind: String,
    pub name: String,
    pub observed: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IntentObject {
    pub name: String,
    pub observed: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CallsObject {
    pub name: String,
    pub observed: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClusterLeg {
    pub profile: ProfileStatus,
    pub items: Vec<NamedObject>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IntentLeg {
    pub profile: ProfileStatus,
    pub items: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CallsLeg {
    pub items: Vec<CallsObject>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusReport {
    pub schema_version: u64,
    pub world: String,
    pub contract_set: Option<String>,
    pub cluster: ClusterLeg,
    pub intent: IntentLeg,
    pub calls: CallsLeg,
}

impl StatusReport {
    pub fn collect(world: &Path, rest: &[String]) -> Result<Self, CliError> {
        let export = export_json(world, rest)?;
        let world_str = world.to_string_lossy().into_owned();
        let contract_set = export
            .get("contractSet")
            .and_then(|v| v.as_str())
            .map(str::to_owned);

        let cluster_profile = env_configured(FACET_KUBECONFIG_ENV);
        let cluster_items = cluster_names(&export)
            .into_iter()
            .map(|(kind, name)| observe_cluster(&kind, &name, cluster_profile))
            .collect();

        let intent_profile = env_configured(FACET_HEDRON_DB_ENV);
        let intent_items = match intent_profile {
            ProfileStatus::Missing => Value::String("n/a".to_owned()),
            ProfileStatus::Configured => {
                let db = std::env::var(FACET_HEDRON_DB_ENV)
                    .map_err(|_| CliError::engine("FACET_HEDRON_DB is unset", 127))?;
                let rows = intent_names(&export)
                    .into_iter()
                    .map(|name| observe_intent(&db, &name))
                    .collect::<Vec<_>>();
                serde_json::to_value(rows)
                    .map_err(|error| CliError::engine(error.to_string(), 127))?
            }
        };

        let calls_items = calls_names(&export)
            .into_iter()
            .map(|name| observe_calls(&name))
            .collect();

        Ok(Self {
            schema_version: SCHEMA_VERSION,
            world: world_str,
            contract_set,
            cluster: ClusterLeg {
                profile: cluster_profile,
                items: cluster_items,
            },
            intent: IntentLeg {
                profile: intent_profile,
                items: intent_items,
            },
            calls: CallsLeg { items: calls_items },
        })
    }

    pub fn to_json(&self) -> Result<Value, CliError> {
        serde_json::to_value(self).map_err(|error| CliError::engine(error.to_string(), 127))
    }
}

pub fn run(world: &Path, rest: &[String], json: bool) -> Result<(), CliError> {
    let report = StatusReport::collect(world, rest)?;
    if json {
        let value = report.to_json()?;
        println!("{}", serde_json::to_string_pretty(&value).unwrap());
    } else {
        print_human(&report);
    }
    Ok(())
}

fn cluster_names(export: &Value) -> Vec<(String, String)> {
    export["cluster"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|obj| {
                    let kind = obj.get("kind").and_then(|v| v.as_str())?;
                    let name = obj.pointer("/metadata/name").and_then(|v| v.as_str())?;
                    Some((kind.to_owned(), name.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn intent_names(export: &Value) -> Vec<String> {
    export["intent"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|obj| obj.get("name").and_then(|v| v.as_str()).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn calls_names(export: &Value) -> Vec<String> {
    export
        .pointer("/calls/info/name")
        .and_then(|v| v.as_str())
        .map(|name| vec![name.to_owned()])
        .unwrap_or_default()
}

fn observe_cluster(kind: &str, name: &str, profile: ProfileStatus) -> NamedObject {
    if profile == ProfileStatus::Missing {
        return NamedObject {
            kind: kind.to_owned(),
            name: name.to_owned(),
            observed: None,
            error: Some("cluster profile missing (set FACET_KUBECONFIG)".into()),
        };
    }
    match kubectl_get(kind, name) {
        Ok(value) => NamedObject {
            kind: kind.to_owned(),
            name: name.to_owned(),
            observed: Some(value),
            error: None,
        },
        Err(message) => NamedObject {
            kind: kind.to_owned(),
            name: name.to_owned(),
            observed: None,
            error: Some(message),
        },
    }
}

fn kubectl_get(kind: &str, name: &str) -> Result<Value, String> {
    let mut command = Command::new(kubectl_bin());
    command
        .args(["get", kind, name, "-o", "json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Ok(kubeconfig) = std::env::var(FACET_KUBECONFIG_ENV) {
        if !kubeconfig.is_empty() {
            command.env("KUBECONFIG", kubeconfig);
        }
    }
    let output = command
        .output()
        .map_err(|error| format!("kubectl exec failed: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("kubectl get {kind} {name} failed")
        } else {
            stderr
        });
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("kubectl stdout is not JSON: {error}"))
}

fn observe_intent(db: &str, name: &str) -> IntentObject {
    match hedron_state(db, name) {
        Ok(value) => IntentObject {
            name: name.to_owned(),
            observed: Some(value),
            error: None,
        },
        Err(message) => IntentObject {
            name: name.to_owned(),
            observed: None,
            error: Some(message),
        },
    }
}

fn hedron_state(db: &str, intent_name: &str) -> Result<Value, String> {
    let escaped = intent_name.replace('"', "\\\"");
    let query = format!(
        "filter name = \"{escaped}\" | state | select name, spec, status, state_version"
    );
    let mut command = Command::new(hedron_bin());
    command
        .args(["hql", "--db", db, "--format", "json", &query])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = command
        .output()
        .map_err(|error| format!("hedron exec failed: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "hedron hql failed".into()
        } else {
            stderr
        });
    }
    let stdout = &output.stdout;
    if stdout.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(stdout)
        .map_err(|error| format!("hedron stdout is not JSON: {error}"))
}

fn observe_calls(collection: &str) -> CallsObject {
    let args: Vec<std::ffi::OsString> = vec!["last".into(), collection.into()];
    match facet_exec(args, true, true) {
        Ok(output) if output.exit_code == 0 => match passthrough_json(&output.stdout) {
            Ok(value) => CallsObject {
                name: collection.to_owned(),
                observed: Some(value),
                error: None,
            },
            Err(err) => CallsObject {
                name: collection.to_owned(),
                observed: None,
                error: Some(format!("{err:?}")),
            },
        },
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            CallsObject {
                name: collection.to_owned(),
                observed: None,
                error: Some(if stderr.is_empty() {
                    format!("facet last {collection} failed")
                } else {
                    stderr
                }),
            }
        }
        Err(err) => CallsObject {
            name: collection.to_owned(),
            observed: None,
            error: Some(format!("{err:?}")),
        },
    }
}

fn print_human(report: &StatusReport) {
    println!("tetractl status {}", report.world);
    if let Some(set) = &report.contract_set {
        println!("contractSet: {set}");
    }
    println!("cluster.profile: {}", profile_word(report.cluster.profile));
    for item in &report.cluster.items {
        let state = if item.observed.is_some() {
            "observed"
        } else {
            "missing"
        };
        println!("  {} {}: {}", item.kind, item.name, state);
    }
    println!("intent.profile: {}", profile_word(report.intent.profile));
    if report.intent.items == Value::String("n/a".to_owned()) {
        println!("  intent: n/a");
    } else if let Some(rows) = report.intent.items.as_array() {
        for row in rows {
            let name = row.get("name").and_then(|v| v.as_str()).unwrap_or("-");
            let state = if row.get("observed").map(|v| !v.is_null()).unwrap_or(false) {
                "observed"
            } else {
                "missing"
            };
            println!("  intent {}: {}", name, state);
        }
    }
    for item in &report.calls.items {
        let state = if item.observed.is_some() {
            "observed"
        } else {
            "missing"
        };
        println!("  calls {}: {}", item.name, state);
    }
}

fn profile_word(status: ProfileStatus) -> &'static str {
    match status {
        ProfileStatus::Missing => "missing",
        ProfileStatus::Configured => "configured",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use crate::facet::ENV_TEST_LOCK;

    fn mock_export() -> Value {
        serde_json::json!({
            "schemaVersion": 1,
            "contractSet": "k8s-1.34-h3s-0.9.1",
            "cluster": [{
                "apiVersion": "v1",
                "kind": "Pod",
                "metadata": { "name": "supported-pod" }
            }],
            "intent": [{ "name": "tetra-g1-docs-eod" }],
            "calls": { "info": { "name": "tetra-g1-fixture" } }
        })
    }

    #[test]
    fn cluster_names_from_export() {
        let names = cluster_names(&mock_export());
        assert_eq!(names, vec![("Pod".into(), "supported-pod".into())]);
    }

    #[test]
    fn intent_leg_is_na_without_hedron_db() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        let saved = std::env::var_os(FACET_HEDRON_DB_ENV);
        std::env::remove_var(FACET_HEDRON_DB_ENV);
        let report = StatusReport {
            schema_version: 1,
            world: "world.ncl".into(),
            contract_set: None,
            cluster: ClusterLeg {
                profile: ProfileStatus::Missing,
                items: vec![],
            },
            intent: IntentLeg {
                profile: ProfileStatus::Missing,
                items: Value::String("n/a".into()),
            },
            calls: CallsLeg { items: vec![] },
        };
        assert_eq!(report.intent.items, Value::String("n/a".into()));
        if let Some(value) = saved {
            std::env::set_var(FACET_HEDRON_DB_ENV, value);
        }
    }

    #[test]
    fn collect_intent_na_when_hedron_profile_missing() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        let saved_db = std::env::var_os(FACET_HEDRON_DB_ENV);
        let saved_bin = std::env::var_os(crate::facet::FACET_BIN_ENV);
        std::env::remove_var(FACET_HEDRON_DB_ENV);

        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("mock-facet");
        let export = mock_export();
        fs::write(
            &script,
            format!("#!/bin/sh\nprintf '%s\\n' '{}'\n", export),
        )
        .unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();
        std::env::set_var(crate::facet::FACET_BIN_ENV, script);

        let world = dir.path().join("world.ncl");
        fs::write(&world, "export").unwrap();
        let report = StatusReport::collect(&world, &[]).unwrap();
        assert_eq!(report.intent.profile, ProfileStatus::Missing);
        assert_eq!(report.intent.items, Value::String("n/a".into()));
        assert_eq!(report.cluster.items.len(), 1);
        assert_eq!(report.cluster.items[0].name, "supported-pod");
        assert_eq!(report.calls.items[0].name, "tetra-g1-fixture");

        if let Some(value) = saved_db {
            std::env::set_var(FACET_HEDRON_DB_ENV, value);
        } else {
            std::env::remove_var(FACET_HEDRON_DB_ENV);
        }
        if let Some(value) = saved_bin {
            std::env::set_var(crate::facet::FACET_BIN_ENV, value);
        } else {
            std::env::remove_var(crate::facet::FACET_BIN_ENV);
        }
    }
}
