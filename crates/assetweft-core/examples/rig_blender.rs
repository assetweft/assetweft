//! Responsibility: optional, bounded Blender 5.1.2 GLB skin roundtrip tool.
//! Dependencies: Assurance rig checks, gltf/serde/sha2/tempfile, external explicit Blender.
//! Callers: opt-in local M3 fixture checks. No Python runtime required by core.
//! Invariants: fixed embedded script; no shell/foreign script; new output directory;
//! immutable staged input, no URI/texture/extension/animation/morph processing.
use assetweft_core::assurance::{self, CheckStatus, ValidationLimits};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
fn hash(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect()
}
fn reject_external(v: &Value, path: &str) -> Result<(), String> {
    match v {
        Value::Object(map) => {
            for (k, value) in map {
                let loc = format!("{path}/{k}");
                if k == "uri" || k == "extensions" {
                    return Err(format!("UNSUPPORTED {loc}"));
                }
                reject_external(value, &loc)?;
            }
        }
        Value::Array(items) => {
            for (i, value) in items.iter().enumerate() {
                reject_external(value, &format!("{path}/{i}"))?;
            }
        }
        _ => (),
    }
    Ok(())
}
fn run() -> Result<Value, Box<dyn std::error::Error>> {
    let a: Vec<_> = env::args_os().skip(1).collect();
    if a.len() != 3 {
        return Err("usage: rig_blender ABSOLUTE_BLENDER INPUT.glb NEW_OUTPUT_DIR".into());
    }
    let binary = Path::new(&a[0]);
    if !binary.is_absolute() {
        return Err("Blender binary path must be explicit and absolute".into());
    }
    let bytes = assurance::read_bounded(Path::new(&a[1]), ValidationLimits::default())?;
    let r = assurance::checks::inspect_rig_bytes(&bytes);
    if r.validation.format_status != CheckStatus::Pass
        || r.bound_nodes.len() != 1
        || r.checks.iter().any(|c| c.status != CheckStatus::Pass)
    {
        return Err(format!("input rig gate rejected/NOT_RUN: {:?}", r.checks).into());
    }
    let g = gltf::Gltf::from_slice(&bytes)?;
    let container = gltf::binary::Glb::from_slice(&bytes)?;
    let j: Value = serde_json::from_slice(&container.json)?;
    reject_external(&j, "")?;
    // Deliberately narrow supported slice; never ask Blender to repair rejected data.
    for key in [
        "animations",
        "textures",
        "images",
        "materials",
        "extensionsUsed",
        "extensionsRequired",
    ] {
        if j.get(key)
            .and_then(Value::as_array)
            .is_some_and(|v| !v.is_empty())
        {
            return Err(format!("UNSUPPORTED /{key}").into());
        }
    }
    if g.nodes().len() != 3
        || g.buffers().len() != 1
        || g.meshes().next().is_none_or(|m| m.primitives().len() != 1)
    {
        return Err(
            "UNSUPPORTED /nodes /buffers /meshes/0/primitives: bounded proof topology".into(),
        );
    }
    if g.skins().len() != 1 || g.meshes().len() != 1 || g.scenes().len() != 1 {
        return Err("UNSUPPORTED /skins /meshes /scenes: require one each".into());
    }
    let skin = g.skins().next().ok_or("missing skin")?;
    let mut names = Vec::new();
    for n in skin.joints() {
        let name = j["nodes"][n.index()]["name"]
            .as_str()
            .ok_or("UNSUPPORTED joint missing explicit name")?;
        if name.is_empty() || names.contains(&name) {
            return Err("UNSUPPORTED duplicate/empty joint name".into());
        }
        names.push(name);
    }
    for p in g.meshes().next().ok_or("missing mesh")?.primitives() {
        if p.morph_targets().len() != 0 || p.mode() != gltf::mesh::Mode::Triangles {
            return Err("UNSUPPORTED /meshes/0/primitives: morph or non-triangles".into());
        }
    }
    // Fixed declaration intentionally admits only this programmatic proof, not F0.
    if j["asset"]["generator"] != "AssetWeft TEST_FIXTURE pose v1" || names != ["root", "bend"] {
        return Err("UNSUPPORTED fixture declaration or identity mapping".into());
    }
    let out = Path::new(&a[2]);
    fs::create_dir(out)?;
    let out = out.canonicalize()?;
    fs::write(out.join("input.glb"), &bytes)?;
    let temp = tempfile::tempdir()?;
    let script = temp.path().join("rig_blender.py");
    fs::write(&script, include_bytes!("rig_blender/bridge.py"))?;
    fs::write(
        out.join("started.json"),
        serde_json::to_vec_pretty(
            &json!({"binary":binary,"arguments":["--background","--factory-startup","--disable-autoexec","--python-exit-code","7","--python","FIXED_EMBEDDED_SCRIPT","--",out],"input_sha256":hash(&bytes),"script_sha256":hash(include_bytes!("rig_blender/bridge.py")),"timeout_seconds":100,"wake":"synchronous bounded child wait","raw_logs":["blender.stdout.log","blender.stderr.log"]}),
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
    let receipt = json!({"input_sha256":hash(&bytes),"script_sha256":hash(include_bytes!("rig_blender/bridge.py")),"timeout_seconds":100,"timed_out":timeout,"raw_exit":status.code(),"success":status.success(),"source":"TEST_FIXTURE","unknown_cost":null});
    fs::write(
        out.join("process.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    if !status.success() || timeout {
        return Err(format!("Blender failed: {receipt}").into());
    }
    let result = assurance::read_bounded(&out.join("result.json"), ValidationLimits::default())?;
    let result: Value = serde_json::from_slice(&result)?;
    let output = assurance::read_bounded(&out.join("roundtrip.glb"), ValidationLimits::default())?;
    let rr = assurance::checks::inspect_rig_bytes(&output);
    if rr.validation.format_status != CheckStatus::Pass
        || rr.bound_nodes.len() != 1
        || rr.checks.iter().any(|c| c.status != CheckStatus::Pass)
    {
        return Err(format!("output rig rejected: {:?}", rr.checks).into());
    }
    Ok(
        json!({"status":"MEASURED","process":receipt,"output_sha256":hash(&output),"result":result,"not_run":["F0","AI","renderer","Windows","Unity","Unreal","Godot","wholeRigB"]}),
    )
}
fn main() {
    match run() {
        Ok(r) => println!("{r}"),
        Err(e) => {
            eprintln!("{e}");
            println!(
                "{}",
                json!({"status":if e.to_string().starts_with("UNSUPPORTED") { "NOT_RUN" } else { "FAIL" },"detail":e.to_string()})
            );
            std::process::exit(1);
        }
    }
}
