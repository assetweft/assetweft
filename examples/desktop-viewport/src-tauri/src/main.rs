//! Responsibility: fixed-fixture incoming desktop adapter and evidence sink.
//! Dependencies: existing Core use cases, Tauri; caller: one local WebView.
//! Invariants: no user paths/network/SQL/plugins; controls are owned; Core owns validation/CAS.
use assetweft_core::{
    application::Application,
    catalog::domain::{AssetVersion, Presence},
    validation::{CheckStatus, RoleRequirement, SourceClass, SourceDeclaration, ValidationLimits},
};
use std::{path::PathBuf, sync::Mutex};
const FIXTURE: &[u8] = include_bytes!("../../../godot-static/triangle.glb");
const HASH: &str = "44e32648c9f09f1ee221b89b7da61dd29aecf4227cf6c704dd73ca3ab89c93c1";
struct Owned {
    app: Mutex<Application>,
    root: PathBuf,
}
#[tauri::command]
fn load_fixture(
    case: &str,
    state: tauri::State<'_, Owned>,
) -> Result<tauri::ipc::Response, String> {
    let mut app = state.app.lock().map_err(|e| e.to_string())?;
    let path = match case {
        "fixture" => {
            let object = app
                .managed_object("technical-static")
                .map_err(|e| e.to_string())?
                .ok_or("managed object missing")?;
            if object.content_hash != HASH || object.byte_length != 804 {
                return Err("immutable identity mismatch".into());
            }
            state.root.join("objects").join(object.content_hash)
        }
        "empty" => state.root.join("empty.glb"),
        "malformed" => state.root.join("malformed.glb"),
        _ => return Err("fixed cases only".into()),
    };
    let mut declaration = SourceDeclaration::declared(SourceClass::TestFixture);
    declaration.declared_license = Some("CC0-1.0 (caller declaration; unverified)".into());
    let report = app
        .validate_file(
            &path,
            RoleRequirement::StaticMesh,
            declaration,
            ValidationLimits { max_bytes: 1024 },
        )
        .map_err(|e| e.to_string())?;
    std::fs::write(
        state.root.join(format!("{case}-core.txt")),
        format!("{report:#?}"),
    )
    .map_err(|e| e.to_string())?;
    if report.format_status != CheckStatus::Pass || report.role.satisfied != Some(true) {
        return Err(format!(
            "Core rejected {case}: format={}, role={}; no rendered PASS",
            report.format_status.as_str(),
            report.role.status.as_str()
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    Ok(tauri::ipc::Response::new(bytes))
}
#[tauri::command]
fn capture(request: tauri::ipc::Request<'_>, state: tauri::State<'_, Owned>) -> Result<(), String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("binary PNG required".into());
    };
    if bytes.len() > 4_000_000 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("bounded PNG required".into());
    }
    std::fs::write(state.root.join("render.png"), bytes).map_err(|e| e.to_string())
}
#[tauri::command]
fn observations(json: String, state: tauri::State<'_, Owned>) -> Result<(), String> {
    if json.len() > 65536 {
        return Err("observation too large".into());
    }
    std::fs::write(state.root.join("renderer-observations.json"), json).map_err(|e| e.to_string())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var("AW_VIEWPORT_RUN")?);
    // Explicit bootstrap into a new caller-owned directory, never a user database.
    std::fs::create_dir(&root)?;
    let mut app = Application::open(&root.join("technical.sqlite"))?;
    app.initialize_library()?;
    let version = AssetVersion {
        asset_id: "technical".into(),
        version_id: "technical-static".into(),
        content_hash: HASH.into(),
        input_id: "AW031-analytic".into(),
        source: "TEST_FIXTURE".into(),
        authorization: "fixture.json CC0 declaration".into(),
        authorized: false,
        license_id: "CC0-1.0".into(),
        usage_scope: "local technical smoke".into(),
        source_format: "GLB".into(),
        contract_version: "1.0.0".into(),
        presence: Presence::Unknown,
    };
    app.commit_object(&version, &mut &FIXTURE[..], 804, 1024)?;
    std::fs::write(root.join("empty.glb"), [])?;
    std::fs::write(root.join("malformed.glb"), b"owned malformed control")?;
    let data = root.join("webview-data");
    std::fs::create_dir(&data)?;
    tauri::Builder::default()
        .manage(Owned {
            app: Mutex::new(app),
            root,
        })
        .invoke_handler(tauri::generate_handler![
            load_fixture,
            capture,
            observations
        ])
        .setup(move |app| {
            tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("AW005 Static Mesh Experiment")
            .inner_size(824., 780.)
            .resizable(false)
            .data_directory(data.clone())
            .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())?;
    Ok(())
}
