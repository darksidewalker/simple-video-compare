use serde::Serialize;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::State;

#[cfg(unix)]
type Ipc = std::os::unix::net::UnixStream;
#[cfg(windows)]
type Ipc = std::fs::File;

#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NativeStatus {
    pub paths: Vec<String>,
    pub duration: f64,
    pub time: f64,
    pub paused: bool,
    pub mode: String,
    pub amount: f64,
    pub audio: String,
    pub volume: f64,
    pub running: bool,
}
impl Default for NativeStatus {
    fn default() -> Self {
        Self {
            paths: vec![],
            duration: 0.0,
            time: 0.0,
            paused: true,
            mode: "side".into(),
            amount: 0.5,
            audio: "A".into(),
            volume: 100.0,
            running: false,
        }
    }
}

#[derive(Default, Clone)]
pub struct NativeState(Arc<Mutex<Engine>>);
impl NativeState {
    #[cfg(target_os = "linux")]
    pub fn overlay_status(&self) -> Option<(NativeStatus, (u32, u32))> {
        self.0.try_lock().ok().map(|engine| {
            let mut status = engine.status.clone();
            if let Some(player) = engine.gpu_slider() { status.amount = player.split_amount().unwrap_or(status.amount); }
            (status, engine.size)
        })
    }

    #[cfg(any(target_os = "linux", windows))]
    pub fn set_embedded(&self, player: Arc<crate::embedded::Player>) {
        self.0.lock().unwrap().embedded = Some(player);
    }
    pub fn shutdown(&self) {
        if let Ok(mut engine) = self.0.lock() {
            engine.stop();
            #[cfg(any(target_os = "linux", windows))]
            if let Some(player) = engine.embedded.take() {
                if !player.ready.load(std::sync::atomic::Ordering::Acquire) {
                    player.shutdown();
                }
            }
        }
    }
}

fn tool(name: &str) -> Result<PathBuf, String> {
    let key = format!("DAWVC_{}", name.to_uppercase());
    if let Some(path) = std::env::var_os(&key).filter(|p| !p.is_empty()) {
        let path = PathBuf::from(path);
        return if path.is_file() {
            Ok(path)
        } else {
            Err(format!("{key} is not a file: {}", path.display()))
        };
    }
    let filename = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.into()
    };
    let mut dirs = vec![];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            dirs.push(dir.to_path_buf());
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs.into_iter()
        .map(|d| d.join(&filename))
        .find(|p| p.is_file())
        .ok_or_else(|| {
            format!("{name} not found; install it, place it beside the app, or set {key}")
        })
}

fn media_path(path: &str) -> Result<PathBuf, String> {
    if path.is_empty() || path.contains('\0') {
        return Err("Empty or invalid media path".into());
    }
    let path = Path::new(path)
        .canonicalize()
        .map_err(|e| format!("Cannot open media: {e}"))?;
    if !path.is_file() {
        return Err("Media path must be a regular file".into());
    }
    Ok(path)
}

