use eframe::egui;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct AppState {
    root_dir: PathBuf,
    media_entries: HashMap<String, MediaEntry>,
    current_mode: CompareMode,
    blend_value: f32,
    seek_position: f64,
    playing: bool,
    audio_mode: AudioMode,
    path_a: Option<PathBuf>,
    path_b: Option<PathBuf>,
    show_details: bool,
    show_browser: bool,
    browser_path: PathBuf,
    browser_items: Vec<BrowserItem>,
    drop_zone_hovered: bool,
}

struct MediaEntry {
    id: String,
    name: String,
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct BrowserItem {
    name: String,
    path: PathBuf,
    is_dir: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompareMode { Side, Slider, Blend, Diff }
impl Default for CompareMode { fn default() -> Self { Self::Side } }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioMode { A, B, Mute }

fn clean_path(value: &str, fallback: &str) -> PathBuf {
    let v = if value.is_empty() { fallback } else { value };
    let expanded = if v.starts_with('~') {
        dirs::home_dir().map(|h| h.join(&v[1..])).unwrap_or_else(|| PathBuf::from(v))
    } else { PathBuf::from(v) };
    fs::canonicalize(&expanded).unwrap_or(expanded)
}

fn is_video_ext(name: &str) -> bool {
    matches!(name.to_lowercase().as_str(),
        "mp4"|"mkv"|"mov"|"webm"|"avi"|"m4v"|"mpg"|"mpeg"|"ts"|"m2ts"|"flv"|"ogv")
}

fn compute_sha1_hex(input: &str) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

fn extract_frame(video_path: &str, seconds: f64) -> Option<Vec<u8>> {
    let output = Command::new("ffmpeg")
        .args(["-ss", &seconds.to_string(), "-i", video_path,
               "-frames:v", "1", "-f", "image2pipe", "-vcodec", "mjpeg", "-q:v", "2", "pipe:1"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output();

    match output {
        Ok(out) if out.status.success() => Some(out.stdout),
        Ok(_) => None,
        Err(e) => { eprintln!("ffmpeg not found: {}", e); None }
    }
}

fn register_media(state: &mut AppState, path: PathBuf) -> Result<(), String> {
    let metadata = fs::metadata(&path).map_err(|e| format!("File not found: {}", e))?;
    if metadata.is_dir() { return Err("Not a file".into()); }
    let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
    if !is_video_ext(filename) { return Err("Unsupported video extension".into()); }
    let id = compute_sha1_hex(&path.to_string_lossy())[..16].to_string();
    state.media_entries.insert(id.clone(), MediaEntry {
        id, name: filename.to_string(), path,
    });
    Ok(())
}

fn browse_directory(state: &mut AppState, dir: PathBuf) {
    state.browser_path = dir.clone();
    state.browser_items.clear();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') { continue; }
            let meta = match entry.metadata() { Ok(m) => m, Err(_) => continue };
            if meta.is_dir() || is_video_ext(&name) {
                state.browser_items.push(BrowserItem {
                    name, path: entry.path(), is_dir: meta.is_dir(),
                });
            }
        }
    }
}

/// Try to load a dropped file into the next available slot (A or B).
fn try_load_dropped(state: &mut AppState, file_path: &str) {
    let ext = Path::new(file_path)
        .extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if !is_video_ext(&ext) { return; }
    let pb = PathBuf::from(file_path);
    let media_path = if state.path_a.is_none() {
        state.path_a = Some(pb.clone());
        pb
    } else if state.path_b.is_none() {
        state.path_b = Some(pb.clone());
        pb
    } else {
        state.path_b = Some(pb.clone());
        pb
    };
    let _ = register_media(state, media_path);
}

fn make_texture(ctx: &egui::Context, jpeg_data: &[u8]) -> Option<(usize, usize, egui::TextureHandle)> {
    let decoded = image::load_from_memory_with_format(jpeg_data, image::ImageFormat::Jpeg).ok()?;
    let rgba = decoded.to_rgba8();
    let w = rgba.width() as usize;
    let h = rgba.height() as usize;
    let pixels: Vec<u8> = rgba.pixels().flat_map(|p| [p[0], p[1], p[2], p[3]]).collect();
    let texture = egui::ColorImage::from_rgba_unmultiplied([w, h], &pixels);
    let handle = ctx.load_texture("frame", texture, egui::TextureOptions::default());
    Some((w, h, handle))
}

impl eframe::App for AppState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {

        // ── Handle dropped files ONCE at start of frame ───────────
        ctx.input(|i| {
            for f in &i.raw.dropped_files {
                if let Some(p) = &f.path {
                    try_load_dropped(self, &p.to_string_lossy());
                }
            }
        });

        // ── TOP BAR: minimal controls only ────────────────────────
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("DaSiWa Video Compare");
                ui.separator();

                ui.label("A:");
                if ui.small_button("Browse...").clicked() {
                    browse_directory(self, self.root_dir.clone());
                    self.show_browser = true;
                }
                if let Some(ref path) = self.path_a {
                    ui.text_edit_singleline(&mut path.display().to_string());
                }
                if ui.small_button("Load A").clicked() {
                    if let Some(ref path) = self.path_a {
                        let s = path.display().to_string();
                        let p = clean_path(&s, "");
                        let _ = register_media(self, p);
                    }
                }
                ui.separator();

                ui.label("B:");
                if ui.small_button("Browse...").clicked() {
                    browse_directory(self, self.root_dir.clone());
                    self.show_browser = true;
                }
                if let Some(ref path) = self.path_b {
                    ui.text_edit_singleline(&mut path.display().to_string());
                }
                if ui.small_button("Load B").clicked() {
                    if let Some(ref path) = self.path_b {
                        let s = path.display().to_string();
                        let p = clean_path(&s, "");
                        let _ = register_media(self, p);
                    }
                }
                ui.separator();

                egui::ComboBox::from_label("")
                    .selected_text(match self.current_mode {
                        CompareMode::Side => "Side",
                        CompareMode::Slider => "Slider",
                        CompareMode::Blend => "Blend",
                        CompareMode::Diff => "Diff",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.current_mode, CompareMode::Side, "Side");
                        ui.selectable_value(&mut self.current_mode, CompareMode::Slider, "Slider");
                        ui.selectable_value(&mut self.current_mode, CompareMode::Blend, "Blend");
                        ui.selectable_value(&mut self.current_mode, CompareMode::Diff, "Diff");
                    });

                ui.separator();
                ui.checkbox(&mut self.show_details, "Details");
                ui.separator();
                if ui.button("Quit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });

        // ── CENTRAL PANEL: large interactive drop zone ────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            let zone_w = ui.available_width();
            let zone_h = 400.0;

            // Big drop zone that captures drag events
            let resp = ui.allocate_response(egui::vec2(zone_w, zone_h), egui::Sense::hover());
            
            // Visual feedback when hovering
            if resp.hovered() {
                let bg = egui::Color32::from_rgba_premultiplied(40, 40, 120, 60);
                ui.painter().rect_filled(resp.rect, 8.0, bg);
                let stroke = egui::Stroke::new(2.0_f32, egui::Color32::YELLOW);
                ui.painter().rect_stroke(resp.rect, 8.0, stroke);
                self.drop_zone_hovered = true;
            } else {
                self.drop_zone_hovered = false;
            }

            // Content inside the drop zone
            ui.allocate_space(egui::vec2(zone_w, zone_h));
            ui.centered_and_justified(|ui| {
                if self.drop_zone_hovered {
                    ui.label("Drop video files here →")
                      .on_hover_text("Drag & drop any video file here");
                } else if self.path_a.is_some() && self.path_b.is_some() {
                    // Both videos loaded - show them side by side
                    ui.horizontal_wrapped(|ui| {
                        if let Some(ref path) = self.path_a {
                            let path_str = path.display().to_string();
                            if let Some(jpeg_data) = extract_frame(&path_str, self.seek_position) {
                                if let Some((_w, _h, handle)) = make_texture(ctx, &jpeg_data) {
                                    ui.image(&handle);
                                }
                            }
                        }
                        if let Some(ref path) = self.path_b {
                            let path_str = path.display().to_string();
                            if let Some(jpeg_data) = extract_frame(&path_str, self.seek_position) {
                                if let Some((_w, _h, handle)) = make_texture(ctx, &jpeg_data) {
                                    ui.image(&handle);
                                }
                            }
                        }
                    });
                } else {
                    ui.label("Drop video files here →")
                      .on_hover_text("Drag & drop any video file here");
                }
            });
        });

