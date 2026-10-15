//! Linux pixels stay in GTK's framebuffer; the existing JSON IPC stays on workers.
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicBool, AtomicPtr, Ordering},
    Arc,
};
use tauri::Manager;

type Ptr = *mut c_void;
#[repr(C)]
struct Param {
    kind: c_int,
    data: Ptr,
}
#[repr(C)]
struct Init {
    get: unsafe extern "C" fn(Ptr, *const c_char) -> Ptr,
    ctx: Ptr,
}
#[repr(C)]
struct Fbo {
    id: c_int,
    width: c_int,
    height: c_int,
    format: c_int,
}
macro_rules! api {
    ($($name:ident: $ty:ty),* $(,)?) => {
        struct Api { _library: libloading::Library, $($name: $ty),* }
        impl Api {
            unsafe fn load(path: &std::path::Path) -> Result<Self, String> {
                let library = libloading::Library::new(path).map_err(|e| e.to_string())?;
                $(let $name = *library.get::<$ty>(concat!(stringify!($name), "\0").as_bytes()).map_err(|e| e.to_string())?;)*
                Ok(Self { _library: library, $($name),* })
            }
        }
    }
}
api! {
    mpv_create: unsafe extern "C" fn() -> Ptr,
    mpv_initialize: unsafe extern "C" fn(Ptr) -> c_int,
    mpv_set_option_string: unsafe extern "C" fn(Ptr, *const c_char, *const c_char) -> c_int,
    mpv_terminate_destroy: unsafe extern "C" fn(Ptr),
    mpv_error_string: unsafe extern "C" fn(c_int) -> *const c_char,
    mpv_render_context_create: unsafe extern "C" fn(*mut Ptr, Ptr, *mut Param) -> c_int,
    mpv_render_context_render: unsafe extern "C" fn(Ptr, *mut Param) -> c_int,
    mpv_render_context_free: unsafe extern "C" fn(Ptr),
    mpv_render_context_set_update_callback: unsafe extern "C" fn(Ptr, Option<unsafe extern "C" fn(Ptr)>, Ptr),
}
#[link(name = "EGL")]
extern "C" {
    fn eglGetProcAddress(name: *const c_char) -> Ptr;
}
#[link(name = "GL")]
extern "C" {
    fn glGetIntegerv(name: u32, result: *mut c_int);
    fn glGetString(name: u32) -> *const c_char;
    fn glBindFramebuffer(target: u32, id: u32);
    fn glReadPixels(x: c_int, y: c_int, w: c_int, h: c_int, format: u32, kind: u32, data: Ptr);
    fn glGetError() -> u32;
    fn glGenTextures(n: c_int, textures: *mut u32);
    fn glDeleteTextures(n: c_int, textures: *const u32);
    fn glBindTexture(target: u32, texture: u32);
    fn glTexImage2D(target: u32, level: c_int, format: c_int, width: c_int, height: c_int, border: c_int, pixel_format: u32, kind: u32, data: *const c_void);
    fn glGenFramebuffers(n: c_int, buffers: *mut u32);
    fn glDeleteFramebuffers(n: c_int, buffers: *const u32);
    fn glFramebufferTexture2D(target: u32, attachment: u32, texture_target: u32, texture: u32, level: c_int);
    fn glCheckFramebufferStatus(target: u32) -> u32;
    fn glBlitFramebuffer(sx0: c_int, sy0: c_int, sx1: c_int, sy1: c_int, dx0: c_int, dy0: c_int, dx1: c_int, dy1: c_int, mask: u32, filter: u32);
    fn glClearColor(r: f32, g: f32, b: f32, a: f32);
    fn glClear(mask: u32);
    fn glDisable(cap: u32);
}
unsafe extern "C" fn get_proc(_: Ptr, name: *const c_char) -> Ptr {
    let address = eglGetProcAddress(name);
    if address.is_null() {
        libc::dlsym(libc::RTLD_DEFAULT, name)
    } else {
        address
    }
}
unsafe extern "C" fn update(data: Ptr) {
    // Only a flag crosses the mpv thread boundary, never a GTK widget.
    let player = &*(data as *const Player);
    player.frame_dirty.store(true, Ordering::Release);
    player.dirty.store(true, Ordering::Release);
}
// Cache a packed A/B frame on the GPU; dragging only blits its two visible regions.
struct SplitFrame { texture: u32, fbo: u32, width: c_int, height: c_int }
impl SplitFrame {
    unsafe fn new(width: c_int, height: c_int) -> Result<Self, String> {
        let mut frame = Self { texture: 0, fbo: 0, width, height };
        glGenTextures(1, &mut frame.texture);
        glBindTexture(0x0de1, frame.texture);
        glTexImage2D(0x0de1, 0, 0x8058, width * 2, height, 0, 0x1908, 0x1401, std::ptr::null());
        glGenFramebuffers(1, &mut frame.fbo);
        glBindFramebuffer(0x8d40, frame.fbo);
        glFramebufferTexture2D(0x8d40, 0x8ce0, 0x0de1, frame.texture, 0);
        if glCheckFramebufferStatus(0x8d40) != 0x8cd5 {
            return Err("Cannot allocate the GPU comparison framebuffer".into());
        }
        Ok(frame)
    }
    unsafe fn draw(&self, target: c_int, viewport: (c_int, c_int, c_int, c_int), amount: f64) {
        let (left, bottom, width, height) = viewport;
        let cut = (width as f64 * amount).round() as c_int;
        let source_cut = (self.width as f64 * amount).round() as c_int;
        glBindFramebuffer(0x8ca9, target as u32);
        glDisable(0x0c11); // scissor test: clear letterboxing as well as video.
        glClearColor(0.0, 0.0, 0.0, 1.0);
        glClear(0x4000);
        glBindFramebuffer(0x8ca8, self.fbo);
        if cut > 0 {
            glBlitFramebuffer(0, 0, source_cut, self.height, left, bottom, left + cut, bottom + height, 0x4000, 0x2601);
        }
        if cut < width {
            glBlitFramebuffer(self.width + source_cut, 0, self.width * 2, self.height, left + cut, bottom, left + width, bottom + height, 0x4000, 0x2601);
        }
        glBindFramebuffer(0x8d40, target as u32);
    }
}
impl Drop for SplitFrame {
    fn drop(&mut self) {
        // Owned exclusively by the GTK GL context, including teardown.
        unsafe { glDeleteFramebuffers(1, &self.fbo); glDeleteTextures(1, &self.texture); }
    }
}