fn probe(path: &str) -> Result<Value, String> {
    let path = media_path(path)?;
    let output = Command::new(tool("ffprobe")?)
        .args([
            "-v",
            "error",
            "-show_format",
            "-show_streams",
            "-of",
            "json",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Start ffprobe: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "ffprobe: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("Invalid ffprobe JSON: {e}"))
}

fn numeric(value: &Value, minimum: f64, maximum: f64) -> Result<f64, String> {
    value
        .as_f64()
        .filter(|v| v.is_finite() && *v >= minimum && *v <= maximum)
        .ok_or_else(|| format!("Expected a number between {minimum} and {maximum}"))
}

fn video_filter(mode: &str, amount: f64, width: u32, height: u32) -> Result<String, String> {
    if !amount.is_finite() || !(0.0..=1.0).contains(&amount) || width < 2 || height < 2 {
        return Err("Invalid comparison dimensions or amount".into());
    }
    let fit = format!("scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2,setsar=1,format=yuv420p");
    let combine = match mode {
        "side" => "hstack=inputs=2:shortest=0".to_string(),
        "slider" | "blend" => "hstack=inputs=2:shortest=0".to_string(),
        "diff" => "blend=all_mode=difference:shortest=0".to_string(),
        _ => return Err(format!("Unknown comparison mode: {mode}")),
    };
    Ok(format!("[vid1]{fit}[a];[vid2]{fit}[b];[a][b]{combine}[vo]"))
}

fn amount_expression(mode: &str, amount: f64) -> String {
    if mode == "slider" { format!("if(lt(X,W*{amount}),A,B)") }
    else { format!("A*{}+B*{amount}", 1.0 - amount) }
}

fn comparison_vf(mode: &str, amount: f64, width: u32, height: u32) -> String {
    if !matches!(mode, "slider" | "blend") { return String::new(); }
    let expression = amount_expression(mode, amount);
    format!("@compare:lavfi=[split[a][b];[a]crop={width}:{height}:0:0[left];[b]crop={width}:{height}:{width}:0[right];[left][right]blend@mix=all_expr='{expression}':shortest=0]")
}

enum OwnedPlayer {
    Process(Child),
    #[cfg(any(target_os = "linux", windows))]
    Embedded(Arc<crate::embedded::Player>),
}
impl OwnedPlayer {
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        match self {
            Self::Process(child) => child.try_wait(),
            #[cfg(any(target_os = "linux", windows))]
            Self::Embedded(player) => {
                if let Some(error) = player.error.lock().unwrap().as_ref() {
                    return Err(std::io::Error::other(error.clone()));
                }
                Ok(None)
            }
        }
    }
    #[cfg(test)]
    fn id(&self) -> u32 {
        match self {
            Self::Process(child) => child.id(),
            #[cfg(any(target_os = "linux", windows))]
            Self::Embedded(_) => std::process::id(),
        }
    }
}
fn embedded_load(paths: &[String], duration: f64, graph: Option<String>) -> Value {
    let mut options = json!({"end": format!("{duration:.6}")});
    if paths.len() == 2 {
        options["external-file"] = json!(paths[1]);
    }
    if let Some(graph) = graph {
        options["lavfi-complex"] = json!(graph);
    }
    json!(["loadfile", paths[0], "replace", -1, options])
}
struct Session {
    child: OwnedPlayer,
    writer: Option<Ipc>,
    replies: mpsc::Receiver<Value>,
    reader: Option<JoinHandle<()>>,
    logger: Option<JoinHandle<()>>,
    logs: Arc<Mutex<VecDeque<String>>>,
    dir: PathBuf,
    endpoint: String,
    request_id: u64,
    file_loaded: bool,
}
impl Session {
    fn error(&self, message: impl std::fmt::Display) -> String {
        let logs = self.logs.lock().unwrap_or_else(|e| e.into_inner());
        format!(
            "{message}\n{}",
            logs.iter().cloned().collect::<Vec<_>>().join("\n")
        )
    }
    fn request(&mut self, command: Value) -> Result<Value, String> {
        self.request_id += 1;
        let id = self.request_id;
        let mut payload =
            serde_json::to_vec(&json!({"command": command, "request_id": id})).unwrap();
        payload.push(b'\n');
        self.writer
            .as_mut()
            .ok_or("mpv IPC is disconnected")?
            .write_all(&payload)
            .map_err(|e| self.error(format!("mpv IPC write: {e}")))?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let response = self
                .replies
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|e| self.error(format!("mpv IPC response: {e}")))?;
            if response["event"] == "file-loaded" {
                self.file_loaded = true;
            }
            if response["request_id"].as_u64() != Some(id) {
                continue;
            }
            if response["error"] != "success" {
                return Err(self.error(format!("mpv: {} ({command})", response["error"])));
            }
            return Ok(response["data"].clone());
        }
    }
    fn get(&mut self, name: &str) -> Result<Value, String> {
        self.request(json!(["get_property", name]))
    }
    fn set(&mut self, name: &str, value: Value) -> Result<(), String> {
        self.request(json!(["set_property", name, value]))
            .map(|_| ())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        if matches!(self.child, OwnedPlayer::Embedded(_)) {
            let _ = self.request(json!(["stop"]));
            if let Some(writer) = self.writer.take() {
                let _ = writer.shutdown(std::net::Shutdown::Both);
            }
            if let Some(reader) = self.reader.take() {
                let _ = reader.join();
            }
            let _ = std::fs::remove_dir(&self.dir);
            return;
        }
        #[cfg(windows)]
        if let OwnedPlayer::Embedded(player) = &self.child {
            // Termination closes mpv's pipe clients before joining the blocking reader.
            player.shutdown();
            self.writer.take();
            if let Some(reader) = self.reader.take() {
                let _ = reader.join();
            }
            let _ = std::fs::remove_dir(&self.dir);
            return;
        }
        // Kill only the child owned by this session; closing its IPC wakes the reader on Windows too.
        if let Some(writer) = &mut self.writer {
            let _ = writer.write_all(b"{\"command\":[\"quit\"]}\n");
        }
        let deadline = Instant::now() + Duration::from_millis(500);
        while matches!(self.child.try_wait(), Ok(None)) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        if matches!(self.child.try_wait(), Ok(None)) {
            if let OwnedPlayer::Process(child) = &mut self.child {
                let _ = child.kill();
            }
        }
        if let OwnedPlayer::Process(child) = &mut self.child {
            let _ = child.wait();
        }
        self.writer.take();
        if let Some(handle) = self.reader.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.logger.take() {
            let _ = handle.join();
        }
        #[cfg(unix)]
        let _ = std::fs::remove_file(&self.endpoint);
        let _ = std::fs::remove_dir(&self.dir);
    }
}

