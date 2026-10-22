//! Tauri commands and the start-up sequence. The work itself is in dsr_core; this file runs it on a blocking
//! thread and reports to the window through the `status` event.

use crate::{
    messages::{feed_message, routine_message},
    settings::{default_game_folder, remember_folder_for_uninstaller, Settings},
    status::UiStatus,
};
use dsr_core::{
    display::{self, ResolutionSetting, ScreenMode},
    extract::Unpacker,
    feed::Feed,
    fetch::Fetcher,
    gameconfig::{self, DisplayChoice},
    inspect::CheckMode,
    launch,
    model::{Recipe, ServerInfo, ARCHIVE_HOSTS},
    progress::Progress,
    routine::{self, Routine},
};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

/// Where the signed server-info and recipes are published. The owner's domain goes first once it exists.
const FEED_BASES: &[&str] = &["https://dragon-soul-reborn.github.io/dragon-soul-reborn/"];

pub struct AppState {
    settings: Mutex<Settings>,
    status: Mutex<UiStatus>,
    server: Mutex<Option<ServerInfo>>,
    recipe: Mutex<Option<Recipe>>,
    offline: AtomicBool,
    cancel: Arc<AtomicBool>,
    busy: AtomicBool,
}

impl AppState {
    pub fn new(settings: Settings) -> AppState {
        AppState {
            settings: Mutex::new(settings),
            status: Mutex::new(UiStatus::Starting),
            server: Mutex::new(None),
            recipe: Mutex::new(None),
            offline: AtomicBool::new(false),
            cancel: Arc::new(AtomicBool::new(false)),
            busy: AtomicBool::new(false),
        }
    }
}

fn set_status(app: &AppHandle, status: UiStatus) {
    *app.state::<AppState>().status.lock().unwrap() = status.clone();
    let _ = app.emit("status", status);
}

/// Update the launcher, load the signed feed, then make the game folder right (or ask for one).
pub async fn startup(app: AppHandle) {
    set_status(&app, UiStatus::Starting);
    if update_launcher(&app).await {
        return;
    }
    if let Err(message) = load_feed(&app).await {
        set_status(&app, UiStatus::Error { message });
        return;
    }
    let has_folder = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .game_folder
        .is_some();
    if has_folder {
        run_routine(app, CheckMode::Quick, Vec::new()).await;
    } else {
        set_status(
            &app,
            UiStatus::NeedsFolder {
                suggested: default_game_folder().display().to_string(),
            },
        );
    }
}

/// Installs a signed launcher update if there is one. Returns true when the app is restarting.
async fn update_launcher(app: &AppHandle) -> bool {
    use tauri_plugin_updater::UpdaterExt;
    if feed_pubkey(app).is_none() {
        return false;
    }
    let checked = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(e) => Err(e),
    };
    match checked {
        Ok(Some(update)) => {
            set_status(app, UiStatus::UpdatingLauncher);
            match update.download_and_install(|_, _| {}, || {}).await {
                Ok(()) => app.restart(),
                Err(e) => {
                    log::warn!("launcher update failed: {e}");
                    false
                }
            }
        }
        Ok(None) => false,
        Err(e) => {
            log::warn!("launcher update check failed: {e}");
            false
        }
    }
}

/// The updater's public key (tauri.conf.json) also verifies server-info and recipes.
fn feed_pubkey(app: &AppHandle) -> Option<String> {
    app.config()
        .plugins
        .0
        .get("updater")?
        .get("pubkey")?
        .as_str()
        .filter(|key| !key.trim().is_empty())
        .map(str::to_string)
}

async fn load_feed(app: &AppHandle) -> Result<(), String> {
    let cache = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("feed");
    let pubkey = feed_pubkey(app)
        .ok_or("Release signing is not configured in this development build yet.")?;
    let (info, recipe) = tauri::async_runtime::spawn_blocking(move || {
        let feed = Feed::new(FEED_BASES, &pubkey);
        let info = feed.server_info(&cache).map_err(|e| feed_message(&e))?;
        let recipe = feed
            .recipe(&info.value.game, &cache, ARCHIVE_HOSTS)
            .map_err(|e| feed_message(&e))?;
        Ok::<_, String>((info, recipe))
    })
    .await
    .map_err(|e| e.to_string())??;
    let current =
        semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version is semver");
    let needed = semver::Version::parse(&recipe.value.min_launcher).map_err(|e| e.to_string())?;
    if current < needed {
        return Err(format!("This game version needs launcher {needed} or newer. Restart the launcher to update it."));
    }
    let st = app.state::<AppState>();
    st.offline
        .store(info.offline || recipe.offline, Ordering::SeqCst);
    *st.server.lock().unwrap() = Some(info.value);
    *st.recipe.lock().unwrap() = Some(recipe.value);
    Ok(())
}