pub struct Player {
    api: Api,
    handle: AtomicPtr<c_void>,
    pub endpoint: String,
    dir: PathBuf,
    pub ready: AtomicBool,
    dirty: AtomicBool,
    frame_dirty: AtomicBool,
    split: std::sync::Mutex<Option<(f64, (u32, u32))>>,
    pub error: std::sync::Mutex<Option<String>>,
}
// libmpv permits concurrent client calls; render context stays exclusively on GTK's thread.
unsafe impl Send for Player {}
unsafe impl Sync for Player {}
impl Player {
    pub fn configure_split(&self, split: Option<(f64, (u32, u32))>) {
        *self.split.lock().unwrap() = split;
        self.frame_dirty.store(true, Ordering::Release);
        self.dirty.store(true, Ordering::Release);
    }
    pub fn split_amount(&self) -> Option<f64> {
        self.split.lock().unwrap().map(|s| s.0)
    }
    pub fn set_split_amount(&self, amount: f64) {
        if let Some(split) = self.split.lock().unwrap().as_mut() {
            split.0 = amount;
            self.dirty.store(true, Ordering::Release);
        }
    }

    fn check(&self, result: c_int) -> Result<(), String> {
        if result < 0 {
            unsafe {
                Err(CStr::from_ptr((self.api.mpv_error_string)(result))
                    .to_string_lossy()
                    .into())
            }
        } else {
            Ok(())
        }
    }
    pub fn shutdown(&self) {
        let handle = self.handle.swap(std::ptr::null_mut(), Ordering::AcqRel);
        if !handle.is_null() {
            unsafe {
                (self.api.mpv_terminate_destroy)(handle);
            }
            let _ = std::fs::remove_file(&self.endpoint);
            let _ = std::fs::remove_dir(&self.dir);
            eprintln!("LIBMPV_DESTROYED");
        }
    }
    pub fn new() -> Result<Arc<Self>, String> {
        let candidates = if let Some(path) = std::env::var_os("DAWVC_LIBMPV") {
            vec![PathBuf::from(path)]
        } else {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let dir = exe.parent().ok_or("No executable directory")?;
            vec![
                dir.join("lib/libmpv.so.2"),
                dir.join("libmpv.so.2"),
                PathBuf::from("libmpv.so.2"),
            ]
        };
        let mut loaded = None;
        let mut errors = vec![];
        for path in candidates {
            match unsafe { Api::load(&path) } {
                Ok(api) => {
                    loaded = Some(api);
                    break;
                }
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            }
        }
        let api = loaded.ok_or_else(|| format!("Cannot load libmpv: {}", errors.join("; ")))?;
        // GTK sets the user's locale, but libmpv requires C decimal formatting.
        unsafe {
            libc::setlocale(libc::LC_NUMERIC, b"C\0".as_ptr().cast());
        }
        let handle = unsafe { (api.mpv_create)() };
        if handle.is_null() {
            return Err("mpv_create failed".into());
        }
        let dir = std::env::temp_dir().join(format!("dawvc-libmpv-{}", std::process::id()));
        let player = Self {
            api,
            handle: AtomicPtr::new(handle),
            endpoint: dir
                .join("ipc")
                .to_str()
                .ok_or("IPC path is not UTF-8")?
                .into(),
            dir,
            ready: AtomicBool::new(false),
            dirty: AtomicBool::new(false),
            frame_dirty: AtomicBool::new(true),
            split: std::sync::Mutex::new(None),
            error: std::sync::Mutex::new(None),
        };
        std::fs::create_dir(&player.dir).map_err(|e| e.to_string())?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&player.dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        // shortcut: software decode until native-display hardware decoding is separately verified.
        for (key, value) in [
            ("vo", "libmpv"),
            ("hwdec", "no"),
            ("idle", "yes"),
            ("keep-open", "yes"),
            ("pause", "yes"),
            ("config", "no"),
            ("input-terminal", "no"),
            ("osc", "no"),
            ("input-ipc-server", player.endpoint.as_str()),
        ] {
            let key = CString::new(key).map_err(|e| e.to_string())?;
            let value = CString::new(value).map_err(|e| e.to_string())?;
            player.check(unsafe {
                (player.api.mpv_set_option_string)(handle, key.as_ptr(), value.as_ptr())
            })?;
        }
        player.check(unsafe { (player.api.mpv_initialize)(handle) })?;
        Ok(Arc::new(player))
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        self.shutdown();
    }
}
// mpv fits the comparison frame into the GLArea, including letterboxing.
fn video_rect(width: f64, height: f64, source: (u32, u32)) -> (f64, f64, f64, f64) {
    let scale = (width / source.0.max(1) as f64).min(height / source.1.max(1) as f64);
    let (w, h) = (source.0 as f64 * scale, source.1 as f64 * scale);
    ((width - w) / 2.0, (height - h) / 2.0, w, h)
}

