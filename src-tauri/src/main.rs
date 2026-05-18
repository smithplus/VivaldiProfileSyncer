#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, State,
};

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Profile {
    id: String,
    name: String,
    path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct SyncLogEntry {
    timestamp: String,
    from: String,
    to: String,
    keys: Vec<String>,
    success: bool,
    message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ExtensionInfo {
    id: String,
    name: String,
    version: String,
    enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AppConfig {
    auto_sync_enabled: bool,
    auto_sync_minutes: u64,
    sync_on_launch: bool,
    login_item_enabled: bool,
    default_from: String,
    default_to: String,
    // Exact list of vivaldi keys that will be synced — what the user explicitly checked.
    // Empty = nothing selected. Used by sync_on_launch and auto-sync to avoid syncing
    // things the user didn't approve.
    selected_keys: Vec<String>,
    advanced_mode: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            auto_sync_enabled: false,
            auto_sync_minutes: 30,
            sync_on_launch: false,
            login_item_enabled: false,
            default_from: String::new(),
            default_to: String::new(),
            selected_keys: Vec::new(),
            advanced_mode: false,
        }
    }
}

struct AppState {
    log: Mutex<Vec<SyncLogEntry>>,
}

// ── Category definitions ──────────────────────────────────────────────────────

const SYNC_CATEGORIES: &[(&str, &str, &str, &str, &[(&str, &str)])] = &[
    (
        "ui_layout", "UI Layout", "Tab bar, toolbar buttons, address bar, and auto-hide rules", "🖥️",
        &[
            ("tabs",        "Tab bar (position, close button side, stacking mode)"),
            ("toolbars",    "Toolbar button layout and order"),
            ("address_bar", "Address bar behavior and search field"),
            ("auto_hide",   "Auto-hide rules for panels, tabs, and address bar"),
        ],
    ),
    (
        "appearance", "Appearance & Themes", "Active theme, saved custom themes, window accent color", "🎨",
        &[
            ("appearance", "Window and UI appearance settings"),
            ("theme",      "Currently active theme"),
            ("themes",     "All saved custom themes"),
        ],
    ),
    (
        "keyboard", "Keyboard Shortcuts", "Custom key bindings, multi-key command chains", "⌨️",
        &[
            ("keyboard",         "Custom keyboard shortcut bindings"),
            ("chained_commands", "Multi-key command chains"),
            ("actions",          "Custom action definitions"),
        ],
    ),
    (
        "menus", "Menus & Context", "Menu bar customization and right-click context menu items", "☰",
        &[
            ("menu",            "Menu bar customization"),
            ("context_dialogs", "Right-click context menu items and dialog preferences"),
        ],
    ),
    (
        "page", "Page & Content", "Default zoom, font rendering, per-site overrides, translation", "📄",
        &[
            ("page",      "Default zoom level and font rendering"),
            ("webpages",  "Per-site overrides and content settings"),
            ("translate", "Translation language preferences"),
        ],
    ),
    (
        "features", "Features & Experiments", "Feature flags, experimental toggles, general preference switches", "🧪",
        &[
            ("features", "Feature flags and experimental options"),
            ("settings", "General Vivaldi preference switches"),
            ("list",     "List view and display preferences"),
        ],
    ),
    (
        "downloads", "Downloads", "Default download folder and download behavior", "⬇️",
        &[
            ("downloads", "Download folder, ask-before-download, open-after-download"),
        ],
    ),
    (
        "web_panels", "Web Panels", "Saved web panels list, URLs, width, reload behavior", "📌",
        &[
            ("panels", "All saved web panels and their configuration"),
        ],
    ),
    (
        "workspaces", "Workspaces", "Workspace names, icons, and tab groupings", "🗂️",
        &[
            ("workspaces", "Workspace names, icons, and layout groupings"),
        ],
    ),
    (
        "startup", "Startup & Homepage", "What opens on launch and the homepage URL", "🏠",
        &[
            ("startup",  "Startup behavior (last session / specific pages / speed dial)"),
            ("homepage", "Homepage URL"),
        ],
    ),
    (
        "search", "Search Engine", "Default search engine for address bar and private search", "🔍",
        &[
            ("root:default_search_provider",      "Active search engine selection"),
            ("root:default_search_provider_data", "Search engine URL templates and settings"),
        ],
    ),
];

// ── Paths ─────────────────────────────────────────────────────────────────────

fn vivaldi_path() -> PathBuf {
    dirs::home_dir().unwrap().join("Library/Application Support/Vivaldi")
}

fn config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap()
        .join("Library/Application Support/app.tabella.vivaldi-sync/config.json")
}

