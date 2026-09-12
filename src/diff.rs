//! `tetractl diff`: desired = wrapped Facet export; observed exportHash from Facet ledger
//! plus status legs by names (G3b/G3c). `--yaml` is a read-only view — never apply.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::cli::CliError;
use crate::facet::{exec as facet_exec, export_json, passthrough_json, SCHEMA_VERSION};
use crate::profile::ProfileStatus;
use crate::status::StatusReport;

pub const DRIFT_EXIT_CODE: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffReport {
    pub schema_version: u64,
    pub world: String,
    pub equal: bool,
    pub desired_export_hash: String,
    pub observed_export_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_run_id: Option<String>,
    pub drift: Vec<String>,
}

impl DiffReport {
    pub fn collect(world: &Path, rest: &[String]) -> Result<Self, CliError> {
        let desired = export_json(world, rest)?;
        let desired_hash = desired
            .get("exportHash")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CliError::engine("export missing exportHash", 127))?
            .to_owned();
        let status = StatusReport::collect(world, rest)?;
        let apply_row = last_apply_row(world)?;
        let observed_export = match &apply_row {
            Some(row) => {
                let hash = row["response"]["body"]["hash"].as_str().unwrap_or_default();
                if hash.is_empty() {
                    None
                } else {
                    blob_export(hash, &history_root(world))?
                }
            }
            None => None,
        };
        let observed_hash = observed_export
            .as_ref()
            .and_then(|value| value.get("exportHash"))
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let observed_run_id = apply_row.and_then(|row| row["id"].as_str().map(str::to_owned));

        let mut drift = Vec::new();
        if observed_hash.is_none() {
            drift.push("never-applied".to_owned());
        } else if observed_hash.as_deref() != Some(desired_hash.as_str()) {
            drift.push("exportHash".to_owned());
        }
        drift.extend(status_drift(&status));

        let equal = observed_hash.as_deref() == Some(desired_hash.as_str());

        Ok(Self {
            schema_version: SCHEMA_VERSION,
            world: world.to_string_lossy().into_owned(),
            equal,
            desired_export_hash: desired_hash,
            observed_export_hash: observed_hash,
            observed_run_id,
            drift,
        })
    }

    pub fn to_json(&self) -> Result<Value, CliError> {
        serde_json::to_value(self).map_err(|error| CliError::engine(error.to_string(), 127))
    }
}

pub fn run(world: &Path, rest: &[String], json: bool, yaml_view: bool) -> Result<(), CliError> {
    if yaml_view {
        let desired = export_json(world, rest)?;
        print_desired_yaml(&desired)?;
        return Ok(());
    }
    let report = DiffReport::collect(world, rest)?;
    if json {
        let value = report.to_json()?;
        println!("{}", serde_json::to_string_pretty(&value).unwrap());
    } else {
        print_human(&report);
    }
    if report.drift.is_empty() {
        Ok(())
    } else {
        Err(CliError::engine(String::new(), DRIFT_EXIT_CODE))
    }
}

fn history_root(world: &Path) -> PathBuf {
    world
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn last_apply_row(world: &Path) -> Result<Option<Value>, CliError> {
    let root = history_root(world);
    let output = facet_exec(["history", root.to_str().unwrap_or(".")], true, false)?;
    if output.exit_code != 0 {
        return Ok(None);
    }
    let history = passthrough_json(&output.stdout)?;
    Ok(history["runs"].as_array().and_then(|rows| {
        rows.iter()
            .find(|row| {
                row["requestPath"] == "ncl:apply"
                    && row["url"]
                        .as_str()
                        .map(|url| url_matches_world(url, world))
                        .unwrap_or(false)
            })
            .cloned()
    }))
}

fn url_matches_world(url: &str, world: &Path) -> bool {
    let world_str = world.to_string_lossy();
    if url.ends_with(world_str.as_ref()) {
        return true;
    }
    world
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| url.ends_with(name) || url == name)
        .unwrap_or(false)
}

