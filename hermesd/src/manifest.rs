use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RestartPolicy {
    No,
    OnFailure,
    Always,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self::No
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSpec {
    pub name: String,
    pub cmd: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub restart: RestartPolicy,
    #[serde(default)]
    pub max_rss_mb: Option<u64>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackManifest {
    pub stack: StackInfo,
    #[serde(default)]
    pub service: Vec<ServiceSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackInfo {
    pub name: String,
    #[serde(default)]
    pub autostart: bool,
}

impl StackManifest {
    pub fn from_toml(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }

    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"
[stack]
name = "hermes"
autostart = true

[[service]]
name = "dashboard"
cmd = "/opt/hermes/venv/bin/hermes dashboard --host 127.0.0.1 --port 9000"
cwd = "/root"
restart = "on-failure"
max_rss_mb = 900
env = { HERMES_HOME = "/root/.hermes" }
"#;

    #[test]
    fn parse_example() {
        let m = StackManifest::from_toml(EXAMPLE).unwrap();
        assert_eq!(m.stack.name, "hermes");
        assert!(m.stack.autostart);
        assert_eq!(m.service.len(), 1);
        let s = &m.service[0];
        assert_eq!(s.name, "dashboard");
        assert_eq!(s.restart, RestartPolicy::OnFailure);
        assert_eq!(s.max_rss_mb, Some(900));
        assert_eq!(s.env.get("HERMES_HOME").unwrap(), "/root/.hermes");
        assert!(s.enabled);
    }

    #[test]
    fn roundtrip() {
        let m = StackManifest::from_toml(EXAMPLE).unwrap();
        let out = m.to_toml().unwrap();
        let m2 = StackManifest::from_toml(&out).unwrap();
        assert_eq!(m.stack.name, m2.stack.name);
        assert_eq!(m.service.len(), m2.service.len());
    }

    #[test]
    fn restart_policy_default_no() {
        let toml = r#"
[stack]
name = "x"

[[service]]
name = "s"
cmd = "echo hi"
"#;
        let m = StackManifest::from_toml(toml).unwrap();
        assert_eq!(m.service[0].restart, RestartPolicy::No);
        assert!(m.service[0].max_rss_mb.is_none());
        assert!(m.service[0].cwd.is_none());
    }

    #[test]
    fn invalid_toml_errors() {
        let r = StackManifest::from_toml("not = [");
        assert!(r.is_err());
    }
}
