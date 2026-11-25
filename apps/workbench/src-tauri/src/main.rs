#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(test)]
mod service_tests;
mod workbench_service;

use std::{
    path::PathBuf,
    sync::{Mutex, MutexGuard},
};
use tauri::{Manager, State};
use workbench_service::{
    AppendFeedbackRequest, ApproveBudgetRequest, BudgetAccountDto, BudgetIntentDto,
    BudgetOverviewDto, ByoGpuRunDto, CandidateAttemptDto, CandidateHeadDto, CandidateItemDto,
    CandidateOverviewDto, CapabilityOverviewDto, CapabilityRunResultDto, FeedbackRecordDto,
    ImportLocalFileRequest, ImportResultDto, InitHeadRequest, LibrarySessionDto, PreviewDto,
    ProposeCandidateRequest, ReconcileCallRequest, ReserveCallRequest, RunByoGpuRequest,
    RunCapabilityRequest, ServiceError, ValidationDto, VersionDetailDto, VersionListDto,
    WorkbenchService,
};

type Service = Mutex<WorkbenchService>;

fn lock<'a>(state: &State<'a, Service>) -> Result<MutexGuard<'a, WorkbenchService>, ServiceError> {
    state.inner().lock().map_err(|_| ServiceError {
        code: "INTERNAL",
        message: "The local library service is unavailable".into(),
    })
}

#[tauri::command]
fn capability_overview() -> Result<CapabilityOverviewDto, ServiceError> {
    WorkbenchService::default().capability_overview()
}

#[tauri::command]
async fn run_capability_check(
    req: RunCapabilityRequest,
) -> Result<CapabilityRunResultDto, ServiceError> {
    tauri::async_runtime::spawn_blocking(move || {
        WorkbenchService::default().run_capability_check(req)
    })
    .await
    .map_err(|error| ServiceError {
        code: "INTERNAL",
        message: error.to_string(),
    })?
}

#[tauri::command]
fn open_library(
    state: State<'_, Service>,
    db_path: PathBuf,
    create_if_missing: bool,
) -> Result<LibrarySessionDto, ServiceError> {
    lock(&state)?.open_library(&db_path, create_if_missing)
}

#[tauri::command]
fn list_versions(
    state: State<'_, Service>,
    query: Option<String>,
    limit: u16,
) -> Result<VersionListDto, ServiceError> {
    lock(&state)?.list_versions(query.as_deref(), limit)
}

#[tauri::command]
fn get_version_detail(
    state: State<'_, Service>,
    version_id: String,
) -> Result<VersionDetailDto, ServiceError> {
    lock(&state)?.get_version_detail(&version_id)
}

#[tauri::command]
fn load_preview(state: State<'_, Service>, version_id: String) -> Result<PreviewDto, ServiceError> {
    lock(&state)?.load_preview(&version_id)
}

#[tauri::command]
fn validate_version(
    state: State<'_, Service>,
    version_id: String,
    role: String,
) -> Result<ValidationDto, ServiceError> {
    lock(&state)?.validate_version(&version_id, &role)
}

#[tauri::command]
fn append_feedback(
    state: State<'_, Service>,
    req: AppendFeedbackRequest,
) -> Result<FeedbackRecordDto, ServiceError> {
    lock(&state)?.append_feedback(req)
}

#[tauri::command]
fn import_local_file(
    state: State<'_, Service>,
    req: ImportLocalFileRequest,
) -> Result<ImportResultDto, ServiceError> {
    lock(&state)?.import_local_file(req)
}

#[tauri::command]
fn workbench_bootstrap_state(
    state: State<'_, Service>,
) -> Result<Option<LibrarySessionDto>, ServiceError> {
    std::env::var_os("AW_WORKBENCH_DB")
        .map(|db| lock(&state)?.open_library(&PathBuf::from(db), false))
        .transpose()
}

#[tauri::command]
fn budget_overview(
    state: State<'_, Service>,
    limit: u16,
) -> Result<BudgetOverviewDto, ServiceError> {
    lock(&state)?.budget_overview(limit)
}

#[tauri::command]
fn approve_budget(
    state: State<'_, Service>,
    req: ApproveBudgetRequest,
) -> Result<BudgetAccountDto, ServiceError> {
    lock(&state)?.approve_budget(req)
}

#[tauri::command]
fn reserve_call(
    state: State<'_, Service>,
    req: ReserveCallRequest,
) -> Result<BudgetIntentDto, ServiceError> {
    lock(&state)?.reserve_call(req)
}

#[tauri::command]
fn cancel_call(
    state: State<'_, Service>,
    intent_id: String,
) -> Result<BudgetIntentDto, ServiceError> {
    lock(&state)?.cancel_call(&intent_id)
}

#[tauri::command]
fn reconcile_call(
    state: State<'_, Service>,
    req: ReconcileCallRequest,
) -> Result<BudgetIntentDto, ServiceError> {
    lock(&state)?.reconcile_call(req)
}

#[tauri::command]
fn candidate_overview(
    state: State<'_, Service>,
    limit: u16,
) -> Result<CandidateOverviewDto, ServiceError> {
    lock(&state)?.candidate_overview(limit)
}

#[tauri::command]
fn initialize_candidate_head(
    state: State<'_, Service>,
    req: InitHeadRequest,
) -> Result<CandidateHeadDto, ServiceError> {
    lock(&state)?.initialize_candidate_head(req)
}

#[tauri::command]
fn propose_candidate(
    state: State<'_, Service>,
    req: ProposeCandidateRequest,
) -> Result<CandidateItemDto, ServiceError> {
    lock(&state)?.propose_candidate(req)
}

#[tauri::command]
fn attempt_accept_candidate(
    state: State<'_, Service>,
    candidate_id: String,
) -> Result<CandidateAttemptDto, ServiceError> {
    lock(&state)?.attempt_accept_candidate(&candidate_id)
}

#[tauri::command]
async fn run_byo_gpu(
    app: tauri::AppHandle,
    req: RunByoGpuRequest,
) -> Result<ByoGpuRunDto, ServiceError> {
    tauri::async_runtime::spawn_blocking(move || lock(&app.state::<Service>())?.run_byo_gpu(req))
        .await
        .map_err(|error| ServiceError {
            code: "INTERNAL",
            message: error.to_string(),
        })?
}

fn main() {
    tauri::Builder::default()
        .manage(Service::default())
        .invoke_handler(tauri::generate_handler![
            open_library,
            capability_overview,
            run_capability_check,
            budget_overview,
            approve_budget,
            reserve_call,
            cancel_call,
            reconcile_call,
            candidate_overview,
            initialize_candidate_head,
            propose_candidate,
            attempt_accept_candidate,
            run_byo_gpu,
            list_versions,
            get_version_detail,
            load_preview,
            validate_version,
            append_feedback,
            import_local_file,
            workbench_bootstrap_state,
        ])
        .run(tauri::generate_context!())
        .expect("start AssetWeft Workbench");
}
