use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: String,
    architecture: String,
    assets: Vec<Asset>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Asset {
    name: String,
    url: String,
    sha256: String,
    size: u64,
    source: String,
    files: Vec<Artifact>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Artifact {
    entry: String,
    output: String,
    size: u64,
}
fn install_to(root: &Path, manifest: &Manifest) -> Result<(), String> {
    validate_manifest(manifest)?;
    fs::create_dir_all(root).map_err(err)?;
    ensure_directory(root)?;
    let versions = root.join("versions");
    fs::create_dir_all(&versions).map_err(err)?;
    ensure_directory(&versions)?;
    let destination = versions.join(&manifest.version);
    if destination.exists() {
        let receipt = read_receipt(&destination)
            .ok_or("Existing runtime version is incomplete; it was not overwritten")?;
        if receipt.manifest != *manifest {
            return Err("Existing version has different provenance; it was not overwritten".into());
        }
        for file in &receipt.files {
            let mut input = fs::File::open(destination.join(&file.name)).map_err(err)?;
            let mut digest = Sha256::new();
            std::io::copy(&mut input, &mut digest).map_err(err)?;
            if format!("{:x}", digest.finalize()) != file.sha256 {
                return Err("Existing runtime checksum mismatch; it was not overwritten".into());
            }
        }
    } else {
        let stage = tempfile::Builder::new()
            .prefix(".install-")
            .tempdir_in(root)
            .map_err(err)?;
        let payload = stage.path().join("payload");
        fs::create_dir(&payload).map_err(err)?;
        let deadline = Instant::now() + Duration::from_secs(900);
        let client = download_client()?;
        let total: u64 = manifest.assets.iter().map(|a| a.size).sum();
        let mut completed = 0;
        let mut files = Vec::new();
        for (index, asset) in manifest.assets.iter().enumerate() {
            let archive = stage.path().join(format!("{index}.7z"));
            download(&client, asset, &archive, deadline, completed, total)?;
            completed += asset.size;
            set_progress(85, format!("Verifying and extracting {}", asset.name));
            files.extend(extract(&archive, asset, &payload, deadline)?);
            fs::remove_file(&archive).map_err(err)?;
        }
        let receipt = Receipt {
            manifest: manifest.clone(),
            files,
        };
        fs::write(
            payload.join("provenance.json"),
            serde_json::to_vec_pretty(&receipt).map_err(err)?,
        )
        .map_err(err)?;
        fs::write(payload.join("RUNTIME-NOTICES.txt"), "Runtime downloaded on explicit request. Archive URLs, exact hashes and build provenance are in provenance.json.\nlibmpv is built by shinchiro/mpv-winbuild-cmake: https://github.com/shinchiro/mpv-winbuild-cmake\nmpv source: https://github.com/mpv-player/mpv (revision 492bae0768 for runtime 20261015); dependency versions/build recipes are maintained by the upstream builder.\nffprobe is built by Gyan; bundled ffprobe-LICENSE.txt and ffprobe-README.txt retain its license and build/source information.\nThese GPL components are provided without warranty; consult the upstream builders for corresponding source and dependency licensing.\n").map_err(err)?;
        fs::write(payload.join("GPL-3.0.txt"), include_str!("../../LICENSE")).map_err(err)?;
        if read_receipt(&payload).is_none() {
            return Err("Extracted runtime failed validation".into());
        }
        fs::rename(&payload, &destination).map_err(err)?;
        // The version is immutable after this rename; loaded DLLs are never replaced.
    }
    set_progress(98, "Activating verified runtime".into());
    let mut pointer = tempfile::NamedTempFile::new_in(root).map_err(err)?;
    pointer
        .write_all(manifest.version.as_bytes())
        .map_err(err)?;
    pointer.as_file().sync_all().map_err(err)?;
    pointer.persist(root.join("current")).map_err(err)?;
    Ok(())
}

fn installed_at(root: &Path) -> Option<(PathBuf, String)> {
    ensure_directory(root).ok()?;
    ensure_directory(&root.join("versions")).ok()?;
    let pointer = root.join("current");
    let meta = fs::symlink_metadata(&pointer).ok()?;
    if !meta.is_file() || meta.len() > 64 {
        return None;
    }
    let version = fs::read_to_string(pointer).ok()?;
    if !valid_version(&version) {
        return None;
    }
    let dir = root.join("versions").join(&version);
    let receipt = read_receipt(&dir)?;
    (receipt.manifest.version == version).then_some((dir, version))
}

const SUPPORTED: bool = cfg!(all(windows, target_arch = "x86_64"));
const MANIFEST: &str = include_str!("../runtime-windows.json");
const MAX_ARCHIVE: u64 = 96 * 1024 * 1024;
const MAX_UNPACKED: u64 = 512 * 1024 * 1024;
static BUSY: AtomicBool = AtomicBool::new(false);
static PROGRESS: Mutex<(u8, String)> = Mutex::new((0, String::new()));

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub supported: bool,
    pub installed: bool,
    pub installed_version: Option<String>,
    pub available_version: String,
    pub update_available: bool,
    pub busy: bool,
    pub progress: u8,
    pub message: String,
}

