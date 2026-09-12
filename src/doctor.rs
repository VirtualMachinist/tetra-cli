//! `tetractl doctor`: capabilities, **host law**, and the materialized pack
//! (G2a), on the presence-only JSON envelope (G2c). Never secret values.
//!
//! Host law: cluster verbs run on the engine host that holds the cluster
//! profile (tower/lima); intent verbs need the intent profile; eval runs
//! wherever facet runs. Doctor says which of those this host is, by name.
//! It also materializes the contract pack under `.tetra/pack/<contractSet>/`
//! so worlds can `import "hedron-ncl/…"` with `--import-path`.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use serde_json::Value;

use crate::cli::CliError;
use crate::facet::{facet_bin, SCHEMA_VERSION};
use crate::profile::{env_configured, env_is_set, ProfileStatus};

/// Exit code when the engine is unusable (facet unreachable, or no ncl/pack).
/// Never a silent pass.
pub const ENGINE_MISSING_EXIT_CODE: u8 = 5;

/// Where doctor materializes the pack, relative to the current directory.
pub const PACK_DIR: &str = ".tetra/pack";

/// The host law sentence carried in `host.law`.
pub const HOST_LAW: &str =
    "cluster verbs run on the engine host that holds the cluster profile (tower/lima); \
intent verbs need the intent profile (FACET_HEDRON_DB); eval runs wherever facet runs; \
credentials are named, never printed";

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
pub struct HostLaw {
    pub law: &'static str,
}

/// The materialized contract pack. `import_path` is relative to the current
/// directory and is the value to pass as `--import-path`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PackLeg {
    pub contract_set: Option<String>,
    pub import_path: Option<String>,
    pub materialized: bool,
    pub marker_matches: bool,
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
    pub host: HostLaw,
    pub pack: PackLeg,
    /// Human sentences for what is missing. Never carries a value.
    pub problems: Vec<String>,
}

impl DoctorReport {
    /// Collect for the current directory, materializing the pack.
    pub fn collect() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::collect_in(&cwd, true)
    }

    /// Collect for `cwd`; `materialize` writes `.tetra/pack/<contractSet>/`
    /// there, otherwise the pack is probed in a temp dir and removed.
    pub fn collect_in(cwd: &Path, materialize: bool) -> Self {
        let bin = facet_bin();
        let reachable = facet_reachable(&bin);
        let capabilities = probe_capabilities(&bin, reachable);
        let mut problems = Vec::new();
        if !reachable {
            problems.push(format!(
                "facet is not runnable as `{}`; install facet on PATH or set FACET_BIN",
                path_basename(&bin)
            ));
        } else if !capabilities.ncl {
            problems
                .push("facet has no `ncl` subcommand; need a Facet with the Nickel pack".into());
        } else if !capabilities.pack {
            problems
                .push("facet ncl has no `pack`; need Facet at or after the ncl-pack cut".into());
        }
        let pack = if capabilities.pack {
            materialize_pack(&bin, cwd, materialize, &mut problems)
        } else {
            PackLeg::default()
        };
        if env_is_set("FACET_KUBECONFIG") && !env_is_file("FACET_KUBECONFIG") {
            problems.push("cluster profile is set but is not a readable file".into());
        }
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
            host: HostLaw { law: HOST_LAW },
            pack,
            problems,
        }
    }

    /// The engine is usable: facet runs and has `ncl` and `pack`.
    pub fn engine_ok(&self) -> bool {
        self.facet.reachable && self.capabilities.ncl && self.capabilities.pack
    }

    pub fn to_json(&self) -> Result<Value, CliError> {
        let value = serde_json::to_value(self).map_err(|error| {
            CliError::engine(format!("doctor JSON encode failed: {error}"), 127)
        })?;
        reject_secret_fields(&value)?;
        Ok(value)
    }
}

pub fn run(json: bool, materialize: bool) -> Result<(), CliError> {
    let report = if materialize {
        DoctorReport::collect()
    } else {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        DoctorReport::collect_in(&cwd, false)
    };
    if json {
        let value = report.to_json()?;
        let bytes = serde_json::to_vec(&value).map_err(|error| {
            CliError::engine(format!("doctor JSON encode failed: {error}"), 127)
        })?;
        io::stdout().write_all(&bytes).ok();
        println!();
    } else {
        print_human(&report);
    }
    if report.engine_ok() {
        Ok(())
    } else {
        // The document above already says what is missing.
        Err(CliError::engine(String::new(), ENGINE_MISSING_EXIT_CODE))
    }
}