pub fn install(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let player = Player::new()?;
    app.state::<crate::native::NativeState>()
        .set_embedded(player.clone());
    let window = app
        .get_webview_window("main")
        .ok_or("Main window missing")?;
    let state = app.state::<crate::native::NativeState>().inner().clone();
    window.with_webview(move |platform| {
        let web = platform.inner();
        let parent = web.parent().expect("Webview parent").downcast::<gtk::Box>().expect("Tauri GTK box");
        let area = gtk::GLArea::new();
        area.set_required_version(3, 2);
        area.set_auto_render(false);
        area.set_hexpand(true);
        area.set_vexpand(true);
        let context = Rc::new(Cell::new(std::ptr::null_mut()));
        let ctx = context.clone();
        let p = player.clone();
        area.connect_realize(move |area| {
            area.make_current();
            let result = (|| {
                if let Some(error) = area.error() { return Err(error.to_string()); }
                let mut init = Init { get: get_proc, ctx: std::ptr::null_mut() };
                let mut params = [Param { kind: 1, data: b"opengl\0".as_ptr() as Ptr }, Param { kind: 2, data: &mut init as *mut _ as Ptr }, Param { kind: 0, data: std::ptr::null_mut() }];
                let mut ptr = std::ptr::null_mut();
                p.check(unsafe { (p.api.mpv_render_context_create)(&mut ptr, p.handle.load(Ordering::Acquire), params.as_mut_ptr()) })?;
                ctx.set(ptr);
                unsafe { (p.api.mpv_render_context_set_update_callback)(ptr, Some(update), Arc::as_ptr(&p) as Ptr); }
                p.ready.store(true, Ordering::Release);
                unsafe { eprintln!("GLAREA_READY renderer={} version={}", CStr::from_ptr(glGetString(0x1f01)).to_string_lossy(), CStr::from_ptr(glGetString(0x1f02)).to_string_lossy()); }
                Ok::<_, String>(())
            })();
            if let Err(error) = result { eprintln!("GLAREA_ERROR {error}"); *p.error.lock().unwrap() = Some(error); }
        });
        let ctx = context.clone();
        let p = player.clone();
        let frames = Cell::new(0u64);
        let split_frame = Rc::new(RefCell::new(None::<SplitFrame>));
        let cache = split_frame.clone();
        area.connect_render(move |area, _| {
            if ctx.get().is_null() { return gtk::glib::Propagation::Stop; }
            let mut id = 0;
            unsafe { glGetIntegerv(0x8ca6, &mut id); }
            let fbo = Fbo { id, width: area.allocated_width() * area.scale_factor(), height: area.allocated_height() * area.scale_factor(), format: 0 };
            let result = (|| -> Result<(), String> {
                let split = *p.split.lock().unwrap();
                let frame_dirty = p.frame_dirty.swap(false, Ordering::AcqRel);
                let mut cache = cache.borrow_mut();
                let mut target = Fbo { ..fbo };
                let mut viewport = (0, 0, fbo.width, fbo.height);
                let mut redraw = true;
                if let Some((_, source)) = split {
                    let (left, top, width, height) = video_rect(fbo.width as f64, fbo.height as f64, source);
                    viewport = (left.round() as c_int, top.round() as c_int, (width.round() as c_int).max(1), (height.round() as c_int).max(1));
                    let mut limit = 0;
                    unsafe { glGetIntegerv(0x0d33, &mut limit); }
                    let scale = (limit as f64 / (width * 2.0)).min(limit as f64 / height).min(1.0);
                    let (w, h) = (((width * scale).round() as c_int).max(1), ((height * scale).round() as c_int).max(1));
                    let resize = cache.as_ref().map_or(true, |frame| frame.width != w || frame.height != h);
                    if resize { *cache = Some(unsafe { SplitFrame::new(w, h)? }); }
                    let frame = cache.as_ref().unwrap();
                    target = Fbo { id: frame.fbo as c_int, width: w * 2, height: h, format: 0 };
                    redraw = frame_dirty || resize;
                } else {
                    cache.take();
                }
                if redraw {
                    let mut flip = 1;
                    let mut params = [Param { kind: 3, data: &mut target as *mut _ as Ptr }, Param { kind: 4, data: &mut flip as *mut _ as Ptr }, Param { kind: 0, data: std::ptr::null_mut() }];
                    p.check(unsafe { (p.api.mpv_render_context_render)(ctx.get(), params.as_mut_ptr()) })?;
                }
                if let Some((amount, _)) = split {
                    let started = std::time::Instant::now();
                    unsafe { cache.as_ref().unwrap().draw(id, viewport, amount); }
                    if std::env::var_os("DAWVC_RENDER_VERIFY").is_some() {
                        let mut rgb = [0u8; 4];
                        unsafe { glReadPixels(fbo.width / 2, fbo.height / 2, 1, 1, 0x1908, 0x1401, rgb.as_mut_ptr().cast()); }
                        eprintln!("GLAREA_SPLIT amount={amount:.4} source_refresh={redraw} blit_us={} center={rgb:?}", started.elapsed().as_micros());
                    }
                }
                Ok(())
            })();
            if let Err(error) = result { eprintln!("GLAREA_ERROR {error}"); *p.error.lock().unwrap() = Some(error); }
            unsafe { glBindFramebuffer(0x8d40, id as u32); }
            frames.set(frames.get() + 1);
            // Opt-in framebuffer evidence only; no pixels cross playback IPC.
            if std::env::var_os("DAWVC_RENDER_VERIFY").is_some() && frames.get() % 30 == 0 && fbo.width > 0 && fbo.height > 0 {
                let mut pixels = vec![0u8; fbo.width as usize * fbo.height as usize * 4];
                unsafe {
                    glBindFramebuffer(0x8ca8, id as u32);
                    glReadPixels(0, 0, fbo.width, fbo.height, 0x1908, 0x1401, pixels.as_mut_ptr().cast());
                    let error = glGetError();
                    let min = pixels.chunks_exact(4).map(|p| p[0]).min().unwrap();
                    let max = pixels.chunks_exact(4).map(|p| p[0]).max().unwrap();
                    let hash = pixels.iter().fold(0xcbf29ce484222325u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x100000001b3));
                    eprintln!("GLAREA_FRAME count={} fbo={} size={}x{} rgb_min={} rgb_max={} fnv64={hash:016x} gl_error={error}", frames.get(), id, fbo.width, fbo.height, min, max);
                }
            }
            gtk::glib::Propagation::Stop
        });
        let ctx = context;
        let p = player.clone();
        area.connect_unrealize(move |area| {
            p.ready.store(false, Ordering::Release);
            if !ctx.get().is_null() {
                area.make_current();
                split_frame.borrow_mut().take();
                unsafe { (p.api.mpv_render_context_set_update_callback)(ctx.get(), None, std::ptr::null_mut()); (p.api.mpv_render_context_free)(ctx.replace(std::ptr::null_mut())); }
                eprintln!("GLAREA_FREED");
            }
        });
        let weak = area.downgrade();
        let p = player.clone();
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(8), move || {
            let Some(area) = weak.upgrade() else { return gtk::glib::ControlFlow::Break; };
            if p.dirty.swap(false, Ordering::AcqRel) { area.queue_render(); }
            gtk::glib::ControlFlow::Continue
        });
        let overlay = gtk::Overlay::new();
        overlay.add(&area);
        let divider = gtk::DrawingArea::new();
        divider.set_tooltip_text(Some("Drag the A/B divider; arrow keys adjust the split"));
        overlay.add_overlay(&divider);
        overlay.set_overlay_pass_through(&divider, false);
        let split = Rc::new(Cell::new(0.5));
        let source = Rc::new(Cell::new((1, 1)));
        let visible = Rc::new(Cell::new(false));
        let dragging = Rc::new(Cell::new(false));

        let (split_draw, source_draw, visible_draw) = (split.clone(), source.clone(), visible.clone());
        divider.connect_draw(move |widget, cr| {
            if visible_draw.get() {
                let (left, top, w, h) = video_rect(widget.allocated_width() as f64, widget.allocated_height() as f64, source_draw.get());
                let inset = 1.5_f64.min(w / 2.0);
                let x = (left + w * split_draw.get()).clamp(left + inset, left + w - inset);
                cr.set_source_rgba(0.0, 0.0, 0.0, 0.8);
                cr.set_line_width(4.0);
                cr.move_to(x, top); cr.line_to(x, top + h); let _ = cr.stroke();
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.set_line_width(2.0);
                cr.move_to(x, top); cr.line_to(x, top + h); let _ = cr.stroke();
                cr.arc(x, top + h / 2.0, 13.0, 0.0, std::f64::consts::TAU);
                cr.set_source_rgb(0.05, 0.08, 0.12); let _ = cr.fill_preserve();
                cr.set_source_rgb(1.0, 1.0, 1.0); let _ = cr.stroke();
                cr.move_to(x - 7.0, top + h / 2.0); cr.line_to(x + 7.0, top + h / 2.0); let _ = cr.stroke();
            }
            gtk::glib::Propagation::Proceed
        });
        if let Some(accessible) = divider.accessible() {
            accessible.set_name("A/B split");
            accessible.set_description("Drag the divider or use the left and right arrow keys. The A/B split range below provides the same control.");
        }
        divider.set_can_focus(true);
        divider.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK | gtk::gdk::EventMask::BUTTON_RELEASE_MASK | gtk::gdk::EventMask::POINTER_MOTION_MASK | gtk::gdk::EventMask::KEY_PRESS_MASK);
        let (s, dims, v, drag) = (split.clone(), source.clone(), visible.clone(), dragging.clone());
        divider.connect_button_press_event(move |area, event| {
            let (left, top, w, h) = video_rect(area.allocated_width() as f64, area.allocated_height() as f64, dims.get());
            let (x, y) = event.position();
            if event.button() == 1 && v.get() && (x - (left + w * s.get())).abs() <= 16.0 && y >= top && y <= top + h {
                drag.set(true); area.grab_focus(); area.grab_add();
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        let (s, dims, v, drag) = (split.clone(), source.clone(), visible.clone(), dragging.clone());
        let p = player.clone();
        let paint = divider.clone();
        let video = area.clone();
        divider.connect_motion_notify_event(move |area, event| {
            let (left, top, w, h) = video_rect(area.allocated_width() as f64, area.allocated_height() as f64, dims.get());
            let (x, y) = event.position();
            if let Some(window) = area.window() {
                let hover = v.get() && (x - (left + w * s.get())).abs() <= 16.0 && y >= top && y <= top + h;
                window.set_cursor(gtk::gdk::Cursor::from_name(&window.display(), if hover || drag.get() { "col-resize" } else { "default" }).as_ref());
            }
            if drag.get() && w > 0.0 {
                let amount = ((x - left) / w).clamp(0.0, 1.0);
                s.set(amount); p.set_split_amount(amount); video.queue_render(); paint.queue_draw();
            }
            gtk::glib::Propagation::Proceed
        });
        let drag = dragging.clone();
        divider.connect_button_release_event(move |area, event| {
            if event.button() == 1 && drag.replace(false) { area.grab_remove(); }
            gtk::glib::Propagation::Proceed
        });
        let (s, v) = (split.clone(), visible.clone());
        let p = player.clone();
        let paint = divider.clone();
        let video = area.clone();
        divider.connect_key_press_event(move |_, event| {
            let delta = match event.keyval() { gtk::gdk::keys::constants::Left => -0.01, gtk::gdk::keys::constants::Right => 0.01, _ => return gtk::glib::Propagation::Proceed };
            if !v.get() { return gtk::glib::Propagation::Proceed; }
            let amount = (s.get() + delta).clamp(0.0, 1.0);
            s.set(amount); p.set_split_amount(amount); video.queue_render(); paint.queue_draw();
            gtk::glib::Propagation::Stop
        });
        let weak = divider.downgrade();
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
            let Some(divider) = weak.upgrade() else { return gtk::glib::ControlFlow::Break; };
            if let Some((status, dimensions)) = state.overlay_status() {
                    let show = status.running && status.mode == "slider" && status.paths.iter().filter(|p| !p.is_empty()).count() == 2;
                    let changed = visible.get() != show || source.get() != dimensions || (!dragging.get() && split.get() != status.amount);
                    visible.set(show);
                    divider.set_can_focus(show);
                    source.set(dimensions);
                    if !dragging.get() { split.set(status.amount); }
                    if changed { divider.queue_draw(); }
            }
            gtk::glib::ControlFlow::Continue
        });
        parent.pack_start(&overlay, true, true, 0);
        parent.reorder_child(&overlay, 0);
        parent.set_child_packing(&web, false, true, 0, gtk::PackType::Start);
        web.set_vexpand(false);
        web.set_size_request(-1, 360);
        overlay.show_all();
        eprintln!("GLAREA_INSTALLED realized={} mapped={} parent_visible={} size={}x{}", area.is_realized(), area.is_mapped(), parent.is_visible(), area.allocated_width(), area.allocated_height());
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::video_rect;
    #[test]
    fn divider_uses_fitted_video_not_letterbox() {
        assert_eq!(video_rect(1600.0, 540.0, (320, 180)), (320.0, 0.0, 960.0, 540.0));
        assert_eq!(video_rect(640.0, 480.0, (320, 180)), (0.0, 60.0, 640.0, 360.0));
        assert_eq!(video_rect(360.0, 640.0, (180, 320)), (0.0, 0.0, 360.0, 640.0));
    }
}
