//! The SETUP window's About page: the project's website, and a check for a newer release on
//! GitHub that can download its files straight into the Downloads folder.
//!
//! Nothing is sent or checked unless the person presses the button. The work runs on its own
//! thread (the panel never waits) through the system's `curl` (built into Windows 10 and later,
//! macOS and Linux), so no web library is linked into the plugin.

use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};

pub const WEBSITE: &str = "https://www.geraniumproduction.be/ritmo6";
pub const REPO: &str = "CedricLawde/RITMO-6";
pub const RELEASES_PAGE: &str = "https://github.com/CedricLawde/RITMO-6/releases/latest";

/// Where the update check stands.
#[derive(Clone, Debug, Default)]
pub enum Update {
    #[default]
    Idle,
    Checking,
    UpToDate(String),
    Available(Release),
    Downloading { tag: String, done: usize, total: usize },
    Downloaded { tag: String, folder: PathBuf },
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct Release {
    pub tag: String,
    /// yyyy-mm-dd
    pub date: String,
    pub page: String,
    /// (file name, download address)
    pub files: Vec<(String, String)>,
}

pub type UpdateState = Arc<Mutex<Update>>;

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    published_at: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
}

/// "v2.10.1" -> (2, 10, 1); missing parts count as 0.
pub fn version_of(tag: &str) -> (u32, u32, u32) {
    let mut it = tag.trim().trim_start_matches(['v', 'V']).split('.').map(|p| {
        p.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<u32>().unwrap_or(0)
    });
    (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}

pub fn this_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn set(state: &UpdateState, u: Update) {
    if let Ok(mut s) = state.lock() {
        *s = u;
    }
}

pub fn get(state: &UpdateState) -> Update {
    state.lock().map(|s| s.clone()).unwrap_or_default()
}

/// A command with no console window popping up (Windows).
fn quiet(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

fn curl_text(url: &str) -> Result<String, String> {
    let ua = format!("Ritmo-6/{}", this_version());
    let out = quiet("curl")
        .args(["-s", "-S", "-L", "--fail", "--max-time", "20", "-A", &ua, "-H", "Accept: application/vnd.github+json", url])
        .output()
        .map_err(|e| format!("curl could not start ({e})"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() { "no answer from GitHub".into() } else { err });
    }
    String::from_utf8(out.stdout).map_err(|e| e.to_string())
}

/// Ask GitHub for the latest release, on its own thread.
pub fn check(state: &UpdateState) {
    if matches!(get(state), Update::Checking | Update::Downloading { .. }) {
        return;
    }
    set(state, Update::Checking);
    let state = state.clone();
    std::thread::spawn(move || {
        let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
        let result = curl_text(&url).and_then(|t| serde_json::from_str::<ApiRelease>(&t).map_err(|e| e.to_string()));
        let next = match result {
            Err(e) => Update::Failed(format!("Couldn't reach GitHub: {e}")),
            Ok(r) => {
                if version_of(&r.tag_name) > version_of(this_version()) {
                    Update::Available(Release {
                        tag: r.tag_name,
                        date: r.published_at.chars().take(10).collect(),
                        page: if r.html_url.is_empty() { RELEASES_PAGE.into() } else { r.html_url },
                        files: r.assets.into_iter().map(|a| (a.name, a.browser_download_url)).collect(),
                    })
                } else {
                    Update::UpToDate(r.tag_name)
                }
            }
        };
        set(&state, next);
    });
}

/// The Downloads folder (or the home folder).
fn downloads_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let d = home.join("Downloads");
    if d.is_dir() {
        d
    } else {
        home
    }
}

/// Download every file of a release into Downloads\Ritmo-6 <tag>, on its own thread, then
/// open that folder.
pub fn download(state: &UpdateState, release: Release) {
    let state = state.clone();
    std::thread::spawn(move || {
        let folder = downloads_dir().join(format!("Ritmo-6 {}", release.tag));
        if download_to(&state, &release, &folder) {
            open(&folder.to_string_lossy());
        }
    });
}

/// Download a release's files into `folder`; true when every file arrived.
fn download_to(state: &UpdateState, release: &Release, folder: &std::path::Path) -> bool {
    {
        let folder = folder.to_path_buf();
        if let Err(e) = std::fs::create_dir_all(&folder) {
            set(state, Update::Failed(format!("Couldn't make the folder {}: {e}", folder.display())));
            return false;
        }
        let total = release.files.len();
        let ua = format!("Ritmo-6/{}", this_version());
        for (i, (name, url)) in release.files.iter().enumerate() {
            set(state, Update::Downloading { tag: release.tag.clone(), done: i, total });
            // the file name comes from GitHub: keep only its last part
            let safe = std::path::Path::new(name).file_name().map(|n| n.to_owned()).unwrap_or_default();
            if safe.is_empty() {
                continue;
            }
            let ok = quiet("curl")
                .args(["-s", "-S", "-L", "--fail", "--max-time", "300", "-A", &ua, "-o"])
                .arg(folder.join(&safe))
                .arg(url)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !ok {
                set(state, Update::Failed(format!("Download of {name} failed. The release page has every file.")));
                return false;
            }
        }
        set(state, Update::Downloaded { tag: release.tag.clone(), folder });
        true
    }
}

/// Open a web address or a folder with the system (browser, file explorer).
pub fn open(target: &str) {
    #[cfg(windows)]
    let r = if target.starts_with("http") {
        quiet("rundll32").args(["url.dll,FileProtocolHandler", target]).spawn()
    } else {
        Command::new("explorer").arg(target).spawn()
    };
    #[cfg(target_os = "macos")]
    let r = Command::new("open").arg(target).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = Command::new("xdg-open").arg(target).spawn();
    let _ = r;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number() {
        assert_eq!(version_of("v2.2.0"), (2, 2, 0));
        assert_eq!(version_of("2.10.1"), (2, 10, 1));
        assert!(version_of("v2.10.0") > version_of("v2.9.3"));
        assert!(version_of("v2.2.0") <= version_of(this_version()));
        assert_eq!(version_of("v3"), (3, 0, 0));
    }

    /// Real network: run with `cargo test --release -- --ignored`.
    #[test]
    #[ignore]
    fn a_release_file_really_downloads() {
        let state = UpdateState::default();
        let release = Release {
            tag: "v2.2.0".into(),
            date: String::new(),
            page: RELEASES_PAGE.into(),
            files: vec![(
                "user-manual-ritmo6.png".into(),
                "https://github.com/CedricLawde/RITMO-6/releases/download/v2.2.0/user-manual-ritmo6.png".into(),
            )],
        };
        let folder = std::env::temp_dir().join("ritmo6-update-test");
        assert!(download_to(&state, &release, &folder), "{:?}", get(&state));
        let size = std::fs::metadata(folder.join("user-manual-ritmo6.png")).unwrap().len();
        assert_eq!(size, 1_217_554);
        assert!(matches!(get(&state), Update::Downloaded { .. }));
    }

    #[test]
    fn a_github_release_is_read() {
        let t = r#"{"tag_name":"v9.0.0","published_at":"2027-01-02T10:00:00Z","html_url":"https://x",
            "assets":[{"name":"Ritmo-6.vst3","browser_download_url":"https://y/Ritmo-6.vst3","size":1}]}"#;
        let r: ApiRelease = serde_json::from_str(t).unwrap();
        assert_eq!(r.tag_name, "v9.0.0");
        assert_eq!(r.assets[0].name, "Ritmo-6.vst3");
    }
}
