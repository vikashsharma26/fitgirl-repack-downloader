// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use fitdl_core::{AddResult, AppPaths, Config, ItemId, Manager, NewLink};
use fitdl_scraper::Page;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager as _, RunEvent, State, WindowEvent};
use tauri_plugin_opener::OpenerExt;

struct AppState {
    manager: Manager,
    api: Arc<Mutex<ApiStatus>>,
}

#[derive(Clone, Serialize)]
struct ApiStatus {
    port: u16,
    running: bool,
    error: Option<String>,
}

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

#[tauri::command]
fn get_items(state: State<AppState>) -> Value {
    json!({ "items": state.manager.items(), "totals": state.manager.totals() })
}

#[tauri::command]
async fn scrape(state: State<'_, AppState>, url: String) -> CmdResult<Page> {
    state.manager.scrape(url.trim()).await.map_err(err)
}

#[tauri::command]
fn add_links(state: State<AppState>, links: Vec<NewLink>, game: Option<String>) -> AddResult {
    state.manager.add(links, game)
}

#[tauri::command]
fn pause(state: State<AppState>, id: ItemId) {
    state.manager.pause(id);
}

#[tauri::command]
fn resume(state: State<AppState>, id: ItemId) {
    state.manager.resume(id);
}

#[tauri::command]
fn remove(state: State<AppState>, id: ItemId, delete_files: bool) {
    state.manager.remove(id, delete_files);
}

#[tauri::command]
fn pause_all(state: State<AppState>) {
    state.manager.pause_all();
}

#[tauri::command]
fn resume_all(state: State<AppState>) {
    state.manager.resume_all();
}

#[tauri::command]
fn clear_completed(state: State<AppState>) {
    state.manager.clear_completed();
}

#[tauri::command]
fn get_config(state: State<AppState>) -> Config {
    state.manager.config()
}

#[tauri::command]
fn set_config(state: State<AppState>, config: Config) -> CmdResult<Config> {
    state.manager.update_config(config).map_err(err)
}

#[tauri::command]
fn app_info(state: State<AppState>) -> Value {
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "config_path": state.manager.paths().config,
        "api": state.api.lock().unwrap().clone(),
    })
}

/// Show a downloaded file in Explorer, or open its folder.
#[tauri::command]
fn show_item(app: AppHandle, state: State<AppState>, id: ItemId) -> CmdResult<()> {
    let item = state.manager.get(id).ok_or("item not found")?;
    let path = item.path();
    if path.exists() {
        app.opener().reveal_item_in_dir(&path).map_err(err)
    } else {
        std::fs::create_dir_all(&item.dir).map_err(err)?;
        app.opener()
            .open_path(item.dir.to_string_lossy(), None::<&str>)
            .map_err(err)
    }
}

#[tauri::command]
fn open_downloads(app: AppHandle, state: State<AppState>) -> CmdResult<()> {
    let dir = state.manager.config().download_dir;
    std::fs::create_dir_all(&dir).map_err(err)?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(err)
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn quit(app: &AppHandle) {
    let manager = app.state::<AppState>().manager.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        manager.shutdown().await;
        app.exit(0);
    });
}

fn human_speed(bps: u64) -> String {
    let mb = bps as f64 / (1024.0 * 1024.0);
    if mb >= 1.0 {
        format!("{mb:.1} MB/s")
    } else {
        format!("{:.0} KB/s", bps as f64 / 1024.0)
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,fitdl_core=info".into()),
        )
        .init();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app)
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let manager =
                tauri::async_runtime::block_on(async { Manager::start(AppPaths::detect()) })?;

            let port = manager.config().server_port;
            let api = Arc::new(Mutex::new(ApiStatus {
                port,
                running: true,
                error: None,
            }));
            {
                let (manager, api) = (manager.clone(), api.clone());
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = fitdl_core::api::serve(manager, port).await {
                        tracing::error!("{e:#}");
                        let mut s = api.lock().unwrap();
                        s.running = false;
                        s.error = Some(format!("{e:#}"));
                    }
                });
            }

            let show = MenuItem::with_id(app, "show", "Show FitDL", true, None::<&str>)?;
            let resume = MenuItem::with_id(app, "resume_all", "Resume all", true, None::<&str>)?;
            let pause = MenuItem::with_id(app, "pause_all", "Pause all", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&show, &resume, &pause, &sep, &quit_item])?;

            let tray = TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().cloned().expect("bundle icon"))
                .tooltip("FitDL")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    let manager = app.state::<AppState>().manager.clone();
                    match event.id.as_ref() {
                        "show" => show_main(app),
                        "resume_all" => manager.resume_all(),
                        "pause_all" => manager.pause_all(),
                        "quit" => quit(app),
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;

            // Keep the tray tooltip showing live status.
            {
                let manager = manager.clone();
                tauri::async_runtime::spawn(async move {
                    loop {
                        let t = manager.totals();
                        let tip = if t.active > 0 {
                            format!("FitDL: {} downloading, {}", t.active, human_speed(t.speed))
                        } else if t.queued > 0 {
                            format!("FitDL: {} queued", t.queued)
                        } else {
                            "FitDL: idle".to_owned()
                        };
                        let _ = tray.set_tooltip(Some(tip));
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                });
            }

            app.manage(AppState { manager, api });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps downloads running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_items,
            scrape,
            add_links,
            pause,
            resume,
            remove,
            pause_all,
            resume_all,
            clear_completed,
            get_config,
            set_config,
            app_info,
            show_item,
            open_downloads,
        ])
        .build(tauri::generate_context!())
        .expect("failed to start FitDL");

    app.run(|app, event| {
        if let RunEvent::ExitRequested { api, code, .. } = event {
            // Exit requested by the OS (logout/shutdown) rather than our Quit:
            // save progress first.
            if code.is_none() {
                api.prevent_exit();
                quit(app);
            }
        }
    });
}
