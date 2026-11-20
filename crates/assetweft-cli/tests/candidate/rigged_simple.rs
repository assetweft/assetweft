//! Actual-binary staging, locked reference selection and fresh receipt reads on Cesium's licensed sample.
//! Application seeds managed immutable versions; no fixture head or executor participates.
use super::*;
use assetweft_core::{
    library::{AssetVersion, ComponentReference, ComponentRole, Presence},
    motion::{add_strike_with_idle, components::rigged_simple_components},
};

fn commit(
    app: &mut Application,
    id: &str,
    asset: &str,
    source: &str,
    input: &str,
    format: &str,
    bytes: &[u8],
) {
    let metadata = AssetVersion {
        asset_id: asset.into(),
        version_id: id.into(),
        content_hash: sha256(bytes),
        input_id: input.into(),
        source: source.into(),
        authorization: "Cesium CC BY 4.0 sample attribution".into(),
        authorized: true,
        license_id: "CC-BY-4.0".into(),
        usage_scope: "LOCAL_SAMPLE".into(),
        source_format: format.into(),
        contract_version: "1.0.0".into(),
        presence: Presence::Unknown,
    };
    app.commit_object(&metadata, &mut &bytes[..], bytes.len() as u64, 128 * 1024)
        .unwrap();
}

#[test]
fn stages_only_the_bound_motion_and_reopens_the_explicit_nonacceptance() {
    let dir = tempfile::tempdir().unwrap();
    let original =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/rigged-simple/RiggedSimple.glb");
    let derivative = dir.path().join("idle.glb");
    add_strike_with_idle(&original, &derivative, val::ValidationLimits::default()).unwrap();
    let db = dir.path().join("local.sqlite");
    let mut app = Application::open(&db).unwrap();
    app.initialize_library().unwrap();
    for (owner, path, source, input) in [
        (
            "original",
            &original,
            "UNKNOWN",
            "Cesium-RiggedSimple-f0920f5",
        ),
        ("edited", &derivative, "PROCEDURAL", "original"),
        ("changed-lock", &derivative, "PROCEDURAL", "original"),
        ("whole-as-motion", &derivative, "PROCEDURAL", "original"),
        ("changed-material", &derivative, "PROCEDURAL", "original"),
    ] {
        let bytes = fs::read(path).unwrap();
        commit(
            &mut app,
            owner,
            "rigged-simple",
            source,
            input,
            "GLB",
            &bytes,
        );
        for (role, mut payload) in rigged_simple_components(&bytes).unwrap() {
            let role_id = match role {
                ComponentRole::Mesh => "mesh",
                ComponentRole::Materials => "materials",
                ComponentRole::Rig => "rig",
                ComponentRole::Motion => "motion",
                _ => panic!("unexpected role"),
            };
            let part = match (owner, role) {
                ("changed-material", ComponentRole::Materials) => {
                    let mut changed: Value = serde_json::from_slice(&payload).unwrap();
                    changed["sections"]["materials"][0]["pbrMetallicRoughness"]["metallicFactor"] =
                        json!(1);
                    payload = serde_json::to_vec(&changed).unwrap();
                    "other-material"
                }
                ("changed-lock", ComponentRole::Mesh) => "duplicate-mesh",
                ("whole-as-motion", ComponentRole::Motion) => "whole-as-motion",
                ("original", ComponentRole::Motion) => "original-motion",
                (_, ComponentRole::Motion) => "edited-motion",
                _ => role_id,
            };
            if app.asset_version(part).unwrap().is_none() {
                commit(&mut app, part, role_id, source, owner, "JSON", &payload);
            }
            app.add_component_reference(&ComponentReference {
                owner_version_id: owner.into(),
                role,
                component_version_id: part.into(),
            })
            .unwrap();
        }
    }
    app.close().unwrap();
    let path = dir.path().join("proposal.json");
    let write = |id: &str, version: &str| {
        fs::write(
            &path,
            json!({"id":id,"base_version":"original","candidate_version":version,
            "expected_revision":0,"editable":["motion"],"lock_mode":"BYTE"})
            .to_string(),
        )
        .unwrap();
    };
    let stage = |action| candidate_invoke(&candidate_write_args(action, &db, &path));
    write("edit", "edited");
    let (out, fixture) = stage("propose");
    assert_eq!(out.status.code(), Some(2));
    assert!(
        fixture["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("only declared TEST_FIXTURE staging supported")
    );
    let (out, staged) = stage("propose-rigged-simple-motion");
    assert!(out.status.success(), "{staged}");
    let c = &staged["results"]["candidate"];
    assert_eq!(c["scope"], "RIGGED_SIMPLE_MOTION_BYTE_STAGING_V1");
    assert_eq!(c["source"], "PROCEDURAL");
    assert_eq!(c["editable"], json!(["motion"]));
    assert_eq!(c["base"].as_array().unwrap().len(), 4);
    for (before, after) in c["base"]
        .as_array()
        .unwrap()
        .iter()
        .zip(c["proposed"].as_array().unwrap())
    {
        if before["role"] == "motion" {
            assert_ne!(before["measured_sha256"], after["measured_sha256"]);
            assert_eq!(after["version"], "edited-motion");
        } else {
            assert_eq!(before, after);
        }
        let id = after["version"].as_str().unwrap();
        let (out, export) = candidate_invoke(&catalog_args(
            "materialize-export",
            db.as_os_str(),
            id.as_ref(),
        ));
        assert!(out.status.success(), "{export}");
        let selected = fs::read(
            dir.path()
                .join(export["results"]["relative_path"].as_str().unwrap()),
        )
        .unwrap();
        assert_eq!(sha256(&selected), after["measured_sha256"]);
    }
    candidate_query(&db, "show", "edit", c);
    candidate_query(&db, "receipt", "edit", &Value::Null);
    let (out, attempt) =
        candidate_invoke(&candidate_args("attempt", db.as_os_str(), "edit".as_ref()));
    assert_eq!(out.status.code(), Some(5));
    let receipt = &attempt["results"]["receipt"];
    assert_eq!(receipt["NotRun"]["candidate"], *c);
    assert_eq!(
        receipt["NotRun"]["reason"],
        "no predeclared GLB structure contract"
    );
    assert!(receipt["NotRun"]["report"].is_null());
    candidate_query(&db, "receipt", "edit", receipt);
    for (version, reason) in [
        ("changed-lock", "locked component changed"),
        ("whole-as-motion", "Rigged Simple component profile"),
        ("changed-material", "Rigged Simple component profile"),
    ] {
        write("bad", version);
        let (out, refused) = stage("propose-rigged-simple-motion");
        assert_eq!(out.status.code(), Some(2), "{refused}");
        assert!(
            refused["errors"][0]["message"]
                .as_str()
                .unwrap()
                .contains(reason)
        );
        candidate_query(&db, "show", "bad", &Value::Null);
    }
    catalog_query(
        &db,
        "current",
        "rigged-simple",
        json!({"scope":"stored_local_candidate_current","current":null}),
    );
}
