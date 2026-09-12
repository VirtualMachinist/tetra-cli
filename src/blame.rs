//! `tetractl blame`: parse h3s Status JSON or kubectl stderr and classify refusals.
//! Never evaluates Nickel or execs Facet/Hedron.

use std::fs;
use std::io::{self, Read};
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::cli::CliError;
use crate::facet::SCHEMA_VERSION;

/// Classified admission refusal (G4a parse + G4b layer/owner).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlameReport {
    pub schema_version: u64,
    pub engine: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<u16>,
    pub layer: &'static str,
    pub owner: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_set: Option<String>,
    pub sentence: String,
}

impl BlameReport {
    pub fn classify(input: &str) -> Result<Self, CliError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(CliError::usage("blame input is empty"));
        }
        if trimmed.starts_with('{') {
            classify_status_json(trimmed)
        } else {
            classify_text(trimmed)
        }
    }

    pub fn to_json(&self) -> Result<Value, CliError> {
        serde_json::to_value(self).map_err(|error| CliError::engine(error.to_string(), 127))
    }
}

pub fn run(input: Option<&str>, json: bool) -> Result<(), CliError> {
    let text = load_input(input)?;
    let report = BlameReport::classify(&text)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report.to_json()?).unwrap());
    } else {
        print_human(&report);
    }
    Ok(())
}

fn load_input(input: Option<&str>) -> Result<String, CliError> {
    match input {
        None | Some("-") => {
            let mut buffer = String::new();
            io::stdin()
                .read_to_string(&mut buffer)
                .map_err(|error| CliError::engine(error.to_string(), 127))?;
            Ok(buffer)
        }
        Some(text) => {
            let path = Path::new(text);
            if path.is_file() {
                fs::read_to_string(path).map_err(|error| {
                    CliError::usage(format!("failed to read {text}: {error}"))
                })
            } else {
                Ok(text.to_owned())
            }
        }
    }
}

fn classify_status_json(input: &str) -> Result<BlameReport, CliError> {
    let value: Value = serde_json::from_str(input)
        .map_err(|error| CliError::usage(format!("status JSON is invalid: {error}")))?;
    let message = value
        .get("message")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CliError::usage("status JSON missing message field"))?
        .to_owned();
    let code = value
        .get("code")
        .and_then(|v| v.as_u64())
        .map(|v| v as u16);
    classify_message("h3s", code, message)
}

fn classify_text(input: &str) -> Result<BlameReport, CliError> {
    let engine = if input.contains("Error from server") {
        "h3s"
    } else if input.contains("The request is invalid:") {
        "h3s"
    } else if input.contains("violates PodSecurity") || input.contains("Pod cannot run under the") {
        "h3s"
    } else {
        "kubectl"
    };
    let code = if input.contains("(Forbidden)") {
        Some(403)
    } else if input.contains("(Invalid)") || input.contains("The request is invalid:") {
        Some(422)
    } else if input.contains("(NotFound)") {
        None
    } else {
        None
    };
    let message = normalize_message(input);
    if message.contains("Pod cannot run under the") {
        return classify_runtime(engine, code.or(Some(422)), &message);
    }
    if message.contains("violates PodSecurity") {
        return classify_policy(engine, code.or(Some(403)), &message);
    }
    Ok(BlameReport {
        schema_version: SCHEMA_VERSION,
        engine,
        code,
        layer: "unknown",
        owner: "unknown",
        profile: None,
        contract_set: None,
        sentence: message,
    })
}

fn classify_message(
    engine: &'static str,
    code: Option<u16>,
    message: String,
) -> Result<BlameReport, CliError> {
    let normalized = normalize_message(&message);
    if normalized.contains("Pod cannot run under the") {
        return classify_runtime(engine, code.or(Some(422)), &normalized);
    }
    if normalized.contains("violates PodSecurity") {
        return classify_policy(engine, code.or(Some(403)), &normalized);
    }
    Ok(BlameReport {
        schema_version: SCHEMA_VERSION,
        engine,
        code,
        layer: "unknown",
        owner: "unknown",
        profile: None,
        contract_set: None,
        sentence: normalized,
    })
}

fn classify_runtime(
    engine: &'static str,
    code: Option<u16>,
    message: &str,
) -> Result<BlameReport, CliError> {
    let parsed = parse_runtime(message)?;
    let owner = runtime_owner(&parsed.sentence);
    Ok(BlameReport {
        schema_version: SCHEMA_VERSION,
        engine,
        code,
        layer: "runtime",
        owner,
        profile: Some(parsed.profile),
        contract_set: parsed.contract_set,
        sentence: parsed.sentence,
    })
}