/// The bundled innoextract sits next to the launcher exe (Tauri drops the target-triple suffix).
fn innoextract_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("innoextract.exe")))
        .unwrap_or_else(|| PathBuf::from("innoextract.exe"))
}

pub async fn run_routine(app: AppHandle, mode: CheckMode, extra_copies: Vec<PathBuf>) {
    let st = app.state::<AppState>();
    if st.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    let folder = st.settings.lock().unwrap().game_folder.clone();
    let recipe = st.recipe.lock().unwrap().clone();
    let (Some(folder), Some(recipe)) = (folder, recipe) else {
        st.busy.store(false, Ordering::SeqCst);
        return;
    };
    if launch::exe_in_use(&folder) {
        st.busy.store(false, Ordering::SeqCst);
        set_status(
            &app,
            UiStatus::Error {
                message: "Close Dragon Soul first, then press Retry.".into(),
            },
        );
        return;
    }
    st.cancel.store(false, Ordering::SeqCst);
    let cancel = st.cancel.clone();
    let worker = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let routine = Routine {
            folder,
            fetcher: Fetcher::new(ARCHIVE_HOSTS),
            unpacker: Unpacker {
                innoextract: innoextract_path(),
            },
            extra_copies,
        };
        let mut last = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .unwrap_or_else(Instant::now);
        routine.run(&recipe, mode, &cancel, &mut |p: Progress| {
            // Counters arrive thousands of times; send about ten a second. Stage changes always go through.
            let counter = matches!(
                p,
                Progress::Checking { .. }
                    | Progress::Downloading { .. }
                    | Progress::Building { .. }
            );
            if !counter || last.elapsed() >= Duration::from_millis(100) {
                last = Instant::now();
                set_status(&worker, UiStatus::Working { progress: p });
            }
        })
    })
    .await;
    st.busy.store(false, Ordering::SeqCst);
    let offline = st.offline.load(Ordering::SeqCst);
    match result {
        Ok(Ok(outcome)) => {
            let note = (!outcome.vanished.is_empty()).then(|| {
                format!(
                    "{} game file(s) had gone missing and were restored. If this keeps happening, your antivirus may be removing them.",
                    outcome.vanished.len()
                )
            });
            set_status(&app, UiStatus::Ready { offline, note });
        }
        Ok(Err(e)) => {
            log::error!("game files: {e}");
            set_status(
                &app,
                UiStatus::Error {
                    message: routine_message(&e),
                },
            );
        }
        Err(e) => set_status(
            &app,
            UiStatus::Error {
                message: format!("Internal error: {e}"),
            },
        ),
    }
}

fn persist(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    settings
        .save(&dir)
        .map_err(|e| format!("Couldn't save settings: {e}"))?;
    remember_folder_for_uninstaller(settings.game_folder.as_deref());
    Ok(())
}

#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> UiStatus {
    state.status.lock().unwrap().clone()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoView {
    message: String,
    game: String,
    launcher_version: String,
}

