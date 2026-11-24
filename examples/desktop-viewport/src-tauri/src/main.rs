use assetweft_core::{
    application::Application,
    catalog::domain::{AssetVersion, Presence},
    validation::{CheckStatus, RoleRequirement, SourceClass, SourceDeclaration, ValidationLimits},
};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
};
const FIXTURE: &[u8] = include_bytes!("../../../godot-static/triangle.glb");
const HASH: &str = "44e32648c9f09f1ee221b89b7da61dd29aecf4227cf6c704dd73ca3ab89c93c1";
struct Owned {
    app: Mutex<Application>,
    root: PathBuf,
    evidence: Mutex<Evidence>,
}
#[tauri::command]
fn load_fixture(
    case: &str,
    generation: u64,
    state: tauri::State<'_, Owned>,
) -> Result<tauri::ipc::Response, String> {
    if !matches!(case, "fixture" | "empty" | "malformed") {
        return Err("fixed cases only".into());
    }
    if !state
        .evidence
        .lock()
        .map_err(|e| e.to_string())?
        .begin(&state.root, generation)
        .map_err(|e| e.to_string())?
    {
        return Err("superseded generation".into());
    }
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
    let mut evidence = state.evidence.lock().map_err(|e| e.to_string())?;
    if evidence.generation != generation {
        return Err("superseded generation".into());
    }
    evidence.admitted = case == "fixture";
    Ok(tauri::ipc::Response::new(bytes))
}
#[tauri::command]
fn save_evidence(
    generation: u64,
    json: String,
    png: Vec<u8>,
    state: tauri::State<'_, Owned>,
) -> Result<BTreeMap<&'static str, &'static str>, String> {
    let status = state
        .evidence
        .lock()
        .map_err(|e| e.to_string())?
        .save(&state.root, generation, &json, &png)
        .map_err(|e| e.to_string())?;
    Ok(BTreeMap::from([("status", status)]))
}
#[derive(Default)]
struct Evidence {
    generation: u64,
    admitted: bool,
}

impl Evidence {
    fn begin(&mut self, root: &Path, generation: u64) -> io::Result<bool> {
        if generation <= self.generation {
            return Ok(false);
        }
        let retired = root.join(format!(".evidence-retired-{}", self.generation));
        self.generation = generation;
        self.admitted = false;
        match fs::rename(root.join("evidence"), &retired) {
            Ok(()) => fs::remove_dir_all(retired)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
        Ok(true)
    }

    fn save(
        &self,
        root: &Path,
        generation: u64,
        json: &str,
        png: &[u8],
    ) -> io::Result<&'static str> {
        let pair = root.join("evidence");
        if generation != self.generation || !self.admitted || pair.try_exists()? {
            return Ok("stale-rejected");
        }
        if json.len() > 65_536 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "observation too large",
            ));
        }
        if png.len() > 4_000_000 || !png.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "bounded PNG required",
            ));
        }

        // #101: publish both files through one directory rename while the generation is locked.
        let staging = root.join(format!(".evidence-staging-{generation}"));
        fs::create_dir(&staging)?;
        let published = fs::write(staging.join("renderer-observations.json"), json)
            .and_then(|()| fs::write(staging.join("render.png"), png))
            .and_then(|()| fs::rename(&staging, pair));
        if published.is_err() {
            fs::remove_dir_all(staging)?;
        }
        published?;
        Ok("saved")
    }
}

#[cfg(test)]
#[path = "evidence_tests.rs"]
mod evidence_tests;

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
            evidence: Mutex::default(),
        })
        .invoke_handler(tauri::generate_handler![load_fixture, save_evidence])
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
