//! Central tool call handler with mandate authorization.
//!
//! This module integrates policy evaluation, mandate authorization, and
//! decision emission into a single handler that guarantees the always-emit
//! invariant (I1).

mod emit;
mod evaluate;
mod evaluate_next;
mod types;

pub use types::{HandleResult, ToolCallHandler, ToolCallHandlerConfig};

use super::decision::DecisionEmitter;
use super::identity::ToolIdentity;
use super::jsonrpc::JsonRpcRequest;
use super::lifecycle::LifecycleEmitter;
use super::policy::{McpPolicy, PolicyState};
use super::tool_definition::ToolDefinitionBinding;
use crate::runtime::{Authorizer, MandateData};
use serde_json::Value;
use std::sync::Arc;

impl ToolCallHandler {
    /// Create a new handler.
    pub fn new(
        policy: McpPolicy,
        authorizer: Option<Authorizer>,
        emitter: Arc<dyn DecisionEmitter>,
        config: ToolCallHandlerConfig,
    ) -> Self {
        types::new_handler(policy, authorizer, emitter, config)
    }

    /// Set the lifecycle emitter for mandate.used events (P0-B).
    pub fn with_lifecycle_emitter(self, emitter: Arc<dyn LifecycleEmitter>) -> Self {
        types::with_lifecycle_emitter(self, emitter)
    }

    /// Handle a tool call with full authorization and always-emit guarantee.
    ///
    /// This is the main entry point that enforces invariant I1: exactly one
    /// decision event is emitted for every tool call attempt.
    pub fn handle_tool_call(
        &self,
        request: &JsonRpcRequest,
        state: &mut PolicyState,
        runtime_identity: Option<&ToolIdentity>,
        mandate: Option<&MandateData>,
        transaction_object: Option<&Value>,
    ) -> HandleResult {
        evaluate::handle_tool_call(
            self,
            request,
            state,
            runtime_identity,
            None,
            mandate,
            transaction_object,
        )
    }

    /// Handle a tool call with an observed bounded tool-definition binding.
    ///
    /// This preserves the existing runtime identity/pin surface while allowing
    /// supported `tools/list` observations to be projected onto decision
    /// evidence as P56b digest visibility.
    pub fn handle_tool_call_with_tool_definition_binding(
        &self,
        request: &JsonRpcRequest,
        state: &mut PolicyState,
        runtime_identity: Option<&ToolIdentity>,
        tool_definition_binding: Option<&ToolDefinitionBinding>,
        mandate: Option<&MandateData>,
        transaction_object: Option<&Value>,
    ) -> HandleResult {
        evaluate::handle_tool_call(
            self,
            request,
            state,
            runtime_identity,
            tool_definition_binding,
            mandate,
            transaction_object,
        )
    }
}

#[cfg(test)]
mod tests;
