#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(test)]
mod service_tests;
mod workbench_service;

use std::{
    path::PathBuf,
    sync::{Mutex, MutexGuard},
};
use tauri::State;
use workbench_service::{
    AppendFeedbackRequest, FeedbackRecordDto, ImportLocalFileRequest, ImportResultDto,
    LibrarySessionDto, PreviewDto, ServiceError, ValidationDto, VersionDetailDto, VersionListDto,
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

fn main() {
    tauri::Builder::default()
        .manage(Service::default())
        .invoke_handler(tauri::generate_handler![
            open_library,
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
