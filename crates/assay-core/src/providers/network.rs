use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkPolicy {
    Allow,
    Deny(String),
}

#[derive(Debug)]
struct NetworkState {
    policy: NetworkPolicy,
}

fn state() -> &'static Mutex<NetworkState> {
    static STATE: OnceLock<Mutex<NetworkState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(NetworkState {
            policy: NetworkPolicy::Allow,
        })
    })
}

pub struct NetworkPolicyGuard {
    previous: NetworkPolicy,
}

impl NetworkPolicyGuard {
    pub fn set(policy: NetworkPolicy) -> Self {
        let mut s = state().lock().expect("network policy mutex poisoned");
        let previous = s.policy.clone();
        s.policy = policy;
        Self { previous }
    }

    pub fn deny(reason: impl Into<String>) -> Self {
        Self::set(NetworkPolicy::Deny(reason.into()))
    }
}

impl Drop for NetworkPolicyGuard {
    fn drop(&mut self) {
        if let Ok(mut s) = state().lock() {
            s.policy = self.previous.clone();
        }
    }
}

pub fn check_outbound(target: &str) -> anyhow::Result<()> {
    let policy = effective_policy();
    match policy {
        NetworkPolicy::Allow => Ok(()),
        NetworkPolicy::Deny(reason) => anyhow::bail!(
            "config error: outbound network blocked by policy (target={}): {}",
            target,
            reason
        ),
    }
}

fn effective_policy() -> NetworkPolicy {
    if let Ok(raw) = std::env::var("ASSAY_NETWORK_POLICY") {
        let mode = raw.trim().to_ascii_lowercase();
        if mode == "deny" {
            return NetworkPolicy::Deny("ASSAY_NETWORK_POLICY=deny".to_string());
        }
    }
    let s = state().lock().expect("network policy mutex poisoned");
    s.policy.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn scoped_deny_blocks_and_restores() {
        let _lock = test_lock().lock().expect("test lock poisoned");
        std::env::remove_var("ASSAY_NETWORK_POLICY");
        let _guard = NetworkPolicyGuard::deny("test deny");
        let err = check_outbound("test-target").unwrap_err().to_string();
        assert!(err.contains("outbound network blocked by policy"));
        assert!(err.contains("test-target"));
        drop(_guard);
        check_outbound("test-target").unwrap();
    }

    #[test]
    fn env_deny_overrides_scoped_allow() {
        let _lock = test_lock().lock().expect("test lock poisoned");
        let previous = std::env::var("ASSAY_NETWORK_POLICY").ok();
        let _guard = NetworkPolicyGuard::set(NetworkPolicy::Allow);
        std::env::set_var("ASSAY_NETWORK_POLICY", "deny");
        let err = check_outbound("env-target").unwrap_err().to_string();
        assert!(err.contains("ASSAY_NETWORK_POLICY=deny"));
        if let Some(v) = previous {
            std::env::set_var("ASSAY_NETWORK_POLICY", v);
        } else {
            std::env::remove_var("ASSAY_NETWORK_POLICY");
        }
    }
}
