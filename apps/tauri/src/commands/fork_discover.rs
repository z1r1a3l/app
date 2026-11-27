//! Fork: Discover tab commands. Thin wrappers over
//! `lettuce_app::fork_discover`; downloads land in the app cache so the
//! regular import preview/commit can read them like any picked file.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use lettuce_app::api::ApiContextSlot;
use lettuce_app::fork_discover;
use lettuce_contracts::{
    ApiError, ApiErrorCode, DiscoverDetail, DiscoverDownload, DiscoverItemRequest, DiscoverSearchPage,
    DiscoverSearchRequest, FileSource,
};
use tauri::{AppHandle, Manager, Runtime, State};

/// Downloads older than this are swept on the next download.
const KEEP_FOR: Duration = Duration::from_secs(24 * 60 * 60);

fn storage_error(message: &str) -> ApiError {
    ApiError { code: ApiErrorCode::Unavailable, message: message.to_owned(), details: None }
}

#[tauri::command]
#[specta::specta]
pub async fn discover_search(
    context: State<'_, ApiContextSlot>,
    request: DiscoverSearchRequest,
) -> Result<DiscoverSearchPage, ApiError> {
    let _context = context.snapshot()?;
    fork_discover::search(&request).await
}

#[tauri::command]
#[specta::specta]
pub async fn discover_tags(
    context: State<'_, ApiContextSlot>,
    request: lettuce_contracts::DiscoverTagsRequest,
) -> Result<lettuce_contracts::DiscoverTagsPage, ApiError> {
    let _context = context.snapshot()?;
    fork_discover::tags(&request).await
}

#[tauri::command]
#[specta::specta]
pub async fn discover_detail(
    context: State<'_, ApiContextSlot>,
    request: DiscoverItemRequest,
) -> Result<DiscoverDetail, ApiError> {
    let _context = context.snapshot()?;
    fork_discover::detail(&request).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn discover_download<R: Runtime>(
    app: AppHandle<R>,
    context: State<'_, ApiContextSlot>,
    request: DiscoverItemRequest,
) -> Result<DiscoverDownload, ApiError> {
    let _context = context.snapshot()?;
    let file = fork_discover::download(&request).await?;
    let folder = app
        .path()
        .app_cache_dir()
        .map_err(|_| storage_error("the app cache folder is unavailable"))?
        .join("discover");
    let path = tauri::async_runtime::spawn_blocking(move || store(&folder, &file.file_name, &file.bytes))
        .await
        .map_err(|_| storage_error("saving the download was interrupted"))??;
    let file_name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(DiscoverDownload { source: FileSource { uri: path.display().to_string() }, file_name })
}

fn store(folder: &Path, file_name: &str, bytes: &[u8]) -> Result<PathBuf, ApiError> {
    std::fs::create_dir_all(folder).map_err(|_| storage_error("could not create the download folder"))?;
    sweep(folder);
    let stamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    // The adapters already reduce names to safe characters; keep the
    // original name last so the import preview sees the right extension.
    let path = folder.join(format!("{stamp}-{file_name}"));
    std::fs::write(&path, bytes).map_err(|_| storage_error("could not save the download"))?;
    Ok(path)
}

fn sweep(folder: &Path) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > KEEP_FOR);
        if old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_keeps_the_extension_and_unique_names() {
        let folder = std::env::temp_dir().join(format!("lettuce-discover-store-{}", std::process::id()));
        let first = store(&folder, "card.png", b"x").expect("store");
        assert!(first.file_name().is_some_and(|name| name.to_string_lossy().ends_with("-card.png")));
        assert_eq!(std::fs::read(&first).expect("read"), b"x");
        std::fs::remove_dir_all(folder).expect("cleanup");
    }
}
