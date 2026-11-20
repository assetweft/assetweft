//! Optional measurement on the pinned Cesium rig, using Assurance and local Blender.
//! Called by the CLI. Only the verified asset identity reaches the fixed native worker.
mod landmarks;
mod process;

use crate::assurance::{self, ValidationLimits};
use serde_json::{Value, json};
use std::path::Path;

pub(crate) const RIGGED_SIMPLE: &str =
    "3a79dabb67bb0cd598a18d08b954d9d357c27c30672f82ef5d3f4e7fe6ca3401";

/// Run one preset world-space bend on the exact licensed Cesium reference asset.
/// Requires a caller-selected trusted Blender 5.1.2 executable and a new evidence directory.
/// # Errors
/// Returns a diagnostic on input/profile mismatch, IO, process or measurement failure.
pub fn measure_rigged_simple(
    binary: &Path,
    input: &Path,
    out: &Path,
    limits: ValidationLimits,
) -> Result<Value, Box<dyn std::error::Error>> {
    let bytes = assurance::read_bounded(input, limits)?;
    if process::hash(&bytes) != RIGGED_SIMPLE {
        return Err("UNSUPPORTED Rigged Simple GLB SHA-256".into());
    }
    let r = process::check_rig(&bytes)?;
    if r.bound_nodes != [2] {
        return Err("Rigged Simple Cylinder nodes[2] binding missing".into());
    }
    let checks = r
        .checks
        .iter()
        .map(|c| json!({"id":c.id,"status":c.status.as_str(),"detail":c.detail}))
        .collect::<Vec<_>>();
    let (result, receipt) = process::evaluate(
        binary,
        &bytes,
        out,
        include_bytes!("rig_blender/rigged_simple.py"),
        "Cesium Rigged Simple",
    )?;
    landmarks::verify(&result)?;
    Ok(
        json!({"status":"MEASURED","profile":"cesium-rigged-simple-v1","checks":checks,
              "process":receipt,"result":result,"not_run":["F0","animations","visual","engine","wholeRigB"]}),
    )
}