        // ── BOTTOM PANEL: seek/play controls ──────────────────────
        egui::TopBottomPanel::bottom("controls").show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(format!("{:.2}s", self.seek_position));
                ui.add(egui::Slider::new(&mut self.seek_position, 0.0..=600.0).text("Seek"));

                ui.separator();

                ui.label(format!("{:.0}%", (self.blend_value * 100.0) as i32));
                ui.add(egui::Slider::new(&mut self.blend_value, 0.0..=1.0).text("Blend"));

                ui.separator();

                ui.label(format!("{:?}", self.audio_mode));
                egui::ComboBox::from_label("")
                    .selected_text(format!("{:?}", self.audio_mode))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.audio_mode, AudioMode::A, "A");
                        ui.selectable_value(&mut self.audio_mode, AudioMode::B, "B");
                        ui.selectable_value(&mut self.audio_mode, AudioMode::Mute, "Mute");
                    });

                ui.separator();

                ui.checkbox(&mut self.playing, "Play");

                if self.show_details {
                    ui.separator();
                    ui.label(format!("Mode: {:?}", self.current_mode));
                }
            });
        });

        // ── File browser dialog ──────────────────────────────────
        if self.show_browser {
            egui::Window::new("File Browser").show(ctx, |ui| {
                ui.label(format!("Current: {}", self.browser_path.display()));
                let items_clone: Vec<BrowserItem> = self.browser_items.clone();
                for item in items_clone {
                    if ui.selectable_label(false, &item.name).clicked() {
                        if item.is_dir {
                            browse_directory(self, item.path);
                        } else {
                            self.path_a = Some(item.path);
                            self.show_browser = false;
                        }
                    }
                }
                if ui.button("Close").clicked() {
                    self.show_browser = false;
                }
            });
        }
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 800.0])
            .with_min_inner_size([900.0, 550.0]),
        ..Default::default()
    };

    eframe::run_native(
        "DaSiWa Simple Video Compare",
        options,
        Box::new(|_cc| {
            Ok(Box::new(AppState {
                root_dir: dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")),
                media_entries: HashMap::new(),
                current_mode: CompareMode::Side,
                blend_value: 0.5,
                seek_position: 0.0,
                playing: false,
                audio_mode: AudioMode::A,
                path_a: None,
                path_b: None,
                show_details: false,
                show_browser: false,
                browser_path: dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")),
                browser_items: Vec::new(),
                drop_zone_hovered: false,
            }))
        }),
    )
}
