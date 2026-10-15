// SPDX-License-Identifier: GPL-3.0-or-later

#[derive(Clone, Debug, PartialEq)]
pub struct SourceDetails {
    pub label: &'static str,
    pub filename: String,
    pub rows: Vec<String>,
}

pub fn from_probe(slot: usize, path: &str, probe: &serde_json::Value) -> SourceDetails {
    let mut details = SourceDetails {
        label: if slot == 0 { "A" } else { "B" },
        filename: std::path::Path::new(path).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| path.into()),
        rows: Vec::new(),
    };
    let streams = probe["streams"].as_array();
    let video = streams.and_then(|s| s.iter().find(|s| {
        s["codec_type"] == "video" && s["disposition"]["attached_pic"].as_u64() != Some(1)
    }));
    if let Some(video) = video {
        let mut row = Vec::new();
        if let (Some(w), Some(h)) = (positive(&video["width"]), positive(&video["height"])) {
            row.push(format!("{w:.0}×{h:.0}"));
        }
        row.extend(text(&video["codec_name"]));
        if let Some(fps) = frame_rate(&video["avg_frame_rate"]).or_else(|| frame_rate(&video["r_frame_rate"])) {
            row.push(format!("{fps:.3} fps"));
        }
        push_row(&mut details.rows, row);
        let mut row: Vec<String> = text(&video["pix_fmt"]).into_iter().collect();
        if let Some(bits) = positive(&video["bits_per_raw_sample"]).or_else(|| positive(&video["bits_per_sample"])) {
            row.push(format!("{bits:.0}-bit"));
        }
        push_row(&mut details.rows, row);
        push_row(&mut details.rows, ["color_space", "color_transfer", "color_primaries", "color_range"]
            .iter().filter_map(|key| text(&video[*key])).collect());
    }
    let mut row = Vec::new();
    if let Some(seconds) = positive(&probe["format"]["duration"])
        .or_else(|| video.and_then(|v| positive(&v["duration"]))) {
        row.push(format!("{seconds:.1} s"));
    }
    if let Some(rate) = positive(&probe["format"]["bit_rate"]) {
        row.push(format!("{:.2} Mb/s", rate / 1_000_000.0));
    }
    push_row(&mut details.rows, row);
    if let Some(audio) = streams.and_then(|s| s.iter().find(|s| s["codec_type"] == "audio")) {
        let mut row: Vec<String> = text(&audio["codec_name"]).into_iter().collect();
        if let Some(rate) = positive(&audio["sample_rate"]) { row.push(format!("{rate:.0} Hz")); }
        if let Some(channels) = positive(&audio["channels"]) { row.push(format!("{channels:.0} ch")); }
        if !row.is_empty() { details.rows.push(format!("Audio: {}", row.join(" · "))); }
    }
    details
}

fn text(value: &serde_json::Value) -> Option<String> {
    value.as_str().map(str::trim)
        .filter(|s| !s.is_empty() && !matches!(*s, "unknown" | "N/A" | "unspecified"))
        .map(str::to_owned)
}

fn positive(value: &serde_json::Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str()?.parse().ok())
        .filter(|n| n.is_finite() && *n > 0.0)
}

fn frame_rate(value: &serde_json::Value) -> Option<f64> {
    if let Some((n, d)) = value.as_str().and_then(|s| s.split_once('/')) {
        let rate = n.parse::<f64>().ok()? / d.parse::<f64>().ok()?;
        return (rate.is_finite() && rate > 0.0).then_some(rate);
    }
    positive(value)
}

fn push_row(rows: &mut Vec<String>, parts: Vec<String>) {
    if !parts.is_empty() { rows.push(parts.join(" · ")); }
}

const PANEL_WIDTH: f64 = 600.0;
const LINE_HEIGHT: f64 = 26.0;
const PAD: f64 = 12.0;
const MAX_LINES: usize = 12;

fn panel_lines(details: &SourceDetails) -> Vec<String> {
    std::iter::once(details.label.to_owned())
        .chain(std::iter::once(details.filename.clone()))
        .chain(details.rows.iter().cloned())
        .take(MAX_LINES)
        .map(|line| {
            let mut chars = line.chars().map(|c| {
                if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') { ' ' } else { c }
            });
            let mut line: String = chars.by_ref().take(80).collect();
            if chars.next().is_some() { line.push('…'); }
            line
        }).collect()
}

fn ass_escape(text: &str) -> String {
    // Match mpv's osd_mangle_ass: WORD JOINER breaks literal backslash escapes.
    text.replace('\\', "\\\u{2060}").replace('{', "\\{")
}

