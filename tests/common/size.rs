// SPDX-License-Identifier: MIT OR Apache-2.0

use serde_json::Value;

pub struct SizeCtx {
    pub event_times: Option<usize>,
    pub values: Option<usize>,
    pub data: Option<usize>,
}

pub fn parse_size_contract(expr: &Value, ctx: &SizeCtx) -> usize {
    if let Some(n) = expr.as_u64() {
        return n as usize;
    }
    if let Some(n) = expr.as_i64() {
        return n as usize;
    }
    match expr.as_str() {
        Some("event_times.len()") => ctx
            .event_times
            .unwrap_or_else(|| panic!("size contract event_times.len() needs event_times len")),
        Some("values.len() - 1") => {
            let n = ctx
                .values
                .unwrap_or_else(|| panic!("size contract values.len()-1 needs values len"));
            n.saturating_sub(1)
        }
        Some("data.len()") => ctx
            .data
            .unwrap_or_else(|| panic!("size contract data.len() needs data len")),
        other => panic!("unsupported size contract: {other:?}"),
    }
}
