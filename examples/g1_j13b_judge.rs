//! Fail-closed J13b child. No runtime adapter means no workload is entered.
#[path = "_shared/j13b_workload.rs"]
#[allow(dead_code)]
mod workload;
use std::{env, process::ExitCode};

const ARMS: [&str; 9] = ["d", "n", "f", "b", "a", "ab", "base", "base2", "mi"];
const CELLS: [&str; 13] = [
    "cycle-1m",
    "cycle-128m",
    "cycle-1g",
    "grow-1g",
    "many-128x16m",
    "sparse-128x16m",
    "stride-64x16m",
    "tail-32x16m",
    "edge-128m",
    "phased",
    "mixed",
    "churn-4x1024",
    "churn-small-4x4096",
];

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() != 2 && args.len() != 4 {
        return usage();
    }
    let cell = &args[0];
    let arm = &args[1];
    let (block, repeat) = if args.len() == 4 {
        let Ok(b) = args[2].parse::<u32>() else {
            emit(cell, arm, None, None, "invalid_request", "invalid_block");
            return ExitCode::from(2);
        };
        let Ok(r) = args[3].parse::<u32>() else {
            emit(
                cell,
                arm,
                Some(b),
                None,
                "invalid_request",
                "invalid_repeat",
            );
            return ExitCode::from(2);
        };
        if b >= 12 || r >= 2 {
            emit(
                cell,
                arm,
                Some(b),
                Some(r),
                "invalid_request",
                "out_of_range",
            );
            return ExitCode::from(2);
        }
        (Some(b), Some(r))
    } else {
        (None, None)
    };
    if !CELLS.contains(&cell.as_str()) {
        emit(cell, arm, block, repeat, "invalid_request", "unknown_cell");
        return ExitCode::from(2);
    }
    if !ARMS.contains(&arm.as_str()) {
        emit(cell, arm, block, repeat, "invalid_request", "unknown_arm");
        return ExitCode::from(2);
    }
    emit(
        cell,
        arm,
        block,
        repeat,
        "unavailable",
        "runtime_adapter_missing",
    );
    ExitCode::from(3)
}
fn usage() -> ExitCode {
    eprintln!("usage: g1_j13b_judge <cell_id> <arm> [block repeat]");
    ExitCode::from(2)
}
fn emit(cell: &str, arm: &str, block: Option<u32>, repeat: Option<u32>, status: &str, error: &str) {
    println!("{{\"schema\":\"j13b.raw.v4\",\"evidence_class\":\"runtime\",\"status\":{},\"cell_id\":{},\"arm\":{},\"block\":{},\"repeat\":{},\"workload_executed\":false,\"error\":{},\"adapter\":null,\"identity\":null,\"requested_tuple\":null,\"resolved_tuple\":null,\"tuple_conflict_delta\":null,\"mechanism\":null,\"sample\":null}}",json(status),json(cell),json(arm),block.map_or("null".to_owned(),|x|x.to_string()),repeat.map_or("null".to_owned(),|x|x.to_string()),json(error));
}
fn json(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if c <= '\u{1f}' => {
                use std::fmt::Write as _;
                let _ = write!(o, "\\u{:04x}", c as u32);
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}
