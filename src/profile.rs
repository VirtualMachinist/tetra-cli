//! Shared profile presence checks (names only, never values).

use serde::Serialize;

/// Profile leg status: names presence, never a path or credential value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProfileStatus {
    Missing,
    Configured,
}

pub const FACET_HEDRON_DB_ENV: &str = "FACET_HEDRON_DB";
pub const FACET_KUBECONFIG_ENV: &str = "FACET_KUBECONFIG";
pub const HEDRON_BIN_ENV: &str = "HEDRON_BIN";
pub const KUBECTL_BIN_ENV: &str = "KUBECTL_BIN";

pub fn env_is_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
}

pub fn env_configured(name: &str) -> ProfileStatus {
    if env_is_set(name) {
        ProfileStatus::Configured
    } else {
        ProfileStatus::Missing
    }
}

pub fn hedron_bin() -> String {
    std::env::var(HEDRON_BIN_ENV).unwrap_or_else(|_| "hedron".to_owned())
}

pub fn kubectl_bin() -> String {
    std::env::var(KUBECTL_BIN_ENV).unwrap_or_else(|_| "kubectl".to_owned())
}
