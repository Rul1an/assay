use super::*;
use serde_json::{json, Value};

fn github_args() -> Value {
    json!({"owner": "acme", "repo": "prod-app", "title": "ci-key"})
}

fn workspace_args() -> Value {
    json!({"workspace_id": "acme", "principal": "alice@example.com"})
}

mod contract;
mod extraction;
mod record;
