//! The validated operator policy for the enforcing MCP proxy.
use std::prelude::v1::*;

mod wire {
    use std::prelude::v1::*;
    tracked_struct! {
        root pub(super) struct EnforcePolicy {
            caller: Caller,
            /// The single upstream credential the proxy holds for this session. The credential-scope gate
            /// (c2) reads its `scopes`; `None` means no credential is declared, which is a fail-closed
            /// `credential_scope_unknown` (coverage cannot be determined), never a silent pass.
            #[default]
            upstream_credential: Option<UpstreamCredential>,
            #[default]
            allowances: Vec<Allowance>,
        }
    }

    tracked_struct! {
        pub(super) struct Caller {
            id: String,
        }
    }

    tracked_struct! {
        pub(super) struct UpstreamCredential {
            /// Referenced in evidence by alias, never by value (the enforcement decision record's
            /// `credential_alias`); not read by the c2 gate, which only compares `scopes`.
            alias: String,
            #[default]
            scopes: Vec<String>,
        }
    }

    tracked_struct! {
        pub(super) struct Allowance {
            action_class: String,
            #[default]
            targets: Vec<Target>,
        }
    }

    tracked_struct! {
        pub(super) struct Target {
            owner: String,
            repo: String,
        }
    }
}

/// Only the bounded reader can construct this policy. The wire types stay private.
///
/// ```compile_fail
/// let _ = assay_common::doc::load::<assay_common::enforce_document::EnforcePolicy>("caller: {id: ci}");
/// ```
#[derive(Debug)]
pub struct EnforcePolicy(wire::EnforcePolicy);

#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    Document(super::Error),
    EmptyCallerId,
}

pub fn read(text: &str) -> Result<EnforcePolicy, Error> {
    let policy = EnforcePolicy(super::read(text).map_err(Error::Document)?);
    if policy.caller_id().trim().is_empty() {
        return Err(Error::EmptyCallerId);
    }
    Ok(policy)
}

impl EnforcePolicy {
    pub fn caller_id(&self) -> &str {
        let wire::EnforcePolicy {
            caller: wire::Caller { id },
            upstream_credential: _scope_gate,
            allowances: _allowance_gate,
        } = &self.0;
        id
    }

    /// The alias is recorded; the scopes are consumed by the credential gate.
    pub fn credential(&self) -> Option<(&str, &[String])> {
        let wire::EnforcePolicy {
            caller: _caller_id,
            upstream_credential,
            allowances: _allowance_gate,
        } = &self.0;
        upstream_credential
            .as_ref()
            .map(|wire::UpstreamCredential { alias, scopes }| (alias.as_str(), scopes.as_slice()))
    }

    /// A literal target declaration. The proxy decides which action classes it can enforce.
    pub fn declares_target(&self, class: &str, owner: &str, repo: &str) -> bool {
        let wire::EnforcePolicy {
            caller: _caller_id,
            upstream_credential: _scope_gate,
            allowances,
        } = &self.0;
        allowances.iter().any(
            |wire::Allowance {
                 action_class,
                 targets,
             }| {
                action_class == class
                    && targets
                        .iter()
                        .any(|wire::Target { owner: o, repo: r }| o == owner && r == repo)
            },
        )
    }
}