/// Windows/final-output only; never put this in Linux's packed video cache.
/// Keep the IPC client alive: mpv removes its overlays when it disconnects.
/// JSON IPC: {"command":{"name":"osd-overlay","id":41,"format":"ass-events",
/// "data":ass_overlay(details),"res_x":1280,"res_y":720,"z":0}}
/// Hide: {"command":{"name":"osd-overlay","id":41,"format":"none","data":""}}
pub fn ass_overlay(details: &[Option<SourceDetails>; 2]) -> String {
    use std::fmt::Write;
    let mut ass = String::new();
    for (slot, details) in details.iter().enumerate() {
        let Some(details) = details else { continue };
        let lines = panel_lines(details);
        let x = if slot == 0 { 24 } else { 656 };
        let right = x + PANEL_WIDTH as i32;
        let height = (PAD * 2.0 + LINE_HEIGHT * lines.len() as f64) as i32;
        let bottom = 24 + height;
        // ASS alpha is transparency: 0x59 = 89/255, hence 65.1% opaque black.
        let _ = writeln!(ass, "{{\\rDefault\\an7\\pos({x},24)\\clip({x},24,{right},{bottom})\\bord0\\shad0\\blur0\\fscx100\\fscy100\\frz0\\1c&H000000&\\1a&H59&\\p1}}m 0 0 l 600 0 600 {height} 0 {height}{{\\p0}}");
        for (line, text) in lines.iter().enumerate() {
            let tx = x + PAD as i32;
            let ty = 24 + PAD as i32 + line as i32 * LINE_HEIGHT as i32;
            let text = ass_escape(text);
            let _ = writeln!(ass, "{{\\rDefault\\an7\\pos({tx},{ty})\\clip({x},24,{right},{bottom})\\q2\\fnSans\\fs20\\b0\\i0\\bord0\\shad0\\blur0\\fscx100\\fscy100\\fsp0\\frz0\\1c&HFFFFFF&\\1a&H00&\\p0}}{text}");
        }
    }
    ass
}

