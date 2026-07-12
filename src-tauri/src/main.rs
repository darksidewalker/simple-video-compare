#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Manager;
use serde_json::json;

struct AppState {
    media_cache: HashMap<String, Vec<u8>>,
}

impl Clone for AppState {
    fn clone(&self) -> Self {
        AppState {
            media_cache: self.media_cache.clone(),
        }
    }
}

// ── Fullscreen commands ───────────────────────────────────────────────
#[tauri::command]
fn enter_fullscreen(window: tauri::Window) -> Result<(), String> {
    window.set_fullscreen(true).map_err(|e| e.to_string())
}

#[tauri::command]
fn exit_fullscreen(window: tauri::Window) -> Result<(), String> {
    window.set_fullscreen(false).map_err(|e| e.to_string())
}

// ── Health & runtime ──────────────────────────────────────────────────
#[tauri::command]
fn handle_health() -> Result<serde_json::Value, String> {
    Ok(json!({"status":"ok"}))
}

#[tauri::command]
fn handle_runtime() -> Result<serde_json::Value, String> {
    Ok(json!({
        "ux_mode": "local-app-window",
        "version": env!("CARGO_PKG_VERSION"),
        "tools": {"ready": true}
    }))
}

// ── File browsing ─────────────────────────────────────────────────────
#[tauri::command]
fn handle_browse(path: Option<String>) -> Result<serde_json::Value, String> {
    let base = path.unwrap_or_else(|| ".".to_string());
    let entries = std::fs::read_dir(&base)
        .map_err(|e| format!("Cannot read {}: {}", base, e))?;

    let mut items = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') { continue; }
        let meta = entry.metadata().map_err(|e| e.to_string())?;
        let is_dir = meta.is_dir();
        items.push(json!({
            "name": name,
            "path": entry.path().to_str().unwrap_or("").to_string(),
            "is_dir": is_dir,
            "size": meta.len(),
        }));
    }
    items.sort_by(|a, b| {
        let a_is_dir = a["is_dir"].as_bool().unwrap_or(false);
        let b_is_dir = b["is_dir"].as_bool().unwrap_or(false);
        b_is_dir.cmp(&a_is_dir).then(a["name"].as_str().cmp(&b["name"].as_str()))
    });

    Ok(json!({"path": base, "items": items}))
}

// ── Video probe (ffprobe) ─────────────────────────────────────────────
#[tauri::command]
fn handle_video_probe(path: String) -> Result<serde_json::Value, String> {
    let output = std::process::Command::new("ffprobe")
        .args(["-v", "quiet", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(&path)
        .output()
        .map_err(|e| format!("ffprobe not found: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffprobe error: {}", stderr.trim()));
    }

    let raw: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("JSON parse error: {}", e))?;
    Ok(raw)
}

// ── Media register & cache ────────────────────────────────────────────
fn media_id(path: &str) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(path.as_bytes());
    hex::encode(hasher.finalize())[..16].to_string()
}

#[tauri::command]
fn handle_register_media(app_state: tauri::State<Mutex<AppState>>, path: String) -> Result<serde_json::Value, String> {
    let info = std::fs::metadata(&path)
        .map_err(|_| format!("File not found: {}", path))?;
    if info.is_dir() {
        return Err("Is a directory".into());
    }

    let ext = std::path::Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let supported = ["mp4", "mkv", "webm", "mov", "avi", "ts", "m4v", "ogv"];
    if !supported.contains(&ext.as_str()) {
        return Err(format!("Unsupported extension: .{}", ext));
    }

    let id = media_id(&path);
    let name = std::path::Path::new(&path)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("unknown")
        .to_string();

    let url = format!("/media/{}{}", id, name);

    Ok(json!({
        "id": id,
        "name": name,
        "path": path,
        "url": url,
        "compatible": true,
        "cached": false,
        "cache_bytes": 0,
    }))
}

#[tauri::command]
fn handle_cache_media(app_state: tauri::State<Mutex<AppState>>, id: String) -> Result<serde_json::Value, String> {
    let guard = app_state.lock().map_err(|e| e.to_string())?;
    if let Some(data) = guard.media_cache.get(&id) {
        return Ok(json!({
            "id": id,
            "cached": true,
            "cache_bytes": data.len(),
        }));
    }
    drop(guard);
    Err("ID not registered".into())
}

// ── Static file serving ───────────────────────────────────────────────
#[tauri::command]
fn serve_asset(filename: String) -> Result<String, String> {
    match filename.as_str() {
        "index.html" => Ok(INDEX_HTML.to_string()),
        "css/app.css" => Ok(CSS_APP.to_string()),
        "js/api.js" => Ok(JS_API.to_string()),
        "js/app.js" => Ok(JS_APP.to_string()),
        "js/compare.js" => Ok(JS_COMPARE.to_string()),
        "js/dropzone.js" => Ok(JS_DROPZONE.to_string()),
        "js/filebrowser.js" => Ok(JS_FILEBROWSER.to_string()),
        _ => Err(format!("Asset not found: {}", filename)),
    }
}

// ── App info ──────────────────────────────────────────────────────────
#[tauri::command]
fn get_app_info() -> String {
    json!({
        "name": "DaSiWa Simple Video Compare",
        "version": env!("CARGO_PKG_VERSION"),
        "ux_mode": "local-app-window"
    }).to_string()
}

// ── Embedded web assets ───────────────────────────────────────────────
const INDEX_HTML: &str = include_str!("../../cmd/dasiwa-simple-video-compare/web/index.html");
const CSS_APP: &str = include_str!("../../cmd/dasiwa-simple-video-compare/web/css/app.css");
const JS_API: &str = include_str!("../../cmd/dasiwa-simple-video-compare/web/js/api.js");
const JS_APP: &str = include_str!("../../cmd/dasiwa-simple-video-compare/web/js/app.js");
const JS_COMPARE: &str = include_str!("../../cmd/dasiwa-simple-video-compare/web/js/compare.js");
const JS_DROPZONE: &str = include_str!("../../cmd/dasiwa-simple-video-compare/web/js/dropzone.js");
const JS_FILEBROWSER: &str = include_str!("../../cmd/dasiwa-simple-video-compare/web/js/filebrowser.js");

fn main() {
    tauri::Builder::default()
        .manage(Mutex::new(AppState { media_cache: HashMap::new() }))
        .invoke_handler(tauri::generate_handler![
            enter_fullscreen,
            exit_fullscreen,
            handle_health,
            handle_runtime,
            handle_browse,
            handle_video_probe,
            handle_register_media,
            handle_cache_media,
            serve_asset,
            get_app_info,
        ])
        .setup(move |app_handle| {
            let win = app_handle.get_webview_window("main").unwrap();
            win.set_title("DaSiWa Simple Video Compare")?;
            win.center()?;
            win.set_resizable(true)?;

            Ok(())
        })
        .on_window_event(move |_window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                _window.close().ok();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
