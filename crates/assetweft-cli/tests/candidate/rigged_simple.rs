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

fn role_name(role: ComponentRole) -> &'static str {
    match role {
        ComponentRole::Mesh => "mesh",
        ComponentRole::Materials => "materials",
        ComponentRole::Rig => "rig",
        ComponentRole::Motion => "motion",
        _ => panic!("unmapped licensed component"),
    }
}

fn seed(dir: &Path) -> PathBuf {
    let original =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/rigged-simple/RiggedSimple.glb");
    let derivative = dir.join("idle.glb");
    add_strike_with_idle(&original, &derivative, val::ValidationLimits::default()).unwrap();
    let db = dir.join("local.sqlite");
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
        ("contender", &derivative, "PROCEDURAL", "original"),
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
            let role_id = role_name(role);
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
    db
}

#[test]
fn stages_only_the_bound_motion_and_reopens_the_explicit_nonacceptance() {
    let dir = tempfile::tempdir().unwrap();
    let db = seed(dir.path());
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
        let expected = match after["role"].as_str().unwrap() {
            "materials" => "b1729ff12b1f4610af0ad3598abc9fd08fc5af21b0f55de79d276269fc14cfb1",
            "mesh" => "4c696f162fb41079ccf4bef4c6bec474871fd34a043e63c37da8cf09f95902a4",
            "rig" => "c820d21a30ee90209881bcc9e299d1523fc765acf18f55f07b11ae358076caac",
            "motion" => "340ecfb15607f8abfc13610a40c9538426b7d3dfec9b41e6f806d81a0e8cc88e",
            _ => panic!("unexpected staged role"),
        };
        assert_eq!(after["measured_sha256"], expected);
        if before["role"] == "motion" {
            assert_eq!(
                before["measured_sha256"],
                "95dd514c688c7b48c02f43ae90b8a87d86971e6984616eb1e2ef473b540bf6ac"
            );
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

#[test]
fn declared_structure_advances_once_and_retains_a_distinct_stale_version() {
    let dir = tempfile::tempdir().unwrap();
    let db = seed(dir.path());
    let file = dir.path().join("input.json");
    let contract = json!({"contract_id":"RIGGED_SIMPLE_MOTION_STRUCTURE_V1",
        "max_component_bytes":16777216,"required_quality":[]});
    fs::write(&file, contract.to_string()).unwrap();
    let mut init = candidate_write_args("initialize", &db, &file);
    init[5] = "edited".as_ref();
    assert_eq!(candidate_invoke(&init).0.status.code(), Some(2));
    init[5] = "original".as_ref();
    let (out, original) = candidate_invoke(&init);
    assert!(out.status.success(), "{original}");
    let head0 = &original["results"]["current"];
    assert_eq!(head0["revision"], 0);
    assert_eq!(head0["contract"], contract);

    let propose = |id: &str, version: &str| {
        fs::write(
            &file,
            json!({"id":id,"base_version":"original","candidate_version":version,
            "expected_revision":0,"editable":["motion"],"lock_mode":"BYTE","contract":contract})
            .to_string(),
        )
        .unwrap();
        candidate_invoke(&candidate_write_args(
            "propose-rigged-simple-motion",
            &db,
            &file,
        ))
    };
    let (out, bad) = propose("bad", "changed-lock");
    assert_eq!(out.status.code(), Some(2), "{bad}");
    candidate_query(&db, "show", "bad", &Value::Null);
    for (id, version) in [("winner", "edited"), ("stale", "contender")] {
        let (out, staged) = propose(id, version);
        assert!(out.status.success(), "{staged}");
        assert_eq!(staged["results"]["candidate"]["contract"], contract);
    }
    let attempt =
        |id: &str| candidate_invoke(&candidate_args("attempt", db.as_os_str(), id.as_ref()));
    let edited_object = dir
        .path()
        .join("objects/667d8bfccdeb68d64c9668ff54ab0af907aa240988a3d488f49ff7b43152b09d");
    let moved = dir.path().join("detached");
    fs::rename(&edited_object, &moved).unwrap();
    assert!(!attempt("winner").0.status.success());
    candidate_query(&db, "receipt", "winner", &Value::Null);
    catalog_query(
        &db,
        "current",
        "rigged-simple",
        json!({"scope":"stored_local_candidate_current","current":head0}),
    );
    fs::rename(&moved, &edited_object).unwrap();

    let (out, won) = attempt("winner");
    assert!(out.status.success(), "{won}");
    assert_eq!(
        won["results"]["scope"],
        "local_rigged_simple_structure_attempt"
    );
    let receipt = &won["results"]["receipt"];
    let current = &receipt["Accepted"]["current"];
    assert_eq!(current["revision"], 1);
    assert_eq!(current["version_id"], "edited");
    let report = &receipt["Accepted"]["report"];
    assert_eq!(report["scope"], contract["contract_id"]);
    assert_eq!(report["components"].as_array().unwrap().len(), 2);
    for (i, hash) in [
        "3a79dabb67bb0cd598a18d08b954d9d357c27c30672f82ef5d3f4e7fe6ca3401",
        "667d8bfccdeb68d64c9668ff54ab0af907aa240988a3d488f49ff7b43152b09d",
    ]
    .iter()
    .enumerate()
    {
        let row = &report["components"][i];
        assert_eq!(row["sha256"], *hash);
        assert_eq!(row["format_status"], "PASS");
        assert_eq!(row["role_status"], "PASS");
        assert_eq!(row["requirement"], "ANIMATED_CHARACTER");
    }
    let (out, lost) = attempt("stale");
    assert_eq!(out.status.code(), Some(1), "{lost}");
    let conflict = &lost["results"]["receipt"]["Conflict"];
    assert_eq!(conflict["current"], *current);
    assert_eq!(conflict["candidate"]["candidate_version"], "contender");
    candidate_query(&db, "show", "stale", &conflict["candidate"]);
    assert_eq!(attempt("stale").1["results"], lost["results"]);
    assert_eq!(attempt("winner").1["results"], won["results"]);
    candidate_query(&db, "receipt", "winner", receipt);
    catalog_query(
        &db,
        "current",
        "rigged-simple",
        json!({"scope":"stored_local_candidate_current","current":current}),
    );
}

#[test]
fn measured_motion_edit_runs_only_its_affected_stage() {
    use std::collections::BTreeMap;

    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("edit.sqlite");
    let file = dir.path().join("request.json");
    let output = dir.path().join("generated.glb");
    let original = include_bytes!("../../../../examples/rigged-simple/RiggedSimple.glb");
    let parts = rigged_simple_components(original).unwrap();
    let pass = |args: &[&OsStr]| {
        let (out, value) = candidate_invoke(args);
        assert!(out.status.success(), "{args:?} {value}");
        value["results"].clone()
    };
    let workflow = |action: &str, run: &OsStr| pass(&workflow_args(action, db.as_os_str(), run));
    let record = |run: &OsStr, id: &str, raw: &Value| {
        let outcome = dir.path().join("outcome.json");
        fs::write(
            &outcome,
            json!({"state":"succeeded","raw":raw.to_string()}).to_string(),
        )
        .unwrap();
        let mut args = workflow_args("finish", db.as_os_str(), run);
        args.extend([
            "--step-id".as_ref(),
            id.as_ref(),
            "--outcome".as_ref(),
            outcome.as_os_str(),
        ]);
        pass(&args);
    };

    let mut app = Application::open(&db).unwrap();
    app.initialize_library().unwrap();
    commit(
        &mut app, "original", "sample", "UNKNOWN", "cesium", "GLB", original,
    );
    app.create_workflow(
        "baseline",
        &parts
            .iter()
            .map(|(r, _)| step(role_name(*r), &[]))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let mut seen = BTreeMap::new();
    let mut drive = |run: &str, execute: &mut dyn FnMut(&str) -> Value| {
        loop {
            let claim = workflow("claim", run.as_ref());
            let Some(id) = claim["step_id"].as_str() else {
                break;
            };
            let entry = seen
                .entry((run.to_owned(), id.to_owned()))
                .or_insert((0, 0));
            entry.0 += 1;
            let result = execute(id);
            entry.1 += 1;
            record(run.as_ref(), id, &result);
        }
    };
    drive("baseline", &mut |id| {
        let (role, bytes) = parts.iter().find(|(r, _)| role_name(*r) == id).unwrap();
        commit(&mut app, id, id, "UNKNOWN", "original", "JSON", bytes);
        app.add_component_reference(&ComponentReference {
            owner_version_id: "original".into(),
            role: *role,
            component_version_id: id.into(),
        })
        .unwrap();
        json!({"component":id,"sha256":sha256(bytes)})
    });
    let contract = json!({"contract_id":"RIGGED_SIMPLE_MOTION_STRUCTURE_V1","max_component_bytes":16777216,"required_quality":[]});
    fs::write(&file, contract.to_string()).unwrap();
    let mut init = candidate_write_args("initialize", &db, &file);
    init[5] = "original".as_ref();
    pass(&init);
    let proposal = json!({"id":"edit","base_version":"original","candidate_version":"edited","expected_revision":0,"editable":["motion"],"lock_mode":"BYTE","contract":contract});
    fs::write(&file, proposal.to_string()).unwrap();
    let schedule = candidate_write_args("schedule-rigged-simple-motion", &db, &file);
    for (field, value) in [
        ("editable", json!(["mesh"])),
        ("lock_mode", json!("SEMANTIC")),
        ("expected_revision", json!(1)),
        ("contract", Value::Null),
    ] {
        let mut invalid = proposal.clone();
        invalid[field] = value;
        fs::write(&file, invalid.to_string()).unwrap();
        assert_eq!(candidate_invoke(&schedule).0.status.code(), Some(2));
    }
    fs::write(&file, proposal.to_string()).unwrap();
    assert!(app.asset_version("edited").unwrap().is_none());
    let plan = pass(&schedule);
    assert_eq!(plan["run_id"], proposal["id"]);
    assert_eq!(plan["candidate_version"], proposal["candidate_version"]);
    let run = plan["run_id"].as_str().unwrap();
    let scheduled = workflow("status", run.as_ref());
    assert_eq!(
        scheduled["steps"],
        json!([{"id":"motion","state":"pending","raw":null,"evidence":null}])
    );
    let export = pass(&catalog_args(
        "materialize-export",
        db.as_os_str(),
        plan["base_version"].as_str().unwrap().as_ref(),
    ));
    let source = dir.path().join(export["relative_path"].as_str().unwrap());
    assert!(!output.exists());
    drive(run, &mut |id| {
        assert_eq!(id, "motion", "locked component dispatched");
        assert_eq!(
            workflow("status", run.as_ref())["steps"][0]["state"],
            "running"
        );
        assert!(workflow("claim", run.as_ref())["step_id"].is_null());
        pass(&[
            "motion-strike".as_ref(),
            source.as_os_str(),
            "--with-idle".as_ref(),
            "--out".as_ref(),
            output.as_os_str(),
        ])
    });
    for (role, _) in &parts {
        let name = role_name(*role).to_owned();
        assert_eq!(seen.get(&("baseline".into(), name.clone())), Some(&(1, 1)));
        assert_eq!(
            seen.get(&("edit".into(), name.clone())),
            (name == "motion").then_some(&(1, 1))
        );
    }

    let rows = workflow("status", "edit".as_ref());
    let row = &rows["steps"][0];
    assert_eq!(row["state"], "succeeded");
    let driver: Value = serde_json::from_str(row["raw"].as_str().unwrap()).unwrap();
    let generated = fs::read(&output).unwrap();
    assert_eq!(driver["source_sha256"], sha256(original));
    assert_eq!(driver["output_sha256"], sha256(&generated));
    assert_eq!(
        driver["output_sha256"],
        "667d8bfccdeb68d64c9668ff54ab0af907aa240988a3d488f49ff7b43152b09d"
    );
    commit(
        &mut app,
        "edited",
        "sample",
        "PROCEDURAL",
        "original",
        "GLB",
        &generated,
    );
    for (role, payload) in rigged_simple_components(&generated).unwrap() {
        let id = if role == ComponentRole::Motion {
            commit(
                &mut app,
                "edited-motion",
                "motion",
                "PROCEDURAL",
                "original",
                "JSON",
                &payload,
            );
            "edited-motion"
        } else {
            role_name(role)
        };
        app.add_component_reference(&ComponentReference {
            owner_version_id: "edited".into(),
            role,
            component_version_id: id.into(),
        })
        .unwrap();
    }
    let staged = pass(&candidate_write_args(
        "propose-rigged-simple-motion",
        &db,
        &file,
    ));
    let candidate = &staged["candidate"];
    assert_eq!(candidate["base"], plan["base"]);
    for (base, proposed) in candidate["base"]
        .as_array()
        .unwrap()
        .iter()
        .zip(candidate["proposed"].as_array().unwrap())
    {
        assert_eq!(base == proposed, base["role"] != "motion");
    }
    let attempt = pass(&candidate_args("attempt", db.as_os_str(), "edit".as_ref()));
    assert_eq!(
        attempt["receipt"]["Accepted"]["current"]["version_id"],
        "edited"
    );
    candidate_query(&db, "receipt", "edit", &attempt["receipt"]);
}