/// Run `facet ncl pack` into a staging dir; on `materialize` move it to
/// `<cwd>/.tetra/pack/<contractSet>/`, else probe and remove. Checks the marker.
fn materialize_pack(
    bin: &str,
    cwd: &Path,
    materialize: bool,
    problems: &mut Vec<String>,
) -> PackLeg {
    let base = if materialize {
        cwd.join(PACK_DIR)
    } else {
        std::env::temp_dir().join(format!("tetra-doctor-{}", std::process::id()))
    };
    // Per-process staging: several doctors may run in one directory at once.
    let staging = base.join(format!(".staging-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    if let Err(error) = fs::create_dir_all(&staging) {
        problems.push(format!(
            "cannot create the pack staging directory: {}",
            error.kind()
        ));
        return PackLeg::default();
    }
    let out = Command::new(bin)
        .args(["ncl", "pack", "--out"])
        .arg(&staging)
        .arg("--json")
        .output();
    let contract_set = match out {
        Ok(out) if out.status.success() => serde_json::from_slice::<Value>(&out.stdout)
            .ok()
            .and_then(|v| v["contractSet"].as_str().map(str::to_owned)),
        _ => None,
    };
    let Some(contract_set) = contract_set else {
        problems.push("facet ncl pack --out did not report a contractSet".into());
        let _ = fs::remove_dir_all(if materialize { staging } else { base });
        return PackLeg::default();
    };
    if !materialize {
        let marker_matches = marker_matches(&staging, &contract_set);
        let _ = fs::remove_dir_all(&base);
        return PackLeg {
            contract_set: Some(contract_set),
            import_path: None,
            materialized: false,
            marker_matches,
        };
    }
    let target = base.join(&contract_set);
    // Another doctor may have placed an identical pack meanwhile: if its
    // marker matches, keep it and drop our staging; otherwise replace it.
    let placed = if marker_matches(&target, &contract_set) {
        let _ = fs::remove_dir_all(&staging);
        true
    } else {
        let _ = fs::remove_dir_all(&target);
        match fs::rename(&staging, &target) {
            Ok(()) => true,
            Err(_) if marker_matches(&target, &contract_set) => {
                let _ = fs::remove_dir_all(&staging);
                true
            }
            Err(error) => {
                problems.push(format!(
                    "cannot place the pack under {PACK_DIR}: {}",
                    error.kind()
                ));
                let _ = fs::remove_dir_all(&staging);
                false
            }
        }
    };
    if !placed {
        return PackLeg {
            contract_set: Some(contract_set),
            ..PackLeg::default()
        };
    }
    let marker_matches = marker_matches(&target, &contract_set);
    if !marker_matches {
        problems.push("materialized pack marker does not match the reported contractSet".into());
    }
    PackLeg {
        import_path: Some(format!("{PACK_DIR}/{contract_set}")),
        contract_set: Some(contract_set),
        materialized: true,
        marker_matches,
    }
}

fn marker_matches(import_path: &Path, contract_set: &str) -> bool {
    fs::read_to_string(import_path.join("hedron-ncl").join("contract-set"))
        .map(|text| text.trim() == contract_set)
        .unwrap_or(false)
}

/// Is `name` set to a readable regular file? Metadata only; never read.
fn env_is_file(name: &str) -> bool {
    std::env::var_os(name)
        .map(PathBuf::from)
        .is_some_and(|path| fs::metadata(path).map(|m| m.is_file()).unwrap_or(false))
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
        if report.facet.reachable {
            "reachable"
        } else {
            "missing"
        }
    );
    println!(
        "pack: {} at {}{}",
        report.pack.contract_set.as_deref().unwrap_or("-"),
        report
            .pack
            .import_path
            .as_deref()
            .unwrap_or("(not materialized)"),
        if report.pack.contract_set.is_some() && !report.pack.marker_matches {
            " (marker MISMATCH)"
        } else {
            ""
        }
    );
    println!("host law: {HOST_LAW}");
    for problem in &report.problems {
        println!("problem: {problem}");
    }
}

fn status_word(ok: bool) -> &'static str {
    if ok {
        "ok"
    } else {
        "missing"
    }
}

fn profile_word(status: ProfileStatus) -> &'static str {
    match status {
        ProfileStatus::Missing => "missing",
        ProfileStatus::Configured => "configured",
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
            host: HostLaw { law: HOST_LAW },
            pack: PackLeg::default(),
            problems: Vec::new(),
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