fn classify_policy(
    engine: &'static str,
    code: Option<u16>,
    message: &str,
) -> Result<BlameReport, CliError> {
    let parsed = parse_policy(message)?;
    Ok(BlameReport {
        schema_version: SCHEMA_VERSION,
        engine,
        code,
        layer: "policy",
        owner: "PodSecurity",
        profile: Some(format!("{}:v1.34", parsed.policy)),
        contract_set: None,
        sentence: parsed.sentence,
    })
}

struct RuntimeParts {
    profile: String,
    contract_set: Option<String>,
    sentence: String,
}

fn parse_runtime(message: &str) -> Result<RuntimeParts, CliError> {
    let rest = message
        .split("Pod cannot run under the ")
        .nth(1)
        .ok_or_else(|| CliError::usage("runtime sentence missing prefix"))?;
    let (profile, after_profile) = rest
        .split_once(" runtime profile")
        .ok_or_else(|| CliError::usage("runtime sentence missing profile"))?;
    if let Some((contract_set, sentence)) = after_profile
        .strip_prefix(" (")
        .and_then(|tail| tail.split_once("): "))
    {
        return Ok(RuntimeParts {
            profile: profile.to_owned(),
            contract_set: Some(contract_set.to_owned()),
            sentence: sentence.to_owned(),
        });
    }
    let sentence = after_profile
        .strip_prefix(": ")
        .ok_or_else(|| CliError::usage("runtime sentence missing tail"))?;
    Ok(RuntimeParts {
        profile: profile.to_owned(),
        contract_set: None,
        sentence: sentence.to_owned(),
    })
}

struct PolicyParts {
    policy: String,
    sentence: String,
}

fn parse_policy(message: &str) -> Result<PolicyParts, CliError> {
    let rest = message
        .split("violates PodSecurity ")
        .nth(1)
        .ok_or_else(|| CliError::usage("policy sentence missing PodSecurity prefix"))?;
    let (policy, sentence) = rest
        .split_once(":v1.34: ")
        .ok_or_else(|| CliError::usage("policy sentence missing :v1.34: separator"))?;
    Ok(PolicyParts {
        policy: policy.to_owned(),
        sentence: sentence.to_owned(),
    })
}

fn normalize_message(input: &str) -> String {
    if let Some(rest) = input.split("Pod cannot run under the ").nth(1) {
        return format!("Pod cannot run under the {rest}");
    }
    if let Some(rest) = input.split("violates PodSecurity ").nth(1) {
        return format!("violates PodSecurity {rest}");
    }
    input.to_owned()
}

fn runtime_owner(sentence: &str) -> &'static str {
    if sentence.contains("token projection")
        || sentence.contains("volume sources")
        || sentence.contains("service environment")
        || sentence.contains("volume source")
        || sentence.contains("execution field")
        || sentence.contains("ConfigMap and Secret")
    {
        "overlay"
    } else {
        "runtime"
    }
}

fn print_human(report: &BlameReport) {
    let code = report
        .code
        .map(|value| value.to_string())
        .unwrap_or_else(|| "?".to_owned());
    println!(
        "{} {} layer={} owner={}",
        report.engine, code, report.layer, report.owner
    );
    println!("sentence: {}", report.sentence);
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUNTIME_422: &str =
        "Pod cannot run under the restricted-v1 runtime profile (k8s-1.34-h3s-0.9.1): automountServiceAccountToken must be false (token projection).";

    const POLICY_403: &str =
        "violates PodSecurity restricted:v1.34: spec.containers[0].runAsNonRoot: Invalid value: false: must be true";

    #[test]
    fn parses_h3s_status_json_422() {
        let input = serde_json::json!({
            "kind": "Status",
            "apiVersion": "v1",
            "status": "Failure",
            "message": RUNTIME_422,
            "reason": "Invalid",
            "code": 422
        });
        let report = BlameReport::classify(&input.to_string()).unwrap();
        assert_eq!(report.code, Some(422));
        assert_eq!(report.layer, "runtime");
        assert_eq!(report.owner, "overlay");
        assert_eq!(report.contract_set.as_deref(), Some("k8s-1.34-h3s-0.9.1"));
    }

    #[test]
    fn parses_policy_sentence() {
        let report = BlameReport::classify(POLICY_403).unwrap();
        assert_eq!(report.code, Some(403));
        assert_eq!(report.layer, "policy");
        assert_eq!(report.owner, "PodSecurity");
    }

    #[test]
    fn lima_runtime_without_contract_set() {
        let input = r#"The request is invalid: error when creating "STDIN": Pod cannot run under the restricted-v1 runtime profile: service-account token projection is not implemented"#;
        let report = BlameReport::classify(input).unwrap();
        assert_eq!(report.code, Some(422));
        assert_eq!(report.contract_set, None);
        assert_eq!(report.owner, "overlay");
    }
}
