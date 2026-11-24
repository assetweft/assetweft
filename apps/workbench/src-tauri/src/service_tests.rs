use super::workbench_service::{AppendFeedbackRequest, ImportLocalFileRequest, WorkbenchService};
use assetweft_core::{
    application::Application,
    history::{Decision, HistoryQuery},
    library::{AssetVersion, ComponentReference, ComponentRole, Presence},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::Digest;
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
    assert_eq!(caps[5]["status"], "READY");
    assert_eq!(caps[6]["status"], "NOT_RUN");
    assert_eq!(caps[7]["status"], "DISABLED");
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

fn task_allowance(id: &str, capacity: u64) -> super::workbench_service::ApproveBudgetRequest {
    serde_json::from_value(json!({
        "id": id, "capacity": capacity, "unit": "Task", "approval": "ExplicitBounded",
        "source": "local fixture allowance",
    }))
    .unwrap()
}

#[test]
fn budget_commands_preserve_uncertainty_and_receipts() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("catalog.sqlite");
    let mut service = WorkbenchService::default();
    assert!(service.budget_overview(100).is_err());
    service.open_library(&db, true).unwrap();
    assert!(service.approve_budget(task_allowance("", 2)).is_err());
    let account = value(service.approve_budget(task_allowance("task", 2)).unwrap());
    assert_eq!(
        account,
        json!({
            "id": "task", "unit": {"label": "Task"}, "approval": "EXPLICIT_BOUNDED",
            "source": "local fixture allowance", "capacity": 2, "reserved": 0, "spent": 0, "available": 2,
        })
    );
    let overview = value(service.budget_overview(100).unwrap());
    assert_eq!(overview["accounts"], json!([account]));
    assert_eq!(overview["profiles"][0]["id"], "hf-zerogpu");
    let request = serde_json::from_value(json!({
        "id": "intent", "account": "task", "unit": "Task", "quote": null,
        "reservation": 1, "inputFingerprint": "fixture", "source": "local fixture",
    }))
    .unwrap();
    let reserved = value(service.reserve_call(request).unwrap());
    assert_eq!(reserved["state"], "PREPARING");
    assert!(reserved["quote"].is_null());
    let mut core = Application::open_existing(&db).unwrap();
    assert!(core.submit_test_fixture("intent", || {}).unwrap());
    assert_eq!(
        core.call_intent("intent").unwrap().unwrap().state,
        assetweft_core::budget::State::Unknown
    );
    let overview = value(service.budget_overview(100).unwrap());
    assert_eq!(overview["intents"][0]["state"], "UNKNOWN");
    assert!(
        overview["intents"][0]["warning"]
            .as_str()
            .unwrap()
            .contains("do NOT duplicate-submit")
    );
    assert!(overview["intents"][0]["charge"].is_null());

    let cancelled = value(service.cancel_call("intent").unwrap());
    assert_eq!(cancelled["cancelled"], true);
    assert_eq!(cancelled["state"], "UNKNOWN");
    assert!(
        cancelled["warning"]
            .as_str()
            .unwrap()
            .contains("Local cancel recorded; remote stop or refund is NOT claimed.")
    );
    assert_eq!(
        value(service.budget_overview(100).unwrap())["accounts"][0]["reserved"],
        1
    );

    for (receipt_id, resolution, state) in [
        ("unresolved", json!("Unresolved"), "UNKNOWN"),
        (
            "confirmed",
            json!({"Confirmed": {"charge": 1}}),
            "CONFIRMED",
        ),
    ] {
        let request = serde_json::from_value(json!({
            "intentId": "intent", "receiptId": receipt_id, "source": "local fixture query",
            "resolution": resolution,
        }))
        .unwrap();
        let intent = value(service.reconcile_call(request).unwrap());
        assert_eq!(intent["state"], state);
    }
    let overview = value(service.budget_overview(100).unwrap());
    assert_eq!(overview["accounts"][0]["available"], 1);
    assert_eq!(overview["accounts"][0]["reserved"], 0);
    assert_eq!(overview["accounts"][0]["spent"], 1);
    assert_eq!(overview["intents"][0]["charge"], 1);
    assert_eq!(
        overview["intents"][0]["receipts"].as_array().unwrap().len(),
        2
    );
    let zero_cost = serde_json::from_value(json!({
        "id": "currency", "capacity": 1, "unit": {"CurrencyMinor": {"currency": "USD", "decimals": 2}},
        "approval": "ZeroAdditionalCost", "source": "fixture",
    })).unwrap();
    assert!(service.approve_budget(zero_cost).is_err());
    let large = value(
        service
            .approve_budget(task_allowance("large", i64::MAX as u64))
            .unwrap(),
    );
    assert_eq!(large["available"], i64::MAX.to_string());
}

