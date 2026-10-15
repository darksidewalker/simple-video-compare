//! mpv owns GPU rendering inside a native child; WebView2 only draws the controls.
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicIsize, Ordering},
    Arc, Mutex,
};
use tauri::{Manager, WindowEvent};
use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    System::LibraryLoader::{LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR},
    UI::{
        HiDpi::GetDpiForWindow,
        WindowsAndMessaging::{
            CreateWindowExW, GetClientRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER, WS_CHILD,
            WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_VISIBLE,
        },
    },
};

type Ptr = *mut c_void;
struct Api {
    _library: libloading::Library,
    create: unsafe extern "C" fn() -> Ptr,
    initialize: unsafe extern "C" fn(Ptr) -> c_int,
    option: unsafe extern "C" fn(Ptr, *const c_char, *const c_char) -> c_int,
    destroy: unsafe extern "C" fn(Ptr),
    error: unsafe extern "C" fn(c_int) -> *const c_char,
}
impl Api {
    unsafe fn load(path: &Path) -> Result<Self, String> {
        // Never search the working directory or PATH, including for dependent DLLs.
        let library: libloading::Library = libloading::os::windows::Library::load_with_flags(
            path,
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
        )
        .map_err(|e| e.to_string())?
        .into();
        Ok(Self {
            create: *library.get(b"mpv_create\0").map_err(|e| e.to_string())?,
            initialize: *library
                .get(b"mpv_initialize\0")
                .map_err(|e| e.to_string())?,
            option: *library
                .get(b"mpv_set_option_string\0")
                .map_err(|e| e.to_string())?,
            destroy: *library
                .get(b"mpv_terminate_destroy\0")
                .map_err(|e| e.to_string())?,
            error: *library
                .get(b"mpv_error_string\0")
                .map_err(|e| e.to_string())?,
            _library: library,
        })
    }
    fn check(&self, result: c_int) -> Result<(), String> {
        if result < 0 {
            Err(unsafe { CStr::from_ptr((self.error)(result)) }
                .to_string_lossy()
                .into_owned())
        } else {
            Ok(())
        }
    }
}

fn dll_candidates(override_path: Option<PathBuf>, exe: &Path, managed: Option<PathBuf>) -> Result<Vec<PathBuf>, String> {
    if let Some(path) = override_path {
        if !path.is_absolute() {
            return Err("DAWVC_LIBMPV must be an absolute DLL path".into());
        }
        return Ok(vec![path]);
    }
    let dir = exe.parent().ok_or("No executable directory")?;
    let mut candidates = Vec::new();
    if let Some(managed) = managed { candidates.push(managed.join("libmpv-2.dll")); }
    candidates.extend([dir.join("libmpv-2.dll"), dir.join("lib/libmpv-2.dll")]);
    Ok(candidates)
}

fn resolve_api() -> Result<(PathBuf, Api), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let candidates = dll_candidates(std::env::var_os("DAWVC_LIBMPV").map(PathBuf::from), &exe, crate::runtime::installed_dir())?;
        let mut loaded = None;
        let mut errors = vec![];
        for path in candidates {
            match unsafe { Api::load(&path) } {
                Ok(api) => {
                    loaded = Some((path, api));
                    break;
                }
                Err(error) => errors.push(format!("{}: {error}", path.display())),
            }
        }
        loaded.ok_or_else(|| format!("Cannot load libmpv: {}", errors.join("; ")))
}
pub fn check_library() -> (Option<String>, Result<String, String>) {
    let (path, api) = match resolve_api() {
        Ok(loaded) => loaded,
        Err(error) => return (std::env::var_os("DAWVC_LIBMPV").map(|p| p.to_string_lossy().into_owned()), Err(error)),
    };
    let result = (|| {
        let handle = unsafe { (api.create)() };
        if handle.is_null() { return Err("mpv_create failed".into()); }
        let instance = Runtime { api, handle: handle as usize };
        for (key, value) in [("config", "no"), ("vo", "null"), ("ao", "null")] {
            let key = CString::new(key).unwrap();
            let value = CString::new(value).unwrap();
            instance.api.check(unsafe { (instance.api.option)(handle, key.as_ptr(), value.as_ptr()) })?;
        }
        instance.api.check(unsafe { (instance.api.initialize)(handle) })?;
        Ok("Library exports loaded; isolated libmpv initialization succeeded".into())
    })();
    (Some(path.to_string_lossy().into_owned()), result)
}

