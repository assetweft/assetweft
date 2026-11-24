use assetweft_core::assurance::{self, CheckStatus, ValidationLimits};
use serde_json::{Value, json};
#[path = "../src/assurance/rig_blender/process.rs"]
mod process;
use std::{
    env,
    io::{self, Write},
    path::Path,
};
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
    process::check_rig(&bytes)?;
    let g = gltf::Gltf::from_slice(&bytes)?;
    let container = gltf::binary::Glb::from_slice(&bytes)?;
    let j: Value = serde_json::from_slice(&container.json)?;
    reject_external(&j, "")?;
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
    if j["asset"]["generator"] != "AssetWeft TEST_FIXTURE pose v1" || names != ["root", "bend"] {
        return Err("UNSUPPORTED fixture declaration or identity mapping".into());
    }
    let out = Path::new(&a[2]);
    let (result, receipt) = process::evaluate(
        binary,
        &bytes,
        out,
        include_bytes!("rig_blender/bridge.py"),
        "TEST_FIXTURE",
    )?;
    let output = assurance::read_bounded(&out.join("roundtrip.glb"), ValidationLimits::default())?;
    let rr = assurance::checks::inspect_rig_bytes(&output);
    if rr.validation.format_status != CheckStatus::Pass
        || rr.bound_nodes.len() != 1
        || rr.checks.iter().any(|c| c.status != CheckStatus::Pass)
    {
        return Err(format!("output rig rejected: {:?}", rr.checks).into());
    }
    Ok(
        json!({"status":"MEASURED","process":receipt,"output_sha256":process::hash(&output),"result":result,"not_run":["F0","AI","renderer","Windows","Unity","Unreal","Godot","wholeRigB"]}),
    )
}
fn main() {
    match run() {
        Ok(r) => println!("{r}"),
        Err(e) => {
            let _ = writeln!(io::stderr().lock(), "{e}");
            println!(
                "{}",
                json!({"status":if e.to_string().starts_with("UNSUPPORTED") { "NOT_RUN" } else { "FAIL" },"detail":e.to_string()})
            );
            std::process::exit(1);
        }
    }
}