pub fn runtime_dir() -> Result<PathBuf, String> {
    if !SUPPORTED {
        return Err("App-managed runtime is supported only on Windows x86_64. On Linux, install mpv/libmpv and ffprobe using your package manager.".into());
    }
    let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?;
    let root = PathBuf::from(local);
    if !root.is_absolute() {
        return Err("LOCALAPPDATA must be an absolute path".into());
    }
    Ok(root.join("DaSiWa-simple-video-compare").join("runtime"))
}

pub fn installed_dir() -> Option<PathBuf> {
    installed_at(&runtime_dir().ok()?).map(|(dir, _)| dir)
}

#[tauri::command]
pub async fn runtime_status() -> Result<RuntimeStatus, String> {
    status()
}

fn status() -> Result<RuntimeStatus, String> {
    let manifest: Manifest = serde_json::from_str(MANIFEST).map_err(err)?;
    let installed_version = runtime_dir()
        .ok()
        .and_then(|root| installed_at(&root))
        .map(|(_, version)| version);
    let (progress, message) = PROGRESS.lock().map_err(err)?.clone();
    Ok(RuntimeStatus {
        supported: SUPPORTED,
        installed: installed_version.is_some(),
        update_available: installed_version
            .as_ref()
            .is_some_and(|v| v < &manifest.version),
        installed_version,
        available_version: manifest.version,
        busy: BUSY.load(Ordering::Acquire),
        progress,
        message: if !SUPPORTED {
            "Use your Linux package manager to install mpv/libmpv and ffprobe. App-managed downloads require Windows x86_64.".into()
        } else {
            message
        },
    })
}

struct InstallGuard;
impl InstallGuard {
    fn acquire() -> Result<Self, String> {
        BUSY.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "A runtime installation is already in progress".to_string())?;
        Ok(Self)
    }
}
impl Drop for InstallGuard {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}