struct Runtime {
    api: Api,
    handle: usize,
}
impl Drop for Runtime {
    fn drop(&mut self) {
        // The mutex serializes all client lifecycle calls; mpv owns its own video thread.
        unsafe {
            (self.api.destroy)(self.handle as Ptr);
        }
    }
}

pub struct Player {
    pub endpoint: String,
    /// The child HWND is installed, not necessarily a loaded DLL or connected pipe.
    pub ready: AtomicBool,
    pub error: Mutex<Option<String>>,
    hwnd: AtomicIsize,
    runtime: Mutex<Option<Runtime>>,
}
impl Player {
    fn new() -> Arc<Self> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Arc::new(Self {
            endpoint: format!(r"\\.\pipe\dawvc-libmpv-{}-{nonce}", std::process::id()),
            ready: AtomicBool::new(false),
            error: Mutex::new(None),
            hwnd: AtomicIsize::new(0),
            runtime: Mutex::new(None),
        })
    }
    /// Call on an engine worker before connecting IPC; restart after each Session drop.
    pub fn start(&self) -> Result<(), String> {
        let result = self.start_inner();
        *self.error.lock().unwrap_or_else(|e| e.into_inner()) = result.as_ref().err().cloned();
        result
    }
    fn start_inner(&self) -> Result<(), String> {
        if !self.ready.load(Ordering::Acquire) {
            return Err(self
                .error
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
                .unwrap_or_else(|| "Embedded Windows pane is not ready".into()));
        }
        let mut runtime = self.runtime.lock().map_err(|e| e.to_string())?;
        if runtime.is_some() {
            return Ok(());
        }
        let (_, api) = resolve_api()?;
        let handle = unsafe { (api.create)() };
        if handle.is_null() {
            return Err("mpv_create failed".into());
        }
        let instance = Runtime {
            api,
            handle: handle as usize,
        };
        let wid = self.hwnd.load(Ordering::Acquire).to_string();
        for (key, value) in [
            ("config", "no"),
            ("vo", "gpu"),
            ("wid", wid.as_str()),
            ("hwdec", "auto-copy"),
            ("idle", "yes"),
            ("keep-open", "yes"),
            ("pause", "yes"),
            ("input-terminal", "no"),
            ("input-default-bindings", "no"),
            ("osc", "no"),
            ("input-ipc-server", self.endpoint.as_str()),
        ] {
            let name = CString::new(key).map_err(|e| e.to_string())?;
            let value = CString::new(value).map_err(|e| e.to_string())?;
            instance
                .api
                .check(unsafe { (instance.api.option)(handle, name.as_ptr(), value.as_ptr()) })
                .map_err(|e| format!("libmpv option {key}: {e}"))?;
        }
        instance
            .api
            .check(unsafe { (instance.api.initialize)(handle) })
            .map_err(|e| format!("libmpv initialize: {e}"))?;
        *runtime = Some(instance);
        Ok(())
    }
    /// Call before joining the pipe reader: destroying mpv closes the server pipe.
    pub fn shutdown(&self) {
        self.runtime
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn pane_heights(height: i32, dpi: u32) -> (i32, i32) {
    let height = height.max(0);
    let controls = (440.0 * dpi.max(96) as f64 / 96.0).round() as i32;
    let controls = controls.min(height);
    (height - controls, controls)
}

fn layout(
    window: &tauri::WebviewWindow,
    player: Arc<Player>,
    parent: isize,
    create: bool,
) -> tauri::Result<()> {
    window.with_webview(move |platform| {
        // This closure runs on the UI thread: no Tauri dispatching APIs may be called here.
        let result = (|| -> Result<(), String> {
            let parent = parent as HWND;
            let mut rect = RECT::default();
            if unsafe { GetClientRect(parent, &mut rect) } == 0 {
                return Err(format!(
                    "Read client bounds: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let (video, controls) = pane_heights(rect.bottom, unsafe { GetDpiForWindow(parent) });
            if create {
                let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
                let hwnd = unsafe {
                    CreateWindowExW(
                        0,
                        class.as_ptr(),
                        std::ptr::null(),
                        WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS | 4, // SS_BLACKRECT
                        0,
                        0,
                        rect.right,
                        video,
                        parent,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null(),
                    )
                };
                if hwnd.is_null() {
                    return Err(format!(
                        "Create video pane: {}",
                        std::io::Error::last_os_error()
                    ));
                }
                player.hwnd.store(hwnd as isize, Ordering::Release);
            }
            let hwnd = player.hwnd.load(Ordering::Acquire) as HWND;
            if hwnd.is_null() {
                return Err("Native video pane is unavailable".into());
            }
            if unsafe {
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    rect.right,
                    video,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                )
            } == 0
            {
                return Err(format!(
                    "Resize video pane: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let controller = platform.controller();
            let mut bounds = Default::default();
            unsafe { controller.Bounds(&mut bounds) }.map_err(|e| e.to_string())?;
            bounds.left = 0;
            bounds.top = video;
            bounds.right = rect.right;
            bounds.bottom = video + controls;
            unsafe { controller.SetBounds(bounds) }.map_err(|e| e.to_string())?;
            player.ready.store(true, Ordering::Release);
            Ok(())
        })();
        if let Err(error) = result {
            player.ready.store(false, Ordering::Release);
            *player.error.lock().unwrap_or_else(|e| e.into_inner()) = Some(error);
        }
    })
}

pub fn install(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let window = app
        .get_webview_window("main")
        .ok_or("Main window missing")?;
    let parent = window.hwnd()?.0 as isize;
    // Disable Wry's full-window resize before taking ownership of controller Bounds.
    window.as_ref().set_auto_resize(false)?;
    let player = Player::new();
    app.state::<crate::native::NativeState>()
        .set_embedded(player.clone());
    layout(&window, player.clone(), parent, true)?;
    let events_window = window.clone();
    window.on_window_event(move |event| {
        if matches!(
            event,
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. }
        ) {
            if let Err(error) = layout(&events_window, player.clone(), parent, false) {
                *player.error.lock().unwrap_or_else(|e| e.into_inner()) = Some(error.to_string());
            }
        } else if matches!(event, WindowEvent::Destroyed) {
            player.ready.store(false, Ordering::Release);
            player.hwnd.store(0, Ordering::Release);
            // Parent HWND destruction owns child cleanup; app shutdown owns the mpv lifecycle.
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a downloaded Windows libmpv DLL"]
    fn downloaded_library_initializes_without_a_player_window() {
        let path = std::env::var_os("DAWVC_RUNTIME_VERIFY_LIB").expect("Set DAWVC_RUNTIME_VERIFY_LIB");
        let api = unsafe { Api::load(Path::new(&path)) }.unwrap();
        let handle = unsafe { (api.create)() };
        assert!(!handle.is_null());
        let instance = Runtime { api, handle: handle as usize };
        for (key, value) in [("vo", "null"), ("ao", "null"), ("config", "no")] {
            let key = CString::new(key).unwrap();
            let value = CString::new(value).unwrap();
            instance.api.check(unsafe { (instance.api.option)(handle, key.as_ptr(), value.as_ptr()) }).unwrap();
        }
        instance.api.check(unsafe { (instance.api.initialize)(handle) }).unwrap();
    }
    #[test]
    fn controls_scale_and_never_make_video_negative() {
        assert_eq!(pane_heights(1080, 96), (640, 440));
        assert_eq!(pane_heights(1500, 144), (840, 660));
        assert_eq!(pane_heights(300, 192), (0, 300));
        assert_eq!(pane_heights(0, 96), (0, 0));
    }
    #[test]
    fn dll_resolution_is_explicit_and_absolute() {
        let exe = Path::new(r"C:\app\compare.exe");
        assert!(dll_candidates(Some("libmpv-2.dll".into()), exe, None).is_err());
        assert_eq!(
            dll_candidates(None, exe, None).unwrap(),
            vec![
                PathBuf::from(r"C:\app\libmpv-2.dll"),
                PathBuf::from(r"C:\app\lib\libmpv-2.dll"),
            ]
        );
        assert_eq!(
            dll_candidates(Some(r"D:\runtime\libmpv-2.dll".into()), exe, Some(r"D:\managed".into())).unwrap(),
            vec![PathBuf::from(r"D:\runtime\libmpv-2.dll")]
        );
        assert_eq!(dll_candidates(None, exe, Some(r"D:\managed".into())).unwrap()[0], PathBuf::from(r"D:\managed\libmpv-2.dll"));
    }
}
