use super::workbench_service::{AppendFeedbackRequest, ImportLocalFileRequest, WorkbenchService};
use assetweft_core::{
    application::Application,
    history::{Decision, HistoryQuery},
    library::{AssetVersion, ComponentReference, ComponentRole, Presence},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

const GLB: &[u8] =
    include_bytes!("../../../../crates/assetweft-core/tests/fixtures/validation/mesh_only.glb");
const PNG: &[u8] = include_bytes!("../icons/icon.png");
const WAV: &[u8] = b"RIFF\x28\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x40\x1f\0\0\x80\x3e\0\0\x02\0\x10\0data\x04\0\0\0\0\0\x01\0";

fn value(item: impl Serialize) -> Value {
    serde_json::to_value(item).unwrap()
}

fn import_request(path: &Path, version: &str, format: &str) -> ImportLocalFileRequest {
    serde_json::from_value(json!({
        "filePath": path, "assetId": "asset", "versionId": version, "sourceFormat": format,
        "licenseId": "UNKNOWN", "usageScope": "LOCAL",
    }))
    .unwrap()
}

fn feedback_request(id: &str, decision: &str, reason: &str) -> AppendFeedbackRequest {
    serde_json::from_value(json!({
        "assetId": "asset", "versionId": "mesh", "feedbackId": id, "decision": decision, "reason": reason,
    })).unwrap()
}

fn metadata() -> AssetVersion {
    AssetVersion {
        asset_id: "asset".into(),
        version_id: "metadata".into(),
        content_hash: "a".repeat(64),
        input_id: "fixture".into(),
        source: "TEST_FIXTURE".into(),
        authorization: "fixture".into(),
        authorized: false,
        license_id: "UNKNOWN".into(),
        usage_scope: "LOCAL".into(),
        source_format: "GLB".into(),
        contract_version: "1.0.0".into(),
        presence: Presence::Missing,
    }
}

fn proxy_cli(db: &Path, args: &[&str]) -> Option<Value> {
    let cli = std::env::var_os("AW_WORKBENCH_TEST_CLI")?;
    let output = Command::new(cli)
        .args(["catalog"])
        .args(args)
        .arg("--db")
        .arg(db)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).lines().count(), 1);
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["status"], "PASS");
    Some(result["results"].clone())
}