#[tauri::command]
pub fn get_info(state: State<'_, AppState>) -> Option<InfoView> {
    state.server.lock().unwrap().as_ref().map(|i| InfoView {
        message: i.message.clone(),
        game: i.game.clone(),
        launcher_version: env!("CARGO_PKG_VERSION").into(),
    })
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let folder_changed = {
        let st = app.state::<AppState>();
        if let (Some(over), Some(info)) = (
            settings.server_override.as_deref(),
            st.server.lock().unwrap().as_ref(),
        ) {
            launch::with_override(&info.server, Some(over))?;
        }
        let mut current = st.settings.lock().unwrap();
        let changed = current.game_folder != settings.game_folder;
        *current = settings.clone();
        changed
    };
    persist(&app, &settings)?;
    if folder_changed {
        run_routine(app, CheckMode::Quick, Vec::new()).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn retry(app: AppHandle) -> Result<(), String> {
    startup(app).await;
    Ok(())
}

#[tauri::command]
pub async fn repair(app: AppHandle) -> Result<(), String> {
    run_routine(app, CheckMode::Full, Vec::new()).await;
    Ok(())
}

#[tauri::command]
pub fn cancel(state: State<'_, AppState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Choose the game folder")
            .blocking_pick_folder()
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(picked
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string()))
}

#[tauri::command]
pub async fn pick_installer(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Choose the Dragon Soul installer")
            .add_filter("Installer", &["exe"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(picked
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderCheck {
    exists: bool,
    empty: bool,
    has_game: bool,
    free_bytes: Option<u64>,
    needed_bytes: u64,
}

#[tauri::command]
pub fn check_folder(state: State<'_, AppState>, path: String) -> FolderCheck {
    let folder = PathBuf::from(path.trim());
    let recipe = state.recipe.lock().unwrap();
    let exists = folder.is_dir();
    let empty = !exists
        || fs::read_dir(&folder)
            .map(|mut d| d.next().is_none())
            .unwrap_or(true);
    let has_game = folder.join(launch::GAME_EXE).exists()
        || recipe
            .as_ref()
            .is_some_and(|r| r.files.iter().any(|e| folder.join(&e.path).exists()));
    FolderCheck {
        exists,
        empty,
        has_game,
        free_bytes: routine::free_space_near(&folder),
        needed_bytes: recipe.as_ref().map(routine::install_size).unwrap_or(0),
    }
}

#[tauri::command]
pub async fn install(app: AppHandle, folder: String, copy: Option<String>) -> Result<(), String> {
    let settings = {
        let st = app.state::<AppState>();
        let mut s = st.settings.lock().unwrap();
        s.game_folder = Some(PathBuf::from(folder.trim()));
        s.clone()
    };
    persist(&app, &settings)?;
    run_routine(
        app,
        CheckMode::Full,
        copy.map(PathBuf::from).into_iter().collect(),
    )
    .await;
    Ok(())
}

#[derive(Serialize)]
pub struct DisplayOptions {
    native: ScreenMode,
    auto: ScreenMode,
    modes: Vec<ScreenMode>,
}

#[tauri::command]
pub fn display_options() -> DisplayOptions {
    let (native, modes) = display::screen_modes();
    let auto = display::choose(&ResolutionSetting::Auto, native, &modes);
    DisplayOptions {
        native,
        auto,
        modes,
    }
}

#[tauri::command]
pub async fn server_status(app: AppHandle) -> Result<Option<bool>, String> {
    let addr = {
        let st = app.state::<AppState>();
        let over = st.settings.lock().unwrap().server_override.clone();
        let Some(info) = st.server.lock().unwrap().clone() else {
            return Ok(None);
        };
        launch::with_override(&info.server, over.as_deref())?
    };
    let up = tauri::async_runtime::spawn_blocking(move || {
        launch::server_up(&addr.ip, addr.port, Duration::from_secs(3))
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(Some(up))
}

#[tauri::command]
pub fn play(window: WebviewWindow, state: State<'_, AppState>) -> Result<(), String> {
    if state.busy.load(Ordering::SeqCst)
        || !matches!(*state.status.lock().unwrap(), UiStatus::Ready { .. })
    {
        return Err("Wait for the game files to finish checking before pressing Play.".into());
    }
    let settings = state.settings.lock().unwrap().clone();
    let folder = settings
        .game_folder
        .clone()
        .ok_or("Install the game first.")?;
    let info = state
        .server
        .lock()
        .unwrap()
        .clone()
        .ok_or("The server details haven't loaded yet. Press Retry.")?;
    let addr = launch::with_override(&info.server, settings.server_override.as_deref())?;
    let (native, modes) = display::screen_modes();
    let mode = display::choose(&settings.resolution, native, &modes);
    let choice = DisplayChoice {
        width: mode.width,
        height: mode.height,
        fullscreen: settings.fullscreen,
    };
    gameconfig::apply_display(&folder, &choice)
        .map_err(|e| format!("Couldn't write the display settings: {e}"))?;
    launch::start_game(&folder, &addr).map_err(|e| format!("Couldn't start the game: {e}"))?;
    log::info!(
        "started the game at {}x{} against {}:{}",
        mode.width,
        mode.height,
        addr.ip,
        addr.port
    );
    let _ = window.minimize();
    Ok(())
}

#[tauri::command]
pub fn open_logs(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// Keeps the newest `keep` log files.
pub fn prune_logs(dir: &Path, keep: usize) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            if !m.is_file() {
                return None;
            }
            Some((m.modified().ok()?, e.path()))
        })
        .collect();
    files.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    for (_, path) in files.into_iter().skip(keep) {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn prune_logs_keeps_the_newest_files() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..7u64 {
            let p = dir.path().join(format!("launcher{i}.log"));
            fs::write(&p, b"x").unwrap();
            let f = fs::File::options().write(true).open(&p).unwrap();
            f.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 + i))
                .unwrap();
        }
        prune_logs(dir.path(), 5);
        let mut left: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(
            left,
            vec![
                "launcher2.log",
                "launcher3.log",
                "launcher4.log",
                "launcher5.log",
                "launcher6.log"
            ]
        );
    }
}