#[derive(Default)]
struct Engine {
    #[cfg(any(target_os = "linux", windows))]
    embedded: Option<Arc<crate::embedded::Player>>,
    session: Option<Session>,
    status: NativeStatus,
    size: (u32, u32),
    audio_ids: [Option<u64>; 2],
}
impl Engine {
    #[cfg(target_os = "linux")]
    fn gpu_slider(&self) -> Option<&Arc<crate::embedded::Player>> {
        if self.status.mode == "slider" && self.status.paths.iter().filter(|p| !p.is_empty()).count() == 2 {
            if let Some(Session { child: OwnedPlayer::Embedded(player), .. }) = self.session.as_ref() { return Some(player); }
        }
        None
    }
    fn uses_gpu_slider(&self) -> bool {
        #[cfg(target_os = "linux")]
        { self.gpu_slider().is_some() }
        #[cfg(not(target_os = "linux"))]
        { false }
    }

    fn stop(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some(player) = &self.embedded { player.configure_split(None); }
        self.session.take();
        self.status.running = false;
        self.status.paused = true;
    }
    fn graph(&self) -> Result<String, String> {
        let mut graph = video_filter(
            &self.status.mode,
            self.status.amount,
            self.size.0,
            self.size.1,
        )?;
        let index = usize::from(self.status.audio == "B");
        if let Some(id) = self.audio_ids[index] {
            graph.push_str(&format!(";[aid{id}]anull[ao]"));
        }
        Ok(graph)
    }
    fn open(&mut self, paths: Vec<String>, headless: bool) -> Result<(), String> {
        if !(1..=2).contains(&paths.len()) {
            return Err("Select one or two videos".into());
        }
        let source_slots: Vec<usize> = paths
            .iter()
            .enumerate()
            .filter(|(_, path)| !path.trim().is_empty())
            .map(|(index, _)| index)
            .collect();
        if source_slots.is_empty() {
            return Err("Select at least one video".into());
        }
        let mut slot_paths = vec![String::new(); 2];
        for &index in &source_slots {
            slot_paths[index] = media_path(&paths[index])?
                .to_str()
                .ok_or("Media path is not UTF-8")?
                .into();
        }
        let paths: Vec<String> = source_slots
            .iter()
            .map(|&index| slot_paths[index].clone())
            .collect();
        let probes: Vec<Value> = paths.iter().map(|p| probe(p)).collect::<Result<_, _>>()?;
        let mut size = (0, 0);
        let mut duration: f64 = 0.0;
        for (index, probe) in probes.iter().enumerate() {
            let streams = probe["streams"].as_array().ok_or("No media streams")?;
            let videos: Vec<_> = streams
                .iter()
                .filter(|s| s["codec_type"] == "video")
                .collect();
            // shortcut: comparison assumes one video track per file; reject multi-angle/cover-art files explicitly.
            if videos.len() != 1 {
                return Err("Each input must contain exactly one video track".into());
            }
            if index == 0 {
                size = (
                    videos[0]["width"].as_u64().unwrap_or(0) as u32,
                    videos[0]["height"].as_u64().unwrap_or(0) as u32,
                );
                size.0 -= size.0 % 2;
                size.1 -= size.1 % 2;
                if size.0 < 2 || size.1 < 2 {
                    return Err("Invalid video dimensions".into());
                }
            }
            let d = probe["format"]["duration"]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|d| d.is_finite() && *d > 0.0)
                .ok_or("Media has no finite positive duration")?;
            duration = duration.max(d);
        }
        self.stop();
        self.status = NativeStatus {
            paths: slot_paths,
            duration,
            audio: if source_slots[0] == 1 { "B" } else { "A" }.into(),
            ..NativeStatus::default()
        };
        self.size = size;
        self.audio_ids = [None, None];
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("vc{:x}{nonce:x}", std::process::id()));
        std::fs::create_dir(&dir).map_err(|e| format!("Create IPC directory: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(e) = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)) {
                let _ = std::fs::remove_dir(&dir);
                return Err(e.to_string());
            }
        }
        #[cfg(unix)]
        let endpoint = dir.join("ipc").to_string_lossy().into_owned();
        #[cfg(windows)]
        let endpoint = format!(r"\\.\pipe\dawvc-{}-{nonce}", std::process::id());
        #[cfg(any(target_os = "linux", windows))]
        let embedded = if headless {
            None
        } else {
            Some(
                self.embedded
                    .clone()
                    .ok_or("Embedded renderer is not initialized")?,
            )
        };
        let mut session = None;
        #[cfg(any(target_os = "linux", windows))]
        if let Some(player) = embedded.as_ref() {
            #[cfg(windows)]
            if let Err(error) = player.start() {
                let _ = std::fs::remove_dir(&dir);
                return Err(error);
            }
            let deadline = Instant::now() + Duration::from_secs(10);
            while !player.ready.load(std::sync::atomic::Ordering::Acquire) {
                if let Some(error) = player.error.lock().map_err(|e| e.to_string())?.clone() {
                    return Err(error);
                }
                if Instant::now() >= deadline {
                    return Err("Embedded video pane never became ready".into());
                }
                thread::sleep(Duration::from_millis(10));
            }
            let (_, replies) = mpsc::channel();
            session = Some(Session {
                child: OwnedPlayer::Embedded(player.clone()),
                writer: None,
                replies,
                reader: None,
                logger: None,
                logs: Arc::default(),
                dir: dir.clone(),
                endpoint: player.endpoint.clone(),
                request_id: 0,
                file_loaded: false,
            });
        }
        if session.is_none() {
            let mut command = Command::new(tool("mpv")?);
            command
                .args([
                    "--no-config",
                    "--idle=yes",
                    "--keep-open=yes",
                    "--pause=yes",
                    "--force-window=yes",
                    "--hwdec=auto-safe",
                    "--input-terminal=no",
                    "--osc=no",
                    "--title=DaSiWa Native Video Compare",
                    "--msg-level=all=warn",
                ])
                .arg(format!("--input-ipc-server={endpoint}"))
                .arg(format!("--end={duration:.6}"))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped());
            if headless {
                command.args(["--vo=null", "--ao=null", "--hwdec=no"]);
            }
            if paths.len() == 2 {
                command.arg(format!("--external-file={}", paths[1]));
                // lavfi consumes CPU frames. auto-copy is the safe hardware-download fallback for comparisons.
                if !headless {
                    command.arg("--hwdec=auto-copy");
                }
                command.arg(format!("--lavfi-complex={}", self.graph()?));
            }
            command.arg("--").arg(&paths[0]);
            let mut child = match command.spawn() {
                Ok(child) => child,
                Err(e) => {
                    let _ = std::fs::remove_dir(&dir);
                    return Err(format!("Start mpv: {e}"));
                }
            };
            let stderr = child.stderr.take().unwrap();
            let logs = Arc::new(Mutex::new(VecDeque::new()));
            let logger_logs = logs.clone();
            let logger = thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    eprintln!("mpv: {line}");
                    let mut logs = logger_logs.lock().unwrap_or_else(|e| e.into_inner());
                    if logs.len() == 32 {
                        logs.pop_front();
                    }
                    logs.push_back(line);
                }
            });
            let (_, replies) = mpsc::channel();
            session = Some(Session {
                child: OwnedPlayer::Process(child),
                writer: None,
                replies,
                reader: None,
                logger: Some(logger),
                logs,
                dir,
                endpoint,
                request_id: 0,
                file_loaded: false,
            });
        }
        let mut session = session.ok_or("No native session")?;
        let (tx, replies) = mpsc::channel();
        session.replies = replies;
        let deadline = Instant::now() + Duration::from_secs(30);
        let stream = loop {
            #[cfg(unix)]
            let connected = Ipc::connect(&session.endpoint);
            #[cfg(windows)]
            let connected = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&session.endpoint);
            if let Ok(stream) = connected {
                break stream;
            }
            if let Some(exit) = session.child.try_wait().map_err(|e| e.to_string())? {
                return Err(session.error(format!("mpv exited at startup: {exit}")));
            }
            if Instant::now() >= deadline {
                return Err(session.error("Timed out connecting to mpv IPC"));
            }
            thread::sleep(Duration::from_millis(25));
        };
        let reader = stream.try_clone().map_err(|e| e.to_string())?;
        session.writer = Some(stream);
        session.reader = Some(thread::spawn(move || {
            for line in BufReader::new(reader).lines().map_while(Result::ok) {
                if let Ok(value) = serde_json::from_str(&line) {
                    if tx.send(value).is_err() {
                        break;
                    }
                }
            }
        }));
        #[cfg(any(target_os = "linux", windows))]
        if embedded.is_some() {
            session.set("pause", json!(true))?;
            session.request(embedded_load(
                &paths,
                duration,
                if paths.len() == 2 {
                    Some(self.graph()?)
                } else {
                    None
                },
            ))?;
            while !session.file_loaded {
                let event = session
                    .replies
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .map_err(|e| format!("Wait for libmpv file-loaded: {e}"))?;
                session.file_loaded = event["event"] == "file-loaded";
                if event["event"] == "end-file" && event["reason"] == "error" {
                    return Err(format!("libmpv loading failed: {event}"));
                }
            }
        }
        loop {
            if session
                .get("video-out-params")
                .ok()
                .filter(|v| !v.is_null())
                .is_some()
            {
                break;
            }
            if let Some(exit) = session.child.try_wait().map_err(|e| e.to_string())? {
                return Err(session.error(format!("mpv exited loading media: {exit}")));
            }
            if Instant::now() >= deadline {
                return Err(session.error("Timed out waiting for decoded video"));
            }
            thread::sleep(Duration::from_millis(50));
        }
        // Read actual mpv track IDs rather than assuming each file has exactly one audio track.
        if let Some(tracks) = session.get("track-list")?.as_array() {
            for track in tracks.iter().filter(|t| t["type"] == "audio") {
                let index = source_slots[usize::from(track["external"].as_bool().unwrap_or(false))];
                if self.audio_ids[index].is_none() {
                    self.audio_ids[index] = track["id"].as_u64();
                }
            }
        }
        if paths.len() == 2 {
            session.set("lavfi-complex", json!(self.graph()?))?;
        }
        session.set("vf", json!(comparison_vf(&self.status.mode, self.status.amount, self.size.0, self.size.1)))?;
        session.set("mute", json!(self.audio_ids[source_slots[0]].is_none()))?;
        self.session = Some(session);
        self.status.running = true;
        Ok(())
    }
    fn refresh(&mut self) -> Result<NativeStatus, String> {
        #[cfg(target_os = "linux")]
        if let Some(player) = self.gpu_slider() { self.status.amount = player.split_amount().unwrap_or(self.status.amount); }
        if let Some(session) = self.session.as_mut() {
            if session
                .child
                .try_wait()
                .map_err(|e| e.to_string())?
                .is_some()
            {
                self.stop();
            } else {
                self.status.time = session
                    .get("time-pos")?
                    .as_f64()
                    .unwrap_or(0.0)
                    .min(self.status.duration);
                self.status.paused = session.get("pause")?.as_bool().unwrap_or(true);
                self.status.volume = session
                    .get("volume")?
                    .as_f64()
                    .unwrap_or(self.status.volume);
            }
        }
        Ok(self.status.clone())
    }
    fn control(&mut self, action: &str, value: Value) -> Result<NativeStatus, String> {
        #[cfg(target_os = "linux")]
        if let Some(player) = self.gpu_slider() {
            if action == "amount" {
                let amount = numeric(&value, 0.0, 1.0)?;
                player.set_split_amount(amount);
                self.status.amount = amount;
                return Ok(self.status.clone());
            }
            self.status.amount = player.split_amount().unwrap_or(self.status.amount);
        }
        if action == "stop" {
            self.stop();
            return Ok(self.status.clone());
        }
        if self.session.is_none() {
            return Err("Open a video first".into());
        }
        if matches!(action, "seek" | "sync" | "stepForward" | "stepBackward" | "playPause")
            && matches!(self.status.mode.as_str(), "slider" | "blend")
            && !self.uses_gpu_slider()
            && self.status.paths.iter().filter(|p| !p.is_empty()).count() == 2
        {
            // A seek recreates lavfi, so carry the latest live expression into its stored options.
            self.session.as_mut().unwrap().set("vf", json!(comparison_vf(
                &self.status.mode, self.status.amount, self.size.0, self.size.1
            )))?;
        }
        match action {
            "playPause" => {
                self.session
                    .as_mut()
                    .unwrap()
                    .request(json!(["cycle", "pause"]))?;
            }
            "seek" => {
                let seconds = numeric(&value, 0.0, self.status.duration)?;
                self.session.as_mut().unwrap().request(json!([
                    "seek",
                    seconds,
                    "absolute+exact"
                ]))?;
            }
            "sync" => {
                let time = self
                    .session
                    .as_mut()
                    .unwrap()
                    .get("time-pos")?
                    .as_f64()
                    .unwrap_or(0.0);
                self.session
                    .as_mut()
                    .unwrap()
                    .request(json!(["seek", time, "absolute+exact"]))?;
            }
            "stepForward" | "stepBackward" => {
                self.session
                    .as_mut()
                    .unwrap()
                    .request(json!([if action == "stepForward" {
                        "frame-step"
                    } else {
                        "frame-back-step"
                    }]))?;
            }
            "fullscreen" => {
                if let Some(enabled) = value.as_bool() {
                    self.session
                        .as_mut()
                        .unwrap()
                        .set("fullscreen", json!(enabled))?;
                } else {
                    self.session
                        .as_mut()
                        .unwrap()
                        .request(json!(["cycle", "fullscreen"]))?;
                }
            }
            "volume" => {
                let volume = numeric(&value, 0.0, 100.0)?;
                self.session
                    .as_mut()
                    .unwrap()
                    .set("volume", json!(volume))?;
                self.status.volume = volume;
            }
            "mode" | "amount" | "audio" => {
                let old = self.status.clone();
                match action {
                    "mode" => {
                        let mode = value.as_str().ok_or("Mode must be a string")?;
                        video_filter(mode, self.status.amount, self.size.0, self.size.1)?;
                        self.status.mode = mode.into();
                    }
                    "amount" => self.status.amount = numeric(&value, 0.0, 1.0)?,
                    "audio" => {
                        let audio = value
                            .as_str()
                            .ok_or("Audio must be a, b, or mute")?
                            .to_lowercase();
                        if !["a", "b", "mute"].contains(&audio.as_str()) {
                            return Err("Audio must be a, b, or mute".into());
                        }
                        self.status.audio = if audio == "mute" {
                            audio
                        } else {
                            audio.to_uppercase()
                        };
                    }
                    _ => unreachable!(),
                }
                let result = (|| {
                    if self
                        .status
                        .paths
                        .iter()
                        .filter(|path| !path.is_empty())
                        .count()
                        == 2
                    {
                        if action == "amount" && matches!(self.status.mode.as_str(), "slider" | "blend") {
                            if self.status.paused {
                                // Seeking resets lavfi runtime commands; persist the expression for paused redraws.
                                self.session.as_mut().unwrap().set("vf", json!(comparison_vf(
                                    &self.status.mode, self.status.amount, self.size.0, self.size.1
                                )))?;
                            } else {
                                let command = json!([
                                    "vf-command", "compare", "all_expr",
                                    amount_expression(&self.status.mode, self.status.amount), "blend@mix"
                                ]);
                                // mpv acknowledges vf replacement before its first frame initializes lavfi.
                                let deadline = Instant::now() + Duration::from_millis(250);
                                loop {
                                    match self.session.as_mut().unwrap().request(command.clone()) {
                                        Ok(_) => break,
                                        Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
                                        Err(error) => return Err(error),
                                    }
                                }
                            }
                        } else if action != "amount" {
                            let graph = self.graph()?;
                            self.session.as_mut().unwrap().set("vf", json!(""))?;
                            self.session.as_mut().unwrap().set("lavfi-complex", json!(graph))?;
                            let vf = if self.uses_gpu_slider() { String::new() } else {
                                comparison_vf(&self.status.mode, self.status.amount, self.size.0, self.size.1)
                            };
                            self.session.as_mut().unwrap().set("vf", json!(vf))?;
                        }
                    }
                    let index = usize::from(self.status.audio == "B");
                    let mute = self.status.audio == "mute" || self.audio_ids[index].is_none();
                    self.session.as_mut().unwrap().set("mute", json!(mute))
                })();
                if let Err(error) = result {
                    self.status = old;
                    return Err(error);
                }
                #[cfg(target_os = "linux")]
                if let Some(player) = &self.embedded {
                    player.configure_split(if self.uses_gpu_slider() { Some((self.status.amount, self.size)) } else { None });
                }
            }
            _ => return Err(format!("Unknown native control: {action}")),
        }
        self.refresh()
    }
}