#[test]
fn commands_follow_catalog_storage_validation_and_history() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("catalog.sqlite");
    let mut service = WorkbenchService::default();
    assert_eq!(
        service.list_versions(None, 100).err().unwrap().code,
        "INPUT_INCOMPLETE"
    );
    assert!(service.open_library(&db, false).is_err());
    assert!(!db.exists());
    fs::write(&db, []).unwrap();
    assert_eq!(
        service.open_library(&db, false).err().unwrap().code,
        "INPUT_INCOMPLETE"
    );
    let session = value(service.open_library(&db, true).unwrap());
    assert_eq!(session["mode"], "LOCAL_SQLITE_NO_LOGIN");
    assert_eq!(session["schemaVersion"], 6);
    let caps = session["capabilities"].as_array().unwrap();
    assert_eq!(caps.len(), 8);
    assert_eq!(caps[5]["status"], "NOT_RUN");
    assert_eq!(caps[7]["status"], "DISABLED");
    assert_eq!(
        value(service.list_versions(None, 100).unwrap()),
        json!({"matches":[],"truncated":false})
    );

    for (id, format, bytes) in [
        ("mesh", "GLTF_GLB", GLB),
        ("image", "PNG", PNG),
        ("sound", "WAV", WAV),
        ("other", "OBJ", b"v 0 0 0"),
    ] {
        let file = dir.path().join(id);
        fs::write(&file, bytes).unwrap();
        let receipt = value(
            service
                .import_local_file(import_request(&file, id, format))
                .unwrap(),
        );
        assert_eq!(receipt["versionId"], id);
        assert_eq!(receipt["byteLength"], bytes.len());
    }
    let mut core = Application::open_existing(&db).unwrap();
    core.add_asset_version(&metadata()).unwrap();
    core.add_component_reference(&ComponentReference {
        owner_version_id: "mesh".into(),
        role: ComponentRole::Materials,
        component_version_id: "image".into(),
    })
    .unwrap();
    let ordered = value(service.list_versions(Some(" "), 100).unwrap());
    let rows = ordered["matches"].as_array().unwrap();
    let core_rows = core.list_versions(100).unwrap().matches;
    assert_eq!(rows.len(), core_rows.len());
    for (row, expected) in rows.iter().zip(core_rows) {
        assert_eq!(row["assetId"], expected.asset_id);
        assert_eq!(row["versionId"], expected.version_id);
        assert_eq!(row["sourceFormat"], expected.source_format);
    }
    assert_eq!(
        value(service.list_versions(Some(" me "), 1).unwrap())["truncated"],
        true
    );
    assert_eq!(
        value(service.list_versions(None, 1).unwrap())["truncated"],
        true
    );
    for limit in [0, 101] {
        assert!(service.list_versions(None, limit).is_err());
    }

    let mesh = core.asset_version("mesh").unwrap().unwrap();
    assert_eq!(mesh.source, "IMPORTED");
    assert_eq!(mesh.authorization, "local-user-import");
    assert!(!mesh.authorized);
    let detail = value(service.get_version_detail("mesh").unwrap());
    assert_eq!(detail["contentHash"], mesh.content_hash);
    assert_eq!(detail["availability"], "AVAILABLE");
    assert_eq!(detail["current"], Value::Null);
    assert_eq!(
        detail["references"],
        json!([{"role":"materials","componentVersionId":"image"}])
    );
    let glb = value(service.load_preview("mesh").unwrap());
    assert_eq!(glb["kind"], "GLB");
    assert_eq!(glb["validation"]["formatStatus"], "PASS");
    assert_eq!(glb["validation"]["fileSha256"], mesh.content_hash);
    assert_eq!(
        STANDARD.decode(glb["glbBase64"].as_str().unwrap()).unwrap(),
        GLB
    );
    let png = value(service.load_preview("image").unwrap());
    assert_eq!(png["previewKind"], "CORE_THUMBNAIL");
    assert!(
        png["dataUrl"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
    let wav = value(service.load_preview("sound").unwrap());
    assert_eq!(wav["kind"], "WAV");
    assert_eq!(
        STANDARD
            .decode(
                wav["dataUrl"]
                    .as_str()
                    .unwrap()
                    .strip_prefix("data:audio/wav;base64,")
                    .unwrap()
            )
            .unwrap(),
        WAV
    );
    assert_eq!(
        value(service.load_preview("other").unwrap())["kind"],
        "UNSUPPORTED"
    );
    let unavailable = value(service.load_preview("metadata").unwrap());
    assert_eq!(unavailable["kind"], "UNAVAILABLE");
    assert_eq!(unavailable["availability"], "METADATA_ONLY");
    assert!(service.validate_version("metadata", "static-mesh").is_err());
    assert!(service.load_preview("unknown").is_err());
    assert!(service.validate_version("mesh", "unknown").is_err());
    let validation = value(service.validate_version("mesh", "static-mesh").unwrap());
    assert_eq!(validation, glb["validation"]);
    let rigged = value(
        service
            .validate_version("mesh", "rigged-character")
            .unwrap(),
    );
    assert_eq!(rigged["formatStatus"], "PASS");
    assert_eq!(rigged["role"]["status"], "FAIL");

    assert!(
        service
            .append_feedback(feedback_request("", "ACCEPT", ""))
            .is_err()
    );
    assert!(
        service
            .append_feedback(feedback_request("x", "accept", ""))
            .is_err()
    );
    assert!(
        service
            .append_feedback(feedback_request("x", "ACCEPT", &"é".repeat(1025)))
            .is_err()
    );
    let first = value(
        service
            .append_feedback(feedback_request("first", "ACCEPT", "  usable  "))
            .unwrap(),
    );
    assert_eq!(first["reason"], "usable");
    let latest = value(
        service
            .append_feedback(feedback_request("second", "REJECT", ""))
            .unwrap(),
    );
    assert!(
        service
            .append_feedback(feedback_request("second", "ACCEPT", ""))
            .is_err()
    );
    let detail = value(service.get_version_detail("mesh").unwrap());
    assert_eq!(detail["history"], json!([latest, first]));
    assert_eq!(detail["latestFeedback"], latest);
    assert_eq!(
        value(service.list_versions(Some("mesh"), 100).unwrap())["matches"][0]["latestDecision"],
        "REJECT"
    );

    if let Some(list) = proxy_cli(&db, &["list"]) {
        assert_eq!(
            list["matches"],
            value(core.list_versions(100).unwrap().matches)
        );
        assert_eq!(list["matches"].as_array().unwrap().len(), rows.len());
        let feedback = proxy_cli(
            &db,
            &[
                "feedback-latest",
                "--asset-id",
                "asset",
                "--version-id",
                "mesh",
            ],
        )
        .unwrap();
        assert_eq!(feedback["feedback"]["sequence"], latest["sequence"]);
        assert_eq!(feedback["feedback"]["decision"], latest["decision"]);
    }

    service.open_library(&db, false).unwrap();
    let mut reopened = Application::open_read_only(&db).unwrap();
    let history = reopened
        .feedback_history(HistoryQuery {
            asset_id: "asset",
            version_id: Some("mesh"),
            before_sequence: None,
            limit: 20,
        })
        .unwrap();
    assert_eq!(history[0].sequence, latest["sequence"].as_i64().unwrap());
    assert_eq!(history[0].feedback.decision, Decision::Reject);
    assert_eq!(
        value(service.get_version_detail("mesh").unwrap())["latestFeedback"],
        latest
    );

    let managed = dir.path().join("objects").join(mesh.content_hash);
    fs::write(&managed, b"corrupt").unwrap();
    let corrupt = value(service.load_preview("mesh").unwrap());
    assert_eq!(corrupt["kind"], "UNAVAILABLE");
    assert_eq!(corrupt["availability"], "CORRUPT");
    fs::remove_file(managed).unwrap();
    assert_eq!(
        value(service.load_preview("mesh").unwrap())["availability"],
        "MISSING"
    );
}

#[test]
fn local_input_rejections_preserve_the_open_library() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("catalog.sqlite");
    let mut service = WorkbenchService::default();
    service.open_library(&db, true).unwrap();
    assert!(
        service
            .open_library(&dir.path().join("missing.sqlite"), false)
            .is_err()
    );
    let empty = dir.path().join("empty.glb");
    fs::write(&empty, []).unwrap();
    let large = dir.path().join("large.glb");
    fs::File::create(&large)
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();
    for path in [&empty, &large, dir.path()] {
        assert_eq!(
            service
                .import_local_file(import_request(path, "bad", "GLB"))
                .err()
                .unwrap()
                .code,
            "INVALID_INPUT"
        );
    }

    #[cfg(unix)]
    {
        let linked = dir.path().join("linked");
        std::os::unix::fs::symlink(&db, &linked).unwrap();
        assert_eq!(
            service.open_library(&linked, true).err().unwrap().code,
            "INVALID_INPUT"
        );
        assert_eq!(
            service
                .import_local_file(import_request(&linked, "bad", "GLB"))
                .err()
                .unwrap()
                .code,
            "INVALID_INPUT"
        );
    }
    assert_eq!(
        value(service.list_versions(None, 100).unwrap()),
        json!({"matches":[],"truncated":false})
    );
}
