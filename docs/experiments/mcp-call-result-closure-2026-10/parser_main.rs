#![allow(dead_code)]
mod mcp {
    pub mod era;
    pub mod types;
    pub mod identity;
    pub mod jcs;
    pub mod json_depth;
    pub mod parser;
}
use mcp::era::*;
use mcp::types::*;
use serde_json::{json, Value};
use std::io::Read;
fn main() {
    let mut input = String::new();
    std::io::stdin().take(65537).read_to_string(&mut input).unwrap();
    if input.len() > 65536 { println!("{{\"error\":\"input_limit\"}}"); return; }
    if mcp::json_depth::exceeds_limit(input.as_bytes(), 32, true) { println!("{{\"error\":\"depth_limit\"}}"); return; }
    let parsed = match mcp::parser::parse_mcp_transcript_detailed_with_depth(&input, McpInputFormat::JsonRpc, 32) {
        Ok(p) => p, Err(_) => { println!("{{\"error\":\"parse_refused\"}}"); return; }
    };
    let mut rows = Vec::new();
    for p in parsed {
        let (kind, raw) = match &p.event.payload {
            McpPayload::ToolCallRequest {raw,..} => ("request",raw),
            McpPayload::ToolCallResponse {raw,..} => ("response",raw),
            _ => { println!("{{\"error\":\"scope_refused\"}}"); return; }
        };
        let id = match &p.context.correlation {
            Some(CorrelationId::Str(s)) => json!(["str",s]),
            Some(CorrelationId::Num(s)) => json!(["num",s]),
            None => Value::Null,
        };
        let request = p.context.request_metadata.as_ref().map(|m|
            format!("{:?}", conclude_request(&p.context.era,m,p.context.capability_observation.as_ref())));
        let conclusion = p.context.result_observation.as_ref().map(|r|
            format!("{:?}", conclude(&p.context.era,r,p.context.capability_observation.as_ref())));
        rows.push(json!({"seq":p.event.source_line,"id":id,"kind":kind,
            "request":request,"conclusion":conclusion,"error_response":p.is_error_response,"raw":raw}));
    }
    println!("{}",json!({"rows":rows}));
}
