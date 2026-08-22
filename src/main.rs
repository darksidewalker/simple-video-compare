use eframe::egui;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct AppState {
    root_dir: PathBuf,
    current_mode: CompareMode,
    blend_value: f32,
    seek_position: f64,
    playing: bool,
    audio_mode: AudioMode,
    path_a: Option<PathBuf>,
    path_b: Option<PathBuf>,
    show_details: bool,
    browser_path: PathBuf,
    show_browser: bool,
    frame_texture_a: Option<(usize, usize, egui::TextureHandle)>,
    frame_texture_b: Option<(usize, usize, egui::TextureHandle)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompareMode { Side, Slider, Blend, Diff }
impl Default for CompareMode { fn default() -> Self { Self::Side } }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioMode { A, B, Mute }

fn is_video_ext(name: &str) -> bool {
    matches!(name.to_lowercase().as_str(),
        "mp4"|"mkv"|"mov"|"webm"|"avi"|"m4v"|"mpg"|"mpeg"|"ts"|"m2ts"|"flv"|"ogv")
}

fn extract_frame(video_path: &str, seconds: f64) -> Option<Vec<u8>> {
    let output = Command::new("ffmpeg")
        .args([
            "-ss", &seconds.to_string(),
            "-i", video_path,
            "-frames:v", "1",
            "-f", "image2pipe",
            "-vcodec", "mjpeg",
            "-q:v", "2",
            "pipe:1"
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output();

    match output {
        Ok(out) if out.status.success() => Some(out.stdout),
        _ => None,
    }
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

fn load_dropped_file(state: &mut AppState, file_path: &str) {
    let ext = Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    
    if !is_video_ext(&ext) { return; }
    
    let pb = PathBuf::from(file_path);
    
    if state.path_a.is_none() {
        state.path_a = Some(pb.clone());
    } else if state.path_b.is_none() {
        state.path_b = Some(pb.clone());
    } else {
        state.path_b = Some(pb);
    }
}

impl eframe::App for AppState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        
        // ════════════════════════════════════════════════════════════
        // CRITICAL: Capture ALL OS-level dropped files IMMEDIATELY
        // This MUST happen BEFORE any other ctx.input() calls
        // because egui clears raw.dropped_files after first read
        // ════════════════════════════════════════════════════════════
        let mut dropped_paths: Vec<String> = Vec::new();
        
        ctx.input(|i| {
            for f in &i.raw.dropped_files {
                if let Some(p) = &f.path {
                    dropped_paths.push(p.to_string_lossy().to_string());
                }
            }
        });
        
        // Process all dropped files immediately
        for dp in &dropped_paths {
            println!("Dropped: {}", dp);
            load_dropped_file(self, dp);
        }
        
        // ════════════════════════════════════════════════════════════
        // TOP BAR
        // ════════════════════════════════════════════════════════════
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("DaSiWa Video Compare");
                
                ui.separator();
                ui.label("Video A:");
                if let Some(ref p) = self.path_a {
                    ui.label(p.display().to_string()).on_hover_text("Drop video here or click Browse");
                }
                if ui.small_button("Browse...").clicked() {
                    self.browser_path = self.root_dir.clone();
                    self.show_browser = true;
                }
                
                ui.separator();
                ui.label("Video B:");
                if let Some(ref p) = self.path_b {
                    ui.label(p.display().to_string()).on_hover_text("Drop video here or click Browse");
                }
                if ui.small_button("Browse...").clicked() {
                    self.browser_path = self.root_dir.clone();
                    self.show_browser = true;
                }
                
                ui.separator();
                
                egui::ComboBox::from_label("Mode")
                    .selected_text(match self.current_mode {
                        CompareMode::Side => "Side by Side",
                        CompareMode::Slider => "Slider",
                        CompareMode::Blend => "Blend",
                        CompareMode::Diff => "Difference",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.current_mode, CompareMode::Side, "Side by Side");
                        ui.selectable_value(&mut self.current_mode, CompareMode::Slider, "Slider");
                        ui.selectable_value(&mut self.current_mode, CompareMode::Blend, "Blend");
                        ui.selectable_value(&mut self.current_mode, CompareMode::Diff, "Difference");
                    });
                
                ui.separator();
                
                if ui.button("Quit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
        
        // ════════════════════════════════════════════════════════════
        // CENTRAL DROP ZONE - NO Sense needed, raw.dropped_files handles it
        // ════════════════════════════════════════════════════════════
        egui::CentralPanel::default().show(ctx, |ui| {
            let available_width = ui.available_width();
            
            // Draw a simple background for the whole central area
            let bg_color = egui::Color32::from_rgba_premultiplied(25, 25, 40, 60);
            let rect = egui::Rect::from_x_y_ranges(0.0..=available_width, 0.0..=ui.max_rect().height());
            ui.painter().rect_filled(rect, 0.0, bg_color);
            
            ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::TopDown), |ui| {
                if self.path_a.is_some() || self.path_b.is_some() {
                    ui.horizontal_wrapped(|ui| {
                        if let Some(ref p) = self.path_a {
                            let name = p.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("?");
                            ui.label(format!("▶ {}", name));
                        }
                        if let Some(ref p) = self.path_b {
                            let name = p.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("?");
                            ui.label(format!("▶ {}", name));
                        }
                    });
                } else {
                    ui.label("📁 Drop video files here");
                    ui.label("Supports MP4, MKV, MOV, AVI, WebM, and more");
                }
            });
        });
        
        // ════════════════════════════════════════════════════════════
        // BOTTOM CONTROLS
        // ════════════════════════════════════════════════════════════
        egui::TopBottomPanel::bottom("controls").show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(format!("{:.2}s", self.seek_position));
                ui.add(egui::Slider::new(&mut self.seek_position, 0.0..=600.0).text("Time"));
                
                ui.separator();
                
                ui.label(format!("{:.0}%", (self.blend_value * 100.0) as i32));
                ui.add(egui::Slider::new(&mut self.blend_value, 0.0..=1.0).text("Blend"));
                
                ui.separator();
                
                ui.checkbox(&mut self.playing, "Auto-play");
                
                if self.show_details {
                    ui.separator();
                    ui.label(format!("Mode: {:?}", self.current_mode));
                }
            });
        });
        
        // ════════════════════════════════════════════════════════════
        // FILE BROWSER DIALOG
        // ════════════════════════════════════════════════════════════
        if self.show_browser {
            egui::Window::new("Select Video File")
                .resizable(true)
                .show(ctx, |ui| {
                    ui.label(format!("Current directory: {}", self.browser_path.display()));
                    
                    if ui.button("Up one level").clicked() {
                        if let Some(parent) = self.browser_path.parent() {
                            self.browser_path = parent.to_path_buf();
                        }
                    }
                    
                    ui.separator();
                    
                    // List directories and video files
                    if let Ok(entries) = fs::read_dir(&self.browser_path) {
                        for entry in entries.flatten() {
                            let name = entry.file_name().to_string_lossy().to_string();
                            if name.starts_with('.') { continue; }
                            
                            if let Ok(meta) = entry.metadata() {
                                if meta.is_dir() {
                                    if ui.selectable_label(false, format!("📂 {}", name)).clicked() {
                                        self.browser_path = entry.path();
                                    }
                                } else if is_video_ext(&name) {
                                    if ui.selectable_label(false, format!("🎬 {}", name)).clicked() {
                                        let target = if self.path_a.is_none() {
                                            &mut self.path_a
                                        } else {
                                            &mut self.path_b
                                        };
                                        *target = Some(entry.path());
                                        self.show_browser = false;
                                    }
                                }
                            }
                        }
                    }
                    
                    ui.separator();
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
                current_mode: CompareMode::Side,
                blend_value: 0.5,
                seek_position: 0.0,
                playing: false,
                audio_mode: AudioMode::A,
                path_a: None,
                path_b: None,
                show_details: false,
                browser_path: dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")),
                show_browser: false,
                frame_texture_a: None,
                frame_texture_b: None,
            }))
        }),
    )
}
