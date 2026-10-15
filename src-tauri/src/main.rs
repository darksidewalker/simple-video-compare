#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
mod embedded;
#[cfg(windows)]
#[path = "embedded_windows.rs"]
mod embedded;
mod native;
use tauri::{Manager, RunEvent};

fn main() {
    // WebKitGTK explicit sync fails on some Wayland compositors; only the controls use WebKit.
    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none()
    {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    tauri::Builder::default()
        .manage(native::NativeState::default())
        .invoke_handler(tauri::generate_handler![
            native::native_open,
            native::native_control,
            native::native_status,
            native::native_probe,
            native::native_pick,
        ])
        .build(tauri::generate_context!())
        .expect("error while building desktop application")
        .run(|app, event| {
            #[cfg(target_os = "linux")]
            if let RunEvent::Ready = event {
                embedded::install(app).expect("Linux embedded renderer initialization failed");
            }
            #[cfg(windows)]
            if let RunEvent::Ready = event {
                if let Err(error) = embedded::install(app) {
                    eprintln!("Windows embedded renderer initialization failed: {error}");
                }
            }
            if let RunEvent::Exit = event {
                app.state::<native::NativeState>().shutdown();
            }
        });
}