#[tauri::command]
pub async fn native_open(
    state: State<'_, NativeState>,
    paths: Vec<String>,
) -> Result<NativeStatus, String> {
    let shared = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut engine = shared.lock().map_err(|e| e.to_string())?;
        engine.open(paths, false)?;
        engine.refresh()
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn native_control(
    window: tauri::WebviewWindow,
    state: State<'_, NativeState>,
    action: String,
    value: Value,
) -> Result<NativeStatus, String> {
    #[cfg(any(target_os = "linux", windows))]
    if action == "fullscreen" {
        let enabled = value
            .as_bool()
            .unwrap_or(!window.is_fullscreen().map_err(|e| e.to_string())?);
        window.set_fullscreen(enabled).map_err(|e| e.to_string())?;
        return native_status(state).await;
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    let _ = window;
    let shared = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        shared
            .lock()
            .map_err(|e| e.to_string())?
            .control(&action, value)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn native_status(state: State<'_, NativeState>) -> Result<NativeStatus, String> {
    let shared = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        shared.lock().map_err(|e| e.to_string())?.refresh()
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn native_probe(path: String) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || probe(&path))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn native_pick() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        Ok(rfd::FileDialog::new()
            .set_title("Select one or two videos")
            .pick_files()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires installed mpv/ffprobe and DAWVC_TEST_MEDIA"]
    fn real_media_opens_in_native_mpv() {
        let path = std::env::var("DAWVC_TEST_MEDIA").expect("set DAWVC_TEST_MEDIA");
        let mut engine = Engine::default();
        engine.open(vec![path], true).unwrap();
        let status = engine.refresh().unwrap();
        assert!(status.running && status.paused && status.duration > 0.0);
        let dir = engine.session.as_ref().unwrap().dir.clone();
        engine.stop();
        assert!(!dir.exists());
    }
    #[test]
    #[ignore = "requires mpv, ffprobe, ffmpeg"]
    fn native_comparison_controls_mismatched_videos_and_cleans_up() {
        let dir = std::env::temp_dir().join(format!("vc-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut paths = vec![];
        for (index, size, duration) in [(0, "320x180", "3"), (1, "160x120", "2")] {
            let path = dir.join(format!("video {index}.mkv"));
            assert!(Command::new("ffmpeg")
                .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
                .arg(format!("testsrc2=size={size}:rate=10"))
                .args([
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440",
                    "-t",
                    duration,
                    "-c:v",
                    "ffv1",
                    "-c:a",
                    "pcm_s16le"
                ])
                .arg(&path)
                .status()
                .unwrap()
                .success());
            paths.push(path.to_string_lossy().into_owned());
        }
        let mut engine = Engine::default();
        engine.open(paths.clone(), true).unwrap();
        assert_eq!(engine.refresh().unwrap().duration, 3.0);
        assert_eq!(engine.audio_ids, [Some(1), Some(2)]);
        assert!(engine
            .session
            .as_mut()
            .unwrap()
            .get("lavfi-complex")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("[aid1]anull[ao]"));
        for mode in ["slider", "blend", "diff", "side"] {
            assert_eq!(engine.control("mode", json!(mode)).unwrap().mode, mode);
            engine.control("amount", json!(0.25)).unwrap();
            engine.control("seek", json!(0.5)).unwrap();
            engine.control("stepForward", Value::Null).unwrap();
            engine.control("stepBackward", Value::Null).unwrap();
        }
        for mode in ["slider", "blend"] {
            engine.control("mode", json!(mode)).unwrap();
            engine.control("seek", json!(0.0)).unwrap();
            engine.control("playPause", Value::Null).unwrap();
            let ipc = engine.session.as_mut().unwrap();
            let graph = ipc.get("lavfi-complex").unwrap();
            let vf = ipc.get("vf").unwrap();
            for amount in [0.1, 0.4, 0.8] {
                engine.control("amount", json!(amount)).unwrap();
                let ipc = engine.session.as_mut().unwrap();
                assert_eq!(ipc.get("lavfi-complex").unwrap(), graph, "live amounts must not rebuild the source graph");
                assert_eq!(ipc.get("vf").unwrap(), vf, "playing amounts use runtime commands, not filter replacement");
            }
            engine.control("playPause", Value::Null).unwrap();
            engine.control("amount", json!(0.2)).unwrap();
            let vf = engine.session.as_mut().unwrap().get("vf").unwrap().to_string();
            assert!(vf.contains(&amount_expression(mode, 0.2)), "paused redraw must retain its expression: {vf}");
            engine.control("seek", json!(0.5)).unwrap();
            assert_eq!(engine.status.amount, 0.2);
        }
        engine.control("mode", json!("side")).unwrap();
        for audio in ["A", "B", "mute"] {
            assert_eq!(engine.control("audio", json!(audio)).unwrap().audio, audio);
            let ipc = engine.session.as_mut().unwrap();
            assert_eq!(ipc.get("mute").unwrap(), json!(audio == "mute"));
            let expected = if audio == "B" {
                "[aid2]anull[ao]"
            } else {
                "[aid1]anull[ao]"
            };
            assert!(ipc
                .get("lavfi-complex")
                .unwrap()
                .as_str()
                .unwrap()
                .contains(expected));
        }
        assert_eq!(engine.control("volume", json!(30)).unwrap().volume, 30.0);
        engine.control("sync", Value::Null).unwrap();
        // Frame back-step is asynchronous in mpv; allow its seek/pause event to settle.
        thread::sleep(Duration::from_millis(100));
        assert!(!engine.control("playPause", Value::Null).unwrap().paused);
        assert!(engine.control("playPause", Value::Null).unwrap().paused);
        engine.control("fullscreen", Value::Null).unwrap();
        let ipc_dir = engine.session.as_ref().unwrap().dir.clone();
        let pid = engine.session.as_ref().unwrap().child.id();
        assert!(engine.control("seek", json!("invalid")).is_err());
        assert!(engine.control("mode", json!("invalid")).is_err());
        assert!(engine.control("unrecognized", Value::Null).is_err());
        assert!(engine.control("stop", Value::Null).unwrap().running == false);
        assert!(!ipc_dir.exists());
        #[cfg(target_os = "linux")]
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
        for pair in [paths.clone(), vec![paths[1].clone(), paths[0].clone()]] {
            engine.open(pair, true).unwrap();
            for mode in ["side", "slider", "blend", "diff"] {
                engine.control("mode", json!(mode)).unwrap();
                engine.control("seek", json!(2.5)).unwrap();
                thread::sleep(Duration::from_millis(150));
                assert!(
                    engine.refresh().unwrap().time >= 2.4,
                    "Cannot seek into longer source tail in {mode}"
                );
                assert!(!engine
                    .session
                    .as_mut()
                    .unwrap()
                    .get("video-out-params")
                    .unwrap()
                    .is_null());
            }
        }
        engine
            .open(vec![String::new(), paths[1].clone()], true)
            .unwrap();
        assert_eq!(
            engine.refresh().unwrap().paths,
            vec![String::new(), paths[1].clone()]
        );
        assert_eq!(engine.status.audio, "B");
        assert_eq!(engine.audio_ids, [None, Some(1)]);
        assert_eq!(
            engine.session.as_mut().unwrap().get("mute").unwrap(),
            json!(false)
        );
        engine.stop();
        engine.open(vec![paths[0].clone()], true).unwrap();
        engine.stop();
        let silent = dir.join("silent.mkv");
        assert!(Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-i"])
            .arg(&paths[0])
            .args(["-c:v", "copy", "-an"])
            .arg(&silent)
            .status()
            .unwrap()
            .success());
        engine
            .open(
                vec![silent.to_string_lossy().into_owned(), paths[1].clone()],
                true,
            )
            .unwrap();
        assert_eq!(
            engine.session.as_mut().unwrap().get("mute").unwrap(),
            json!(true)
        );
        engine.control("audio", json!("B")).unwrap();
        assert_eq!(
            engine.session.as_mut().unwrap().get("mute").unwrap(),
            json!(false)
        );
        engine.stop();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires libmpv"]
    fn gpu_slider_amount_does_not_use_ipc_even_when_paused() {
        let player = crate::embedded::Player::new().unwrap();
        player.configure_split(Some((0.5, (3840, 2160))));
        let (_, replies) = mpsc::channel();
        let mut engine = Engine::default();
        engine.status.paths = vec!["A".into(), "B".into()];
        engine.status.mode = "slider".into();
        engine.status.running = true;
        engine.session = Some(Session {
            child: OwnedPlayer::Embedded(player.clone()), writer: None, replies,
            reader: None, logger: None, logs: Arc::default(),
            dir: PathBuf::new(), endpoint: String::new(), request_id: 0, file_loaded: true,
        });
        for paused in [true, false] {
            engine.status.paused = paused;
            for amount in [0.0, 0.2, 0.8, 1.0] {
                assert_eq!(engine.control("amount", json!(amount)).unwrap().amount, amount);
                assert_eq!(player.split_amount(), Some(amount));
            }
        }
        assert_eq!(engine.session.as_ref().unwrap().request_id, 0);
        assert!(engine.control("amount", json!(1.01)).is_err());
        drop(engine);
        player.shutdown();
    }
    #[test]
    fn rejects_bad_paths_counts_and_control_values() {
        let mut engine = Engine::default();
        assert!(engine.open(vec![], true).is_err());
        assert!(engine.open(vec!["x".into(); 3], true).is_err());
        assert!(engine
            .open(vec!["/missing/dawvc-file".into()], true)
            .is_err());
        assert!(media_path("").is_err());
        assert!(media_path("invalid\0path").is_err());
        assert!(media_path(".").is_err());
        assert!(numeric(&json!("1"), 0.0, 1.0).is_err());
        assert!(numeric(&json!(-1), 0.0, 1.0).is_err());
        assert!(numeric(&json!(2), 0.0, 1.0).is_err());
        assert!(engine.control("playPause", Value::Null).is_err());
        assert!(!engine.control("stop", Value::Null).unwrap().running);
        assert_eq!(
            serde_json::to_value(engine.refresh().unwrap()).unwrap()["audio"],
            "A"
        );
        NativeState::default().shutdown();
    }
    #[test]
    fn embedded_load_options_preserve_originals_and_max_duration() {
        let command = embedded_load(
            &["/original A.mkv".into(), "/original B.mkv".into()],
            3.0,
            Some("[vid1][vid2]hstack[vo]".into()),
        );
        assert_eq!(command[1], "/original A.mkv");
        assert_eq!(command[4]["external-file"], "/original B.mkv");
        assert_eq!(command[4]["end"], "3.000000");
        assert_eq!(command[4]["lavfi-complex"], "[vid1][vid2]hstack[vo]");
        assert!(embedded_load(&["/only B.mkv".into()], 2.0, None)[4]
            .get("external-file")
            .is_none());
    }
    #[test]
    fn comparison_filters_fit_b_to_a_and_have_mpv_output() {
        for mode in ["side", "slider", "blend", "diff"] {
            let graph = video_filter(mode, 0.5, 640, 360).unwrap();
            assert!(graph.contains("[vid1]"));
            assert!(graph.contains("[vid2]"));
            assert!(graph.contains("force_original_aspect_ratio=decrease"));
            assert!(graph.contains("pad=640:360"));
            assert!(graph.ends_with("[vo]"));
        }
        assert!(video_filter("unknown", 0.5, 640, 360).is_err());
    }
}