#[cfg(unix)]
#[test]
fn byo_gpu_refuses_upload_and_exhaustion_then_replays_without_invoking() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("catalog.sqlite");
    let image = dir.path().join("image.png");
    let adapter = dir.path().join("adapter.sh");
    let counter = dir.path().join("calls");
    fs::write(&image, PNG).unwrap();
    fs::write(&adapter, "#!/bin/sh\nset -eu\nprintf x >> \"$(dirname \"$0\")/calls\"\nwhile [ \"$1\" != --out-dir ]; do shift; done\nprintf 'local fixture' > \"$2/artifact.glb\"\n").unwrap();
    fs::set_permissions(&adapter, fs::Permissions::from_mode(0o700)).unwrap();
    let request = |budget, intent, out_dir, upload| {
        serde_json::from_value(json!({
            "budgetId": budget, "intentId": intent, "profile": "hf-zerogpu", "model": "trellis2",
            "outDir": out_dir, "image": image, "adapterBin": adapter,
            "allowRemoteUpload": upload, "maxBytes": 1048576,
        }))
        .unwrap()
    };
    let mut service = WorkbenchService::default();
    service.open_library(&db, true).unwrap();
    service.approve_budget(task_allowance("task", 1)).unwrap();
    service.approve_budget(task_allowance("spent", 0)).unwrap();
    let before = value(service.budget_overview(100).unwrap());
    let out = dir.path().join("out");
    let error = service
        .run_byo_gpu(request("task", "run", &out, false))
        .err()
        .unwrap();
    assert!(error.message.contains("requires --allow-remote-upload"));
    assert!(
        service
            .run_byo_gpu(request("spent", "denied", &out, true))
            .is_err()
    );
    assert!(!counter.exists());
    assert_eq!(value(service.budget_overview(100).unwrap()), before);

    let run = value(
        service
            .run_byo_gpu(request("task", "run", &out, true))
            .unwrap(),
    );
    assert_eq!(run["status"], "SUCCEEDED");
    assert_eq!(run["invoked"], true);
    assert!(run["cashCost"].is_null());
    assert_eq!(
        fs::read(out.join("artifact.glb")).unwrap(),
        b"local fixture"
    );
    assert_eq!(run["artifactSha256"], sha256(b"local fixture"));
    let replay = value(
        service
            .run_byo_gpu(request("task", "run", &out, true))
            .unwrap(),
    );
    assert_eq!(replay["invoked"], false);
    assert_eq!(replay["artifactSha256"], run["artifactSha256"]);
    assert_eq!(fs::read(&counter).unwrap(), b"x");
}

#[test]
fn candidate_commands_preserve_stale_branch_and_feedback_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("catalog.sqlite");
    let mut service = WorkbenchService::default();
    service.open_library(&db, true).unwrap();
    let mut core = Application::open_existing(&db).unwrap();
    for (asset_id, version_id) in [
        ("asset", "base"),
        ("asset", "next"),
        ("asset", "stale"),
        ("part", "base-part"),
        ("part", "next-part"),
        ("part", "stale-part"),
        ("other-asset", "lock-base"),
        ("other-asset", "lock-next"),
    ] {
        core.commit_object(
            &AssetVersion {
                asset_id: asset_id.into(),
                version_id: version_id.into(),
                content_hash: sha256(GLB),
                presence: Presence::Unknown,
                ..metadata()
            },
            &mut &GLB[..],
            GLB.len() as u64,
            1048576,
        )
        .unwrap();
    }
    for (owner, part) in [
        ("base", "base-part"),
        ("next", "next-part"),
        ("stale", "stale-part"),
    ] {
        core.add_component_reference(&ComponentReference {
            owner_version_id: owner.into(),
            component_version_id: part.into(),
            role: ComponentRole::Mesh,
        })
        .unwrap();
    }

    for (owner, mesh) in [("lock-base", "base-part"), ("lock-next", "next-part")] {
        for (role, part) in [
            (ComponentRole::Mesh, mesh),
            (ComponentRole::Rig, "stale-part"),
        ] {
            core.add_component_reference(&ComponentReference {
                owner_version_id: owner.into(),
                component_version_id: part.into(),
                role,
            })
            .unwrap();
        }
    }
    let locked = serde_json::from_value(json!({
        "id": "locked", "baseVersion": "lock-base", "candidateVersion": "lock-next",
        "expectedRevision": 0, "editable": ["rig"], "lockMode": "BYTE",
        "maxComponentBytes": 1048576, "requiredQuality": [],
    }))
    .unwrap();
    assert!(
        service
            .propose_candidate(locked)
            .err()
            .unwrap()
            .message
            .contains("locked component changed")
    );
    assert!(core.candidate("locked").unwrap().is_none());
    let initial = serde_json::from_value(json!({
        "versionId": "base", "maxComponentBytes": 1048576, "requiredQuality": [],
    }))
    .unwrap();
    let head = value(service.initialize_candidate_head(initial).unwrap());
    assert_eq!(head["revision"], 0);
    assert_eq!(head["versionId"], "base");
    for (id, version, outcome) in [("a", "next", "ACCEPTED"), ("b", "stale", "CONFLICT")] {
        let proposal = serde_json::from_value(json!({
            "id": id, "baseVersion": "base", "candidateVersion": version,
            "expectedRevision": 0, "editable": ["mesh"], "lockMode": "BYTE",
            "maxComponentBytes": 1048576, "requiredQuality": [],
        }))
        .unwrap();
        let candidate = value(service.propose_candidate(proposal).unwrap());
        assert_eq!(candidate["editable"], json!(["mesh"]));
        assert_eq!(candidate["lockMode"], "BYTE");
        assert!(candidate["attempt"].is_null());
        let attempt = value(service.attempt_accept_candidate(id).unwrap());
        assert_eq!(attempt["decision"], outcome);
        assert_eq!(attempt["currentRevision"], 1);
    }
    let overview = value(service.candidate_overview(100).unwrap());
    assert_eq!(overview["heads"].as_array().unwrap().len(), 1);
    assert_eq!(overview["heads"][0]["versionId"], "next");
    assert_eq!(overview["candidates"][1]["candidateVersion"], "stale");
    assert_eq!(overview["candidates"][1]["attempt"]["decision"], "CONFLICT");
    assert!(core.candidate("b").unwrap().is_some());
    assert_eq!(
        core.candidate_current("asset").unwrap().unwrap().revision,
        1
    );
    assert!(value(service.get_version_detail("next").unwrap())["latestFeedback"].is_null());
}

fn sha256(bytes: &[u8]) -> String {
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