#[tauri::command]
pub async fn runtime_install() -> Result<RuntimeStatus, String> {
    let root = runtime_dir()?;
    let guard = InstallGuard::acquire()?;
    set_progress(0, "Preparing app-managed runtime download".into());
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        let result = serde_json::from_str::<Manifest>(MANIFEST)
            .map_err(err)
            .and_then(|m| install_to(&root, &m));
        match &result {
            Ok(()) => set_progress(100, "Runtime ready. Open a video to use it.".into()),
            Err(error) => set_progress(0, format!("Runtime installation failed: {error}")),
        }
        result
    })
    .await
    .map_err(err)??;
    status()
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
fn set_progress(progress: u8, message: String) {
    if let Ok(mut state) = PROGRESS.lock() {
        *state = (progress, message);
    }
}
fn ensure_directory(path: &Path) -> Result<(), String> {
    if !fs::symlink_metadata(path).map_err(err)?.is_dir() {
        return Err("Runtime directory must not be a symlink".into());
    }
    Ok(())
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    manifest: Manifest,
    files: Vec<InstalledFile>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InstalledFile {
    name: String,
    size: u64,
    sha256: String,
}

fn read_receipt(dir: &Path) -> Option<Receipt> {
    ensure_directory(dir).ok()?;
    let path = dir.join("provenance.json");
    let meta = fs::symlink_metadata(&path).ok()?;
    if !meta.is_file() || meta.len() > 32768 {
        return None;
    }
    let receipt: Receipt = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    validate_manifest(&receipt.manifest).ok()?;
    let expected: Vec<_> = receipt
        .manifest
        .assets
        .iter()
        .flat_map(|a| &a.files)
        .collect();
    if receipt.files.len() != expected.len() {
        return None;
    }
    for artifact in expected {
        let file = receipt.files.iter().find(|f| f.name == artifact.output)?;
        if file.size != artifact.size || !valid_hash(&file.sha256) {
            return None;
        }
        let meta = fs::symlink_metadata(dir.join(&file.name)).ok()?;
        if !meta.is_file() || meta.len() != file.size {
            return None;
        }
    }
    Some(receipt)
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn validate_manifest(manifest: &Manifest) -> Result<(), String> {
    if !valid_version(&manifest.version)
        || manifest.architecture != "x86_64"
        || manifest.assets.is_empty()
        || manifest.assets.len() > 4
    {
        return Err("Invalid runtime manifest".into());
    }
    let mut names = HashSet::new();
    let mut total = 0u64;
    for asset in &manifest.assets {
        if !valid_version(&asset.name)
            || !valid_hash(&asset.sha256)
            || asset.size == 0
            || asset.size > MAX_ARCHIVE
            || asset.files.is_empty()
            || asset.files.len() > 16
        {
            return Err("Invalid runtime asset".into());
        }
        let url = reqwest::Url::parse(&asset.url).map_err(err)?;
        if !allowed_url(&url) {
            return Err("Runtime URL is not an approved HTTPS origin".into());
        }
        total += asset.size;
        for file in &asset.files {
            if !safe_entry(&file.entry)
                || !safe_entry(&file.output)
                || file.output.contains('/')
                || file.size == 0
                || file.size > MAX_UNPACKED
                || !names.insert(file.output.to_ascii_lowercase())
            {
                return Err("Invalid runtime artifact".into());
            }
        }
    }
    if total > 128 * 1024 * 1024
        || !names.contains("libmpv-2.dll")
        || !names.contains("ffprobe.exe")
    {
        return Err("Incomplete or oversized runtime manifest".into());
    }
    Ok(())
}

fn allowed_url(url: &reqwest::Url) -> bool {
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return false;
    }
    #[cfg(test)]
    if url.scheme() == "http" && url.host_str() == Some("127.0.0.1") {
        return true;
    }
    url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && matches!(
            url.host_str(),
            Some(
                "github.com"
                    | "release-assets.githubusercontent.com"
                    | "objects.githubusercontent.com"
            )
        )
}

fn download_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent("DaSiWa-simple-video-compare/0.4.0")
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(300))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || !allowed_url(attempt.url()) {
                attempt.error("Unapproved runtime download redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(err)
}

fn download(
    client: &reqwest::blocking::Client,
    asset: &Asset,
    path: &Path,
    deadline: Instant,
    completed: u64,
    total: u64,
) -> Result<(), String> {
    let mut response = client
        .get(&asset.url)
        .send()
        .map_err(err)?
        .error_for_status()
        .map_err(err)?;
    if response
        .content_length()
        .is_some_and(|size| size != asset.size)
    {
        return Err(format!(
            "{} download size does not match the pinned manifest",
            asset.name
        ));
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(err)?;
    let mut digest = Sha256::new();
    let mut received = 0u64;
    let mut buffer = [0; 65536];
    loop {
        if Instant::now() > deadline {
            return Err("Runtime installation timed out".into());
        }
        let count = response.read(&mut buffer).map_err(err)?;
        if count == 0 {
            break;
        }
        received += count as u64;
        if received > asset.size {
            return Err("Download exceeds pinned size".into());
        }
        digest.update(&buffer[..count]);
        output.write_all(&buffer[..count]).map_err(err)?;
        set_progress(
            ((completed + received) * 80 / total) as u8,
            format!("Downloading {}", asset.name),
        );
    }
    if received != asset.size || format!("{:x}", digest.finalize()) != asset.sha256 {
        return Err(format!(
            "{} download failed size/SHA-256 verification",
            asset.name
        ));
    }
    output.sync_all().map_err(err)
}

fn extract(
    path: &Path,
    asset: &Asset,
    payload: &Path,
    deadline: Instant,
) -> Result<Vec<InstalledFile>, String> {
    let mut archive =
        sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty()).map_err(err)?;
    archive.set_thread_count(1);
    if archive.archive().files.len() > 128 {
        return Err("Too many runtime archive entries".into());
    }
    let mut total = 0u64;
    let mut seen = HashSet::new();
    for entry in &archive.archive().files {
        let unix_kind = (entry.windows_attributes >> 16) & 0o170000;
        if !safe_entry(&entry.name)
            || entry.is_anti_item
            || entry.windows_attributes & 0x400 != 0
            || (unix_kind != 0 && unix_kind != 0o100000 && unix_kind != 0o040000)
            || !seen.insert(entry.name.to_ascii_lowercase())
        {
            return Err("Unsafe runtime archive entry (path, link, or duplicate)".into());
        }
        total = total
            .checked_add(entry.size)
            .ok_or("Archive size overflow")?;
        if total > MAX_UNPACKED {
            return Err("Runtime archive exceeds extraction size limit".into());
        }
    }
    let mut extracted = Vec::new();
    archive
        .for_each_entries(|entry, reader| {
            if entry.is_directory {
                return Ok(true);
            }
            let wanted = asset.files.iter().find(|f| f.entry == entry.name);
            if wanted.is_some_and(|file| file.size != entry.size) {
                return Err(sevenz_rust2::Error::Other(
                    "Pinned artifact size mismatch".into(),
                ));
            }
            let mut output = wanted
                .map(|file| {
                    fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(payload.join(&file.output))
                })
                .transpose()?;
            let mut digest = Sha256::new();
            let mut copied = 0u64;
            let mut buffer = [0; 65536];
            loop {
                if Instant::now() > deadline {
                    return Err(sevenz_rust2::Error::Other(
                        "Runtime extraction timed out".into(),
                    ));
                }
                let count = reader.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                copied += count as u64;
                if copied > entry.size {
                    return Err(sevenz_rust2::Error::Other(
                        "Archive entry exceeds declared size".into(),
                    ));
                }
                if let Some(output) = &mut output {
                    output.write_all(&buffer[..count])?;
                    digest.update(&buffer[..count]);
                }
            }
            if copied != entry.size {
                return Err(sevenz_rust2::Error::Other("Truncated archive entry".into()));
            }
            if let Some(file) = wanted {
                output.as_ref().unwrap().sync_all()?;
                extracted.push(InstalledFile {
                    name: file.output.clone(),
                    size: copied,
                    sha256: format!("{:x}", digest.finalize()),
                });
            }
            Ok(true)
        })
        .map_err(err)?;
    if extracted.len() != asset.files.len() {
        return Err(format!(
            "{} archive is missing a required runtime file",
            asset.name
        ));
    }
    Ok(extracted)
}

fn valid_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn safe_entry(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 240
        && !name.contains(['\\', ':', '\0'])
        && name
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != ".." && !s.ends_with(['.', ' ']))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::{
        io::{Cursor, Read, Write},
        net::TcpListener,
        thread,
    };

    fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = sevenz_rust2::ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
        for (name, bytes) in entries {
            writer
                .push_archive_entry(
                    sevenz_rust2::ArchiveEntry::new_file(name),
                    Some(Cursor::new(*bytes)),
                )
                .unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn serve(bytes: Vec<u8>, advertised: usize) -> String {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/asset.7z", socket.local_addr().unwrap());
        thread::spawn(move || {
            let (mut stream, _) = socket.accept().unwrap();
            let mut request = [0; 4096];
            stream.read(&mut request).unwrap();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {advertised}\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            let _ = stream.write_all(&bytes);
        });
        url
    }

    fn fixture(version: &str) -> Manifest {
        let bytes = archive(&[
            ("libmpv-2.dll", b"MZ-mpv"),
            ("nested/ffprobe.exe", b"MZ-probe"),
        ]);
        Manifest {
            version: version.into(),
            architecture: "x86_64".into(),
            assets: vec![Asset {
                name: "fixture".into(),
                url: serve(bytes.clone(), bytes.len()),
                sha256: format!("{:x}", Sha256::digest(&bytes)),
                size: bytes.len() as u64,
                source: "test fixture".into(),
                files: vec![
                    Artifact {
                        entry: "libmpv-2.dll".into(),
                        output: "libmpv-2.dll".into(),
                        size: 6,
                    },
                    Artifact {
                        entry: "nested/ffprobe.exe".into(),
                        output: "ffprobe.exe".into(),
                        size: 8,
                    },
                ],
            }],
        }
    }

    #[test]
    fn successful_install_promotes_flat_files_and_keeps_prior_version() {
        let root = tempfile::tempdir().unwrap();
        install_to(root.path(), &fixture("old")).unwrap();
        install_to(root.path(), &fixture("new")).unwrap();
        let (dir, version) = installed_at(root.path()).unwrap();
        assert_eq!(version, "new");
        assert_eq!(fs::read(dir.join("ffprobe.exe")).unwrap(), b"MZ-probe");
        assert!(root.path().join("versions/old/libmpv-2.dll").is_file());
        assert_eq!(
            fs::read_dir(root.path()).unwrap().count(),
            2,
            "no stage remains"
        );
    }

    #[test]
    fn failed_hash_or_truncated_download_preserves_current_and_cleans_staging() {
        let root = tempfile::tempdir().unwrap();
        install_to(root.path(), &fixture("old")).unwrap();
        let mut bad = fixture("bad-hash");
        bad.assets[0].sha256 = "0".repeat(64);
        assert!(install_to(root.path(), &bad)
            .unwrap_err()
            .contains("SHA-256"));
        assert_eq!(installed_at(root.path()).unwrap().1, "old");
        let mut truncated = fixture("truncated");
        truncated.assets[0].url = serve(b"short".to_vec(), truncated.assets[0].size as usize);
        assert!(install_to(root.path(), &truncated).is_err());
        assert_eq!(installed_at(root.path()).unwrap().1, "old");
        assert!(!root.path().join("versions/bad-hash").exists());
        assert!(!root.path().join("versions/truncated").exists());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }

    #[test]
    fn archive_traversal_is_rejected_before_any_payload_is_written() {
        let root = tempfile::tempdir().unwrap();
        let bytes = archive(&[
            ("../escape.dll", b"malicious"),
            ("libmpv-2.dll", b"MZ-mpv"),
            ("nested/ffprobe.exe", b"MZ-probe"),
        ]);
        let mut manifest = fixture("traversal");
        manifest.assets[0].url = serve(bytes.clone(), bytes.len());
        manifest.assets[0].size = bytes.len() as u64;
        manifest.assets[0].sha256 = format!("{:x}", Sha256::digest(&bytes));
        assert!(install_to(root.path(), &manifest)
            .unwrap_err()
            .contains("Unsafe runtime archive entry"));
        assert!(!root.path().join("current").exists());
        assert!(!root.path().join("escape.dll").exists());
        assert_eq!(
            fs::read_dir(root.path()).unwrap().count(),
            1,
            "only empty versions directory remains"
        );
    }

    #[test]
    fn url_policy_rejects_foreign_hosts_credentials_and_downgrades() {
        for url in [
            "https://evil.example/a",
            "http://github.com/a",
            "https://user:pass@github.com/a",
            "https://github.com:444/a",
            "https://github.com.evil.example/a",
        ] {
            assert!(!allowed_url(&reqwest::Url::parse(url).unwrap()));
        }
        assert!(allowed_url(
            &reqwest::Url::parse("https://release-assets.githubusercontent.com/a").unwrap()
        ));
    }

    #[test]
    #[ignore = "downloads the pinned Windows runtime archives from GitHub"]
    fn real_catalog_download_installs_verified_windows_binaries() {
        let parent = std::env::var_os("DAWVC_RUNTIME_TEST_ROOT")
            .expect("Set DAWVC_RUNTIME_TEST_ROOT to a scratch directory");
        let root = tempfile::Builder::new()
            .prefix("runtime-verified-")
            .tempdir_in(parent)
            .unwrap();
        let manifest: Manifest = serde_json::from_str(MANIFEST).unwrap();
        install_to(root.path(), &manifest).unwrap();
        let (dir, version) = installed_at(root.path()).unwrap();
        assert_eq!(version, manifest.version);
        for name in ["libmpv-2.dll", "ffprobe.exe"] {
            let mut file = fs::File::open(dir.join(name)).unwrap();
            let mut signature = [0; 2];
            file.read_exact(&mut signature).unwrap();
            assert_eq!(&signature, b"MZ");
        }
        assert!(dir.join("ffprobe-LICENSE.txt").is_file());
        println!("Verified runtime retained at {}", root.keep().display());
    }

    #[test]
    fn identifiers_and_archive_paths_reject_windows_and_unix_escapes() {
        for name in ["", "..", "../old", "C:\\bad", "a/b", ".hidden"] {
            assert!(!valid_version(name), "unsafe version: {name}");
        }
        assert!(valid_version("20261015"));
        for name in [
            "../libmpv-2.dll",
            "/abs",
            "C:/abs",
            "a\\b",
            "a/../b",
            "a//b",
            "a:ads",
            "a/./b",
        ] {
            assert!(!safe_entry(name), "unsafe archive path: {name}");
        }
        assert!(safe_entry("ffmpeg-9.0.2-essentials_build/bin/ffprobe.exe"));
    }
}
