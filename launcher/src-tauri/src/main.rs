#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod messages;
mod settings;
mod status;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        // Must be first: a second launch focuses the running launcher instead of starting another.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .clear_targets()
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("launcher".into()),
                    },
                ))
                .max_file_size(1_000_000)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepAll)
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let settings = settings::Settings::load(&app.path().app_config_dir()?);
            settings::remember_folder_for_uninstaller(settings.game_folder.as_deref());
            app.manage(app::AppState::new(settings));
            if let Ok(dir) = app.path().app_log_dir() {
                app::prune_logs(&dir, 5);
            }
            tauri::async_runtime::spawn(app::startup(app.handle().clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app::get_status,
            app::get_info,
            app::get_settings,
            app::save_settings,
            app::retry,
            app::repair,
            app::cancel,
            app::pick_folder,
            app::pick_installer,
            app::check_folder,
            app::install,
            app::display_options,
            app::server_status,
            app::play,
            app::open_logs
        ])
        .run(tauri::generate_context!())
        .expect("error while running the launcher");
}
