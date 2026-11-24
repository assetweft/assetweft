use super::assurance::{self, CheckStatus, ValidationLimits};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub(super) fn hash(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect()
}
pub(super) fn check_rig(bytes: &[u8]) -> Result<assurance::RigReport> {
    let r = assurance::checks::inspect_rig_bytes(bytes);
    if r.validation.format_status != CheckStatus::Pass
        || r.bound_nodes.len() != 1
        || r.checks.iter().any(|c| c.status != CheckStatus::Pass)
    {
        return Err(format!("input rig gate rejected/NOT_RUN: {:?}", r.checks).into());
    }
    Ok(r)
}

pub(super) fn evaluate(
    binary: &Path,
    bytes: &[u8],
    out: &Path,
    script_bytes: &[u8],
    source: &str,
) -> Result<(Value, Value)> {
    if !binary.is_absolute() {
        return Err("Blender binary path must be explicit and absolute".into());
    }
    fs::create_dir(out)?;
    let out = out.canonicalize()?;
    fs::write(out.join("input.glb"), bytes)?;
    let temp = tempfile::tempdir_in(&out)?;
    let script = temp.path().join("rig_blender.py");
    fs::write(&script, script_bytes)?;
    fs::write(
        out.join("started.json"),
        serde_json::to_vec_pretty(
            &json!({"binary":binary,"arguments":["--background","--factory-startup","--disable-autoexec","--python-exit-code","7","--python","FIXED_EMBEDDED_SCRIPT","--",out],"input_sha256":hash(bytes),"script_sha256":hash(script_bytes),"timeout_seconds":100,"wake":"synchronous bounded child wait","raw_logs":["blender.stdout.log","blender.stderr.log"]}),
        )?,
    )?;
    let log = fs::File::create(out.join("blender.stdout.log"))?;
    let err = fs::File::create(out.join("blender.stderr.log"))?;
    let mut child = Command::new(binary)
        .args([
            "--background",
            "--factory-startup",
            "--disable-autoexec",
            "--python-exit-code",
            "7",
            "--python",
        ])
        .arg(&script)
        .arg("--")
        .arg(&out)
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(err)
        .spawn()?;
    let start = Instant::now();
    let mut timeout = false;
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if start.elapsed() > Duration::from_secs(100) {
            timeout = true;
            child.kill()?;
            break child.wait()?;
        }
        thread::sleep(Duration::from_millis(50));
    };
    let receipt = json!({"input_sha256":hash(bytes),"script_sha256":hash(script_bytes),"timeout_seconds":100,"timed_out":timeout,"raw_exit":status.code(),"success":status.success(),"source":source,"unknown_cost":null});
    fs::write(
        out.join("process.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    if !status.success() || timeout {
        return Err(format!("Blender failed: {receipt}").into());
    }
    let result = assurance::read_bounded(&out.join("result.json"), ValidationLimits::default())?;
    let result: Value = serde_json::from_slice(&result)?;
    Ok((result, receipt))
}