// ── Commands ──────────────────────────────────────────────────────────────────

#[tauri::command]
fn is_vivaldi_running() -> bool {
    std::process::Command::new("pgrep")
        .args(["-x", "Vivaldi"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tauri::command]
fn get_profiles() -> Result<Vec<Profile>, String> {
    let base = vivaldi_path();
    let content = fs::read_to_string(base.join("Local State"))
        .map_err(|e| format!("Cannot read Local State: {}", e))?;
    let state: Value = serde_json::from_str(&content)
        .map_err(|e| format!("Cannot parse Local State: {}", e))?;

    let info_cache = state
        .get("profile")
        .and_then(|p| p.get("info_cache"))
        .and_then(|c| c.as_object())
        .ok_or("No profile info_cache found")?;

    let mut profiles = Vec::new();
    for (folder, info) in info_cache {
        let name = info.get("name").and_then(|n| n.as_str()).unwrap_or(folder).to_string();
        let path = base.join(folder);
        if path.join("Preferences").exists() {
            profiles.push(Profile {
                id: folder.clone(),
                name,
                path: path.to_string_lossy().to_string(),
            });
        }
    }
    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(profiles)
}

#[tauri::command]
fn get_categories() -> Vec<serde_json::Value> {
    SYNC_CATEGORIES.iter().map(|(id, label, desc, icon, sub_keys)| {
        let subs: Vec<serde_json::Value> = sub_keys.iter().map(|(key, key_label)| {
            serde_json::json!({ "key": key, "label": key_label })
        }).collect();
        serde_json::json!({ "id": id, "label": label, "desc": desc, "icon": icon, "subKeys": subs })
    }).collect()
}

#[tauri::command]
fn sync_profiles(
    from_id: String,
    to_id: String,
    keys: Vec<String>,
    dry_run: bool,
    state: State<AppState>,
) -> Result<SyncLogEntry, String> {
    let base = vivaldi_path();
    let from_prefs = base.join(&from_id).join("Preferences");
    let to_prefs   = base.join(&to_id).join("Preferences");

    let from_json: Value = serde_json::from_str(
        &fs::read_to_string(&from_prefs).map_err(|e| format!("Cannot read source: {}", e))?
    ).map_err(|e| format!("Cannot parse source: {}", e))?;

    let mut to_json: Value = serde_json::from_str(
        &fs::read_to_string(&to_prefs).map_err(|e| format!("Cannot read destination: {}", e))?
    ).map_err(|e| format!("Cannot parse destination: {}", e))?;

    let from_viv = from_json.get("vivaldi").ok_or("Source has no vivaldi settings")?.clone();
    let mut copied = Vec::new();

    if let Some(to_viv) = to_json.get_mut("vivaldi").and_then(|v| v.as_object_mut()) {
        for key in keys.iter().filter(|k| !k.starts_with("root:")) {
            if let Some(val) = from_viv.get(key.as_str()) {
                to_viv.insert(key.clone(), val.clone());
                copied.push(key.clone());
            }
        }
    }
    for key in keys.iter().filter(|k| k.starts_with("root:")) {
        let real_key = key.trim_start_matches("root:");
        if let Some(val) = from_json.get(real_key) {
            if let Some(obj) = to_json.as_object_mut() {
                obj.insert(real_key.to_string(), val.clone());
                copied.push(real_key.to_string());
            }
        }
    }

    if !dry_run && !copied.is_empty() {
        let backup = to_prefs.with_extension(
            format!("bak.{}", chrono::Local::now().format("%Y%m%d_%H%M%S"))
        );
        fs::copy(&to_prefs, &backup).map_err(|e| format!("Backup failed: {}", e))?;
        fs::write(&to_prefs, serde_json::to_string(&to_json).unwrap())
            .map_err(|e| format!("Write failed: {}", e))?;
    }

    let entry = SyncLogEntry {
        timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        from: from_id,
        to: to_id,
        keys: copied.clone(),
        success: true,
        message: if dry_run {
            format!("Dry run: would copy {} keys", copied.len())
        } else {
            format!("Synced {} keys", copied.len())
        },
    };
    state.log.lock().unwrap().push(entry.clone());
    Ok(entry)
}

#[tauri::command]
fn get_log(state: State<AppState>) -> Vec<SyncLogEntry> {
    state.log.lock().unwrap().clone()
}

#[tauri::command]
fn list_extensions(profile_id: String) -> Result<Vec<ExtensionInfo>, String> {
    let base = vivaldi_path();
    let ext_dir = base.join(&profile_id).join("Extensions");
    let prefs_path = base.join(&profile_id).join("Preferences");

    if !ext_dir.exists() { return Ok(Vec::new()); }

    let prefs: Value = serde_json::from_str(
        &fs::read_to_string(&prefs_path).map_err(|e| format!("Cannot read Preferences: {}", e))?
    ).map_err(|e| format!("Cannot parse Preferences: {}", e))?;

    let ext_settings = prefs.get("extensions").and_then(|e| e.get("settings"))
        .and_then(|s| s.as_object()).cloned().unwrap_or_default();

    let mut extensions = Vec::new();

    for entry in fs::read_dir(&ext_dir).map_err(|e| e.to_string())?.flatten() {
        let ext_id = entry.file_name().to_string_lossy().to_string();
        if ext_id.starts_with('.') { continue; }

        let ext_path = entry.path();
        let version_dirs: Vec<_> = fs::read_dir(&ext_path)
            .ok().into_iter().flatten().flatten()
            .filter(|e| e.path().is_dir()).collect();
        if version_dirs.is_empty() { continue; }

        let version = version_dirs[0].file_name().to_string_lossy().to_string();
        let manifest_path = ext_path.join(&version).join("manifest.json");
        if !manifest_path.exists() { continue; }

        let Ok(manifest_str) = fs::read_to_string(&manifest_path) else { continue };
        let Ok(manifest): Result<Value, _> = serde_json::from_str(&manifest_str) else { continue };

        let raw_name = manifest.get("name").and_then(|n| n.as_str()).unwrap_or(&ext_id).to_string();
        let name = if raw_name.starts_with("__MSG_") {
            let key = raw_name.trim_start_matches("__MSG_").trim_end_matches("__");
            resolve_msg_name(&ext_path.join(&version), key).unwrap_or(raw_name)
        } else { raw_name };

        let enabled = ext_settings.get(&ext_id)
            .and_then(|s| s.get("state")).and_then(|s| s.as_i64())
            .map(|s| s == 1).unwrap_or(true);

        extensions.push(ExtensionInfo { id: ext_id, name, version, enabled });
    }

    extensions.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(extensions)
}

fn resolve_msg_name(ext_version_path: &PathBuf, key: &str) -> Option<String> {
    for lang in &["en", "en_US", "es"] {
        let msg_path = ext_version_path.join("_locales").join(lang).join("messages.json");
        if let Ok(content) = fs::read_to_string(&msg_path) {
            if let Ok(msgs) = serde_json::from_str::<Value>(&content) {
                if let Some(obj) = msgs.as_object() {
                    for (k, v) in obj {
                        if k.to_lowercase() == key.to_lowercase() {
                            if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
                                return Some(msg.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

#[tauri::command]
fn copy_extensions(
    ext_ids: Vec<String>,
    from_id: String,
    to_id: String,
    dry_run: bool,
) -> Result<Vec<String>, String> {
    let base = vivaldi_path();
    let from_ext = base.join(&from_id).join("Extensions");
    let to_ext   = base.join(&to_id).join("Extensions");
    let to_prefs_path = base.join(&to_id).join("Preferences");

    let from_prefs: Value = serde_json::from_str(
        &fs::read_to_string(base.join(&from_id).join("Preferences")).map_err(|e| format!("Cannot read source Preferences: {}", e))?
    ).map_err(|e| format!("Cannot parse source Preferences: {}", e))?;

    let mut to_prefs: Value = serde_json::from_str(
        &fs::read_to_string(&to_prefs_path).map_err(|e| format!("Cannot read dest Preferences: {}", e))?
    ).map_err(|e| format!("Cannot parse dest Preferences: {}", e))?;

    let from_ext_settings = from_prefs.get("extensions").and_then(|e| e.get("settings"))
        .and_then(|s| s.as_object()).cloned().unwrap_or_default();

    let mut copied = Vec::new();

    for ext_id in &ext_ids {
        let src = from_ext.join(ext_id);
        let dst = to_ext.join(ext_id);
        if !src.exists() { continue; }
        if dst.exists() { copied.push(format!("{} (already exists)", ext_id)); continue; }
        if !dry_run {
            copy_dir_all(&src, &dst).map_err(|e| format!("Failed to copy {}: {}", ext_id, e))?;
            if let Some(ext_cfg) = from_ext_settings.get(ext_id) {
                if let Some(to_settings) = to_prefs.get_mut("extensions")
                    .and_then(|e| e.get_mut("settings")).and_then(|s| s.as_object_mut()) {
                    to_settings.insert(ext_id.clone(), ext_cfg.clone());
                }
            }
        }
        copied.push(ext_id.clone());
    }

    if !dry_run {
        let backup = to_prefs_path.with_extension(
            format!("bak.{}", chrono::Local::now().format("%Y%m%d_%H%M%S"))
        );
        fs::copy(&to_prefs_path, &backup).map_err(|e| format!("Backup failed: {}", e))?;
        fs::write(&to_prefs_path, serde_json::to_string(&to_prefs).unwrap())
            .map_err(|e| format!("Write failed: {}", e))?;
    }
    Ok(copied)
}

fn copy_dir_all(src: &PathBuf, dst: &PathBuf) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let dst_path = dst.join(entry.file_name());
        if entry.path().is_dir() { copy_dir_all(&entry.path(), &dst_path)?; }
        else { fs::copy(entry.path(), dst_path)?; }
    }
    Ok(())
}

#[tauri::command]
fn load_config() -> AppConfig {
    let path = config_path();
    if let Ok(content) = fs::read_to_string(&path) {
        serde_json::from_str(&content).unwrap_or_default()
    } else { AppConfig::default() }
}

#[tauri::command]
fn save_config(config: AppConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).map_err(|e| e.to_string())
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle, config: AppConfig) {
    let path = config_path();
    if let Some(parent) = path.parent() { let _ = fs::create_dir_all(parent); }
    let _ = fs::write(&path, serde_json::to_string_pretty(&config).unwrap());
    app.exit(0);
}

#[tauri::command]
fn shell_open(url: String) -> Result<(), String> {
    std::process::Command::new("open").arg(&url).spawn()
        .map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_login_item(enable: bool) -> Result<(), String> {
    let app_path_output = std::process::Command::new("mdfind")
        .args(["kMDItemCFBundleIdentifier == 'app.tabella.vivaldi-sync'"])
        .output()
        .map_err(|e| e.to_string())?;
    let app_path = String::from_utf8_lossy(&app_path_output.stdout)
        .lines().next().unwrap_or("").trim().to_string();

    if app_path.is_empty() {
        return Err("App not found via Spotlight. Try launching from /Applications first.".into());
    }

    let script = if enable {
        format!(r#"tell application "System Events" to make login item at end with properties {{path:"{}", hidden:true}}"#, app_path)
    } else {
        format!(r#"tell application "System Events" to delete login item "Vivaldi Sync""#)
    };

    std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn is_already_running() -> bool {
    let my_pid = std::process::id();
    let output = std::process::Command::new("pgrep")
        .args(["-x", "vivaldi-sync"])
        .output();
    if let Ok(out) = output {
        let pids: Vec<u32> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| l.trim().parse().ok())
            .collect();
        // Running if there's another PID besides ours
        pids.iter().any(|&p| p != my_pid)
    } else {
        false
    }
}

fn main() {
    if is_already_running() {
        // Bring the existing window to front and exit
        let _ = std::process::Command::new("open")
            .args(["-a", "Vivaldi Sync"])
            .spawn();
        return;
    }

    tauri::Builder::default()
        .manage(AppState { log: Mutex::new(Vec::new()) })
        .setup(|app| {
            // Build tray menu
            let open_i  = MenuItem::with_id(app, "open",  "Open Vivaldi Sync", true, None::<&str>)?;
            let sync_i  = MenuItem::with_id(app, "sync",  "Sync Now",          true, None::<&str>)?;
            let sep     = PredefinedMenuItem::separator(app)?;
            let quit_i  = MenuItem::with_id(app, "quit",  "Quit",              true, None::<&str>)?;
            let menu    = Menu::with_items(app, &[&open_i, &sync_i, &sep, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .tooltip("Vivaldi Sync")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "sync" => {
                        // Emit to frontend to trigger sync
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.emit("tray-sync-now", ());
                        }
                    }
                    "quit" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.emit("tray-quit", ());
                        } else {
                            app.exit(0);
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up, ..
                    } = event {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Hide window instead of quit on close
            let window = app.get_webview_window("main").unwrap();
            let window_clone = window.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window_clone.emit("window-hiding", ());
                    let _ = window_clone.hide();
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_profiles,
            get_categories,
            sync_profiles,
            get_log,
            is_vivaldi_running,
            list_extensions,
            copy_extensions,
            load_config,
            save_config,
            quit_app,
            shell_open,
            set_login_item,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