/// Draw on the GTK overlay after video composition; caller owns visibility/redraws.
#[cfg(target_os = "linux")]
pub fn draw_cairo(cr: &gtk::cairo::Context, width: f64, height: f64, details: &[Option<SourceDetails>; 2]) {
    use gtk::cairo::{FontSlant, FontWeight};
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return;
    }
    if cr.save().is_err() { return; }
    cr.rectangle(0.0, 0.0, width, height);
    cr.clip();
    cr.scale(width / 1280.0, height / 720.0);
    cr.select_font_face("Sans", FontSlant::Normal, FontWeight::Normal);
    cr.set_font_size(20.0);
    for (slot, details) in details.iter().enumerate() {
        let Some(details) = details else { continue };
        if cr.save().is_err() { break; }
        let lines = panel_lines(details);
        let x = if slot == 0 { 24.0 } else { 656.0 };
        let h = PAD * 2.0 + lines.len() as f64 * LINE_HEIGHT;
        cr.rectangle(x, 24.0, PANEL_WIDTH, h);
        cr.set_source_rgba(0.0, 0.0, 0.0, 166.0 / 255.0);
        let _ = cr.fill();
        cr.rectangle(x + PAD, 24.0 + PAD, PANEL_WIDTH - PAD * 2.0, h - PAD * 2.0);
        cr.clip();
        cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
        let ascent = cr.font_extents().map(|e| e.ascent()).unwrap_or(20.0);
        for (line, text) in lines.iter().enumerate() {
            let mut text = text.clone();
            // shortcut: toy-text single lines; use Pango if complex shaping is needed.
            while !text.is_empty() && cr.text_extents(&text).map(|e| e.width().max(e.x_advance()) > PANEL_WIDTH - PAD * 2.0).unwrap_or(false) {
                if text.ends_with('…') { text.pop(); }
                text.pop();
                if !text.is_empty() { text.push('…'); }
            }
            cr.move_to(x + PAD, 24.0 + PAD + ascent + line as f64 * LINE_HEIGHT);
            let _ = cr.show_text(&text);
        }
        let _ = cr.restore();
    }
    let _ = cr.restore();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[cfg(target_os = "linux")]
    #[test]
    fn cairo_draws_alpha_panels_solid_text_and_restores_context() {
        use gtk::cairo::{Context, Format, ImageSurface};
        let mut surface = ImageSurface::create(Format::ARgb32, 1280, 720).unwrap();
        let cr = Context::new(&surface).unwrap();
        let matrix = cr.matrix();
        let clip = cr.clip_extents().unwrap();
        let mut a = from_probe(0, "A".repeat(1000).as_str(), &json!({}));
        a.rows = vec!["測試".repeat(1000); 100];
        draw_cairo(&cr, 1280.0, 720.0, &[Some(a), Some(from_probe(1, "B.mkv", &json!({})))]);
        assert_eq!(cr.matrix(), matrix);
        assert_eq!(cr.clip_extents().unwrap(), clip);
        cr.status().unwrap();
        drop(cr);
        surface.flush();
        let stride = surface.stride() as usize;
        let data = surface.data().unwrap();
        let pixel = |x: usize, y: usize| u32::from_ne_bytes(data[y * stride + x * 4..y * stride + x * 4 + 4].try_into().unwrap());
        assert_eq!(pixel(25, 25) >> 24, 166);
        assert_eq!(pixel(657, 25) >> 24, 166);
        assert_eq!(pixel(0, 0), 0);
        assert!((36..60).any(|y| (36..60).any(|x| pixel(x, y) == 0xffffffff)), "no solid white glyph interior");
        for y in 0..720 {
            for x in 0..1280 {
                if !((24..624).contains(&x) && (24..360).contains(&y)
                    || (656..1256).contains(&x) && (24..100).contains(&y)) {
                    assert_eq!(pixel(x, y), 0, "spill at {x},{y}");
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cairo_handles_small_or_invalid_viewports() {
        use gtk::cairo::{Context, Format, ImageSurface};
        let surface = ImageSurface::create(Format::ARgb32, 9, 7).unwrap();
        let cr = Context::new(&surface).unwrap();
        let details = [None, Some(from_probe(1, "B.mkv", &json!({})))];
        for (w, h) in [(9.0, 7.0), (0.0, 1.0), (-1.0, 1.0), (f64::NAN, 1.0), (1.0, f64::INFINITY)] {
            draw_cairo(&cr, w, h, &details);
            cr.status().unwrap();
        }
    }

    #[test]
    fn filenames_cannot_inject_ass_events_or_tags() {
        let name = "原本{\\alpha&HFF&}\\N\nnew.mkv";
        let mut details = from_probe(0, name, &json!({}));
        #[cfg(not(windows))]
        assert_eq!(details.filename, name);
        details.filename = name.into();
        let ass = ass_overlay(&[Some(details), None]);
        assert_eq!(ass.lines().count(), 3);
        assert!(ass.contains("原本\\{\\\u{2060}alpha&HFF&}\\\u{2060}N new.mkv"), "{ass}");
        assert!(!ass.contains("{\\alpha&HFF&}"));
        assert_eq!(ass_escape("{}\\N\\n\\h"), "\\{}\\\u{2060}N\\\u{2060}n\\\u{2060}h");
    }

    #[test]
    fn ass_panels_are_fixed_separate_and_do_not_dim_text() {
        let a = from_probe(0, "A.mkv", &json!({}));
        let b = from_probe(1, "B.mkv", &json!({}));
        let ass = ass_overlay(&[Some(a), Some(b.clone())]);
        assert_eq!(ass.matches("\\1a&H59&").count(), 2);
        assert_eq!(ass.matches("\\1a&H00&").count(), 4);
        for position in ["\\pos(24,24)", "\\pos(656,24)", "\\pos(36,36)", "\\pos(668,36)"] {
            assert!(ass.contains(position), "{position}: {ass}");
        }
        assert!(ass.contains("\\clip(24,24,624,100)"));
        assert!(ass.contains("\\clip(656,24,1256,100)"));
        assert!(ass.contains("}A\n"));
        assert!(ass.contains("}B\n"));
        let only_b = ass_overlay(&[None, Some(b)]);
        assert!(only_b.contains("\\pos(656,24)"));
        assert!(!only_b.contains("}A\n"));
        assert_eq!(ass_overlay(&[None, None]), "");
    }

    #[test]
    fn missing_probe_values_are_omitted() {
        let d = from_probe(1, "B.mkv", &json!({"streams":[{"codec_type":"video",
            "width":0,"height":null,"codec_name":"unknown","avg_frame_rate":"0/0",
            "r_frame_rate":"1/0","pix_fmt":"N/A","bits_per_raw_sample":"0"}],
            "format":{"duration":"NaN","bit_rate":"-1"}}));
        assert!(d.rows.is_empty(), "{:?}", d.rows);
        assert_eq!(d.label, "B");
    }

    #[test]
    fn metadata_uses_original_per_file_streams() {
        let a = from_probe(0, "/videos/原本.mkv", &json!({"streams":[
            {"codec_type":"video","width":100,"height":100,"disposition":{"attached_pic":1}},
            {"codec_type":"video","width":3840,"height":2160,"codec_name":"hevc",
             "avg_frame_rate":"24000/1001","pix_fmt":"yuv420p10le","bits_per_raw_sample":"10",
             "color_space":"bt2020nc","color_transfer":"smpte2084","color_primaries":"bt2020"},
            {"codec_type":"audio","codec_name":"aac","sample_rate":"48000","channels":2}],
            "format":{"duration":"120.5","bit_rate":"12000000"}}));
        let b = from_probe(1, "/videos/B.mp4", &json!({"streams":[
            {"codec_type":"video","width":1920,"height":1080,"codec_name":"h264","r_frame_rate":"30/1"}]}));
        assert_eq!(a.filename, "原本.mkv");
        assert_eq!(b.filename, "B.mp4");
        assert_eq!((a.label, b.label), ("A", "B"));
        let rows = a.rows.join(" | ");
        for value in ["3840×2160", "hevc", "23.976 fps", "yuv420p10le", "10-bit", "bt2020nc", "smpte2084", "120.5 s", "12.00 Mb/s", "aac", "48000 Hz", "2 ch"] {
            assert!(rows.contains(value), "missing {value}: {rows}");
        }
        assert!(b.rows.join(" ").contains("1920×1080"));
        assert!(!b.rows.join(" ").contains("3840"));
    }
}