fn blob_export(hash: &str, root: &Path) -> Result<Option<Value>, CliError> {
    let blob = facet_exec(["blob", hash, root.to_str().unwrap_or(".")], true, false)?;
    if blob.exit_code != 0 {
        return Ok(None);
    }
    let blob_doc = passthrough_json(&blob.stdout)?;
    let text = blob_doc["blob"]["body"]["content"]
        .as_str()
        .or_else(|| blob_doc["blob"]["body"].as_str())
        .unwrap_or_default();
    if text.is_empty() {
        return Ok(None);
    }
    Ok(serde_json::from_str(text).ok())
}

fn status_drift(status: &StatusReport) -> Vec<String> {
    let mut drift = Vec::new();
    if status.cluster.profile == ProfileStatus::Missing {
        drift.push("cluster".to_owned());
    } else {
        for item in &status.cluster.items {
            if item.error.is_some() || item.observed.is_none() {
                drift.push(format!("cluster:{}", item.name));
            }
        }
    }
    if status.intent.profile == ProfileStatus::Missing
        || status.intent.items == Value::String("n/a".to_owned())
    {
        drift.push("intent".to_owned());
    } else if let Some(rows) = status.intent.items.as_array() {
        for row in rows {
            let missing = row.get("error").is_some()
                || row
                    .get("observed")
                    .map(|value| value.is_null())
                    .unwrap_or(true);
            if missing {
                let name = row.get("name").and_then(|v| v.as_str()).unwrap_or("-");
                drift.push(format!("intent:{}", name));
            }
        }
    }
    for item in &status.calls.items {
        if item.error.is_some() || item.observed.is_none() {
            drift.push(format!("calls:{}", item.name));
        }
    }
    drift
}

fn print_human(report: &DiffReport) {
    println!("tetractl diff {}", report.world);
    println!("desired  exportHash {}", report.desired_export_hash);
    match &report.observed_export_hash {
        Some(hash) => println!("observed exportHash {}", hash),
        None => println!("observed exportHash (never applied)"),
    }
    if report.drift.is_empty() {
        println!("in sync");
    } else {
        println!("drift: {}", report.drift.join(", "));
    }
}

fn print_desired_yaml(desired: &Value) -> Result<(), CliError> {
    let mut out = String::new();
    if let Some(hash) = desired.get("exportHash").and_then(|v| v.as_str()) {
        out.push_str(&format!("exportHash: {hash}\n"));
    }
    if let Some(set) = desired.get("contractSet").and_then(|v| v.as_str()) {
        out.push_str(&format!("contractSet: {set}\n"));
    }
    for key in ["cluster", "intent", "calls"] {
        if let Some(value) = desired.get(key) {
            out.push_str(&format!("{key}:\n"));
            out.push_str(&json_to_yaml(value, 1));
        }
    }
    io::stdout().write_all(out.as_bytes()).ok();
    if !out.ends_with('\n') {
        println!();
    }
    Ok(())
}

fn json_to_yaml(value: &Value, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    match value {
        Value::Object(map) => {
            let mut out = String::new();
            for (key, child) in map {
                match child {
                    Value::Object(_) | Value::Array(_) => {
                        out.push_str(&format!("{pad}{key}:\n"));
                        out.push_str(&json_to_yaml(child, indent + 1));
                    }
                    _ => out.push_str(&format!("{pad}{key}: {}\n", yaml_scalar(child))),
                }
            }
            out
        }
        Value::Array(items) => {
            let mut out = String::new();
            for item in items {
                match item {
                    Value::Object(_) | Value::Array(_) => {
                        out.push_str(&format!("{pad}-\n"));
                        out.push_str(&json_to_yaml(item, indent + 1));
                    }
                    _ => out.push_str(&format!("{pad}- {}\n", yaml_scalar(item))),
                }
            }
            out
        }
        other => format!("{pad}{}\n", yaml_scalar(other)),
    }
}

fn yaml_scalar(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null => "null".to_owned(),
        other => other.to_string(),
    }
}
