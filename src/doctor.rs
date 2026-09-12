//! `tetractl doctor` JSON envelope (G2c). Presence-only profiles; never secret values.

use std::io::{self, Write};
use std::process::Command;

use serde::Serialize;
use serde_json::Value;

use crate::cli::CliError;
use crate::facet::{facet_bin, SCHEMA_VERSION};

/// Profile leg status: names presence, never a path or credential value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProfileStatus {
    Missing,
    Configured,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    pub ncl: bool,
    pub pack: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClusterLeg {
    pub profile: ProfileStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IntentLeg {
    pub profile: ProfileStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FacetLeg {
    /// Basename of [`crate::facet::facet_bin`] only (never a full path).
    pub bin: String,
    pub reachable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvPresence {
    pub facet_session: bool,
    pub tetra_session: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub schema_version: u64,
    pub capabilities: Capabilities,
    pub cluster: ClusterLeg,
    pub intent: IntentLeg,
    pub facet: FacetLeg,
    pub env: EnvPresence,
}

impl DoctorReport {
    pub fn collect() -> Self {
        let bin = facet_bin();
        let reachable = facet_reachable(&bin);
        let capabilities = probe_capabilities(&bin, reachable);
        Self {
            schema_version: SCHEMA_VERSION,
            capabilities,
            cluster: ClusterLeg {
                profile: env_configured("FACET_KUBECONFIG"),
            },
            intent: IntentLeg {
                profile: env_configured("FACET_HEDRON_DB"),
            },
            facet: FacetLeg {
                bin: path_basename(&bin),
                reachable,
            },
            env: EnvPresence {
                facet_session: env_is_set("FACET_SESSION"),
                tetra_session: env_is_set("TETRA_SESSION"),
            },
        }
    }

    pub fn to_json(&self) -> Result<Value, CliError> {
        let value = serde_json::to_value(self)
            .map_err(|error| CliError::engine(format!("doctor JSON encode failed: {error}"), 127))?;
        reject_secret_fields(&value)?;
        Ok(value)
    }
}

pub fn run(json: bool) -> Result<(), CliError> {
    let report = DoctorReport::collect();
    if json {
        let value = report.to_json()?;
        let bytes = serde_json::to_vec(&value)
            .map_err(|error| CliError::engine(format!("doctor JSON encode failed: {error}"), 127))?;
        io::stdout().write_all(&bytes).ok();
        println!();
        Ok(())
    } else {
        print_human(&report);
        Ok(())
    }
}

fn print_human(report: &DoctorReport) {
    println!("tetractl doctor");
    println!(
        "capabilities: ncl={} pack={}",
        status_word(report.capabilities.ncl),
        status_word(report.capabilities.pack)
    );
    println!("cluster.profile: {}", profile_word(report.cluster.profile));
    println!("intent.profile: {}", profile_word(report.intent.profile));
    println!(
        "facet: {} ({})",
        report.facet.bin,
        if report.facet.reachable { "reachable" } else { "missing" }
    );
}

fn status_word(ok: bool) -> &'static str {
    if ok { "ok" } else { "missing" }
}

fn profile_word(status: ProfileStatus) -> &'static str {
    match status {
        ProfileStatus::Missing => "missing",
        ProfileStatus::Configured => "configured",
    }
}

fn env_is_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
}

fn env_configured(name: &str) -> ProfileStatus {
    if env_is_set(name) {
        ProfileStatus::Configured
    } else {
        ProfileStatus::Missing
    }
}

fn path_basename(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_owned()
}

fn facet_reachable(bin: &str) -> bool {
    Command::new(bin)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn probe_capabilities(bin: &str, reachable: bool) -> Capabilities {
    if !reachable {
        return Capabilities {
            ncl: false,
            pack: false,
        };
    }
    Capabilities {
        ncl: facet_subcommand_ok(bin, &["ncl", "--help"]),
        pack: facet_subcommand_ok(bin, &["ncl", "pack", "--help"]),
    }
}

fn facet_subcommand_ok(bin: &str, args: &[&str]) -> bool {
    Command::new(bin)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Reject any key that could carry secret material in doctor JSON.
pub fn reject_secret_fields(value: &Value) -> Result<(), CliError> {
    walk_value(value, "")
}

fn walk_value(value: &Value, path: &str) -> Result<(), CliError> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                if is_forbidden_key(key) {
                    return Err(CliError::engine(
                        format!("doctor JSON must not include secret field `{child_path}`"),
                        127,
                    ));
                }
                walk_value(child, &child_path)?;
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                walk_value(child, &format!("{path}[{index}]"))?;
            }
        }
        Value::String(text) => {
            if looks_like_secret_value(text) {
                return Err(CliError::engine(
                    format!("doctor JSON must not include secret value at `{path}`"),
                    127,
                ));
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
    Ok(())
}

fn is_forbidden_key(key: &str) -> bool {
    matches!(
        key,
        "token"
            | "kubeconfig"
            | "secret"
            | "password"
            | "credential"
            | "hedronToken"
            | "facetSecretKey"
            | "facetKubeContext"
            | "facetKubeconfig"
            | "facetHedronDb"
            | "facetHedronToken"
    )
}

fn looks_like_secret_value(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("kubeconfig")
        || lower.ends_with(".pem")
        || lower.ends_with(".key")
        || lower.contains("/.kube/")
        || lower.starts_with("eyj") // jwt-ish
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_json_has_schema_version_and_capabilities() {
        let report = DoctorReport {
            schema_version: SCHEMA_VERSION,
            capabilities: Capabilities {
                ncl: true,
                pack: true,
            },
            cluster: ClusterLeg {
                profile: ProfileStatus::Missing,
            },
            intent: IntentLeg {
                profile: ProfileStatus::Missing,
            },
            facet: FacetLeg {
                bin: "facet".to_owned(),
                reachable: true,
            },
            env: EnvPresence {
                facet_session: false,
                tetra_session: false,
            },
        };
        let json = report.to_json().unwrap();
        assert_eq!(json["schemaVersion"], SCHEMA_VERSION);
        assert_eq!(json["capabilities"]["ncl"], true);
        assert_eq!(json["cluster"]["profile"], "missing");
    }

    #[test]
    fn reject_secret_fields_catches_token_and_kubeconfig_keys() {
        let bad = serde_json::json!({
            "schemaVersion": 1,
            "token": "hunter2"
        });
        let err = reject_secret_fields(&bad).unwrap_err();
        assert!(format!("{err:?}").contains("token"));

        let bad = serde_json::json!({
            "cluster": { "kubeconfig": "/home/me/.kube/config" }
        });
        assert!(reject_secret_fields(&bad).is_err());
    }

    #[test]
    fn collect_never_puts_env_values_in_json() {
        let report = DoctorReport::collect();
        let json = report.to_json().unwrap();
        let text = json.to_string().to_ascii_lowercase();
        assert!(!text.contains("hunter2"));
        assert!(!text.contains(".kube/"));
        assert!(json.get("token").is_none());
        assert!(json.get("kubeconfig").is_none());
        assert!(json["env"]["facetSession"].is_boolean());
    }
}
