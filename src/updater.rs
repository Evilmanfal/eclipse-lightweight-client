//! Checks GitHub for a newer Eclipse release at launch and, when asked, installs it: the new
//! Eclipse.exe is downloaded, checked against the release's SHA256SUMS.txt, swapped in place of
//! the running exe and started.
use crossbeam_channel::{bounded, Receiver};
use eframe::egui;
use serde_json::Value;
use std::{path::PathBuf, time::Duration};

const REPO: &str = "Evilmanfal/eclipse-lightweight-client";
const MAX_EXE: u64 = 200 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct Release { pub version: String, exe: String, sums: String }

#[derive(Default)]
pub enum State {
    #[default]
    Idle,
    Checking(Receiver<Option<Release>>),
    /// A newer release was found and the prompt is showing.
    Prompt(Release),
    /// The user said No: the green update button stays in the top right.
    Later(Release),
    Installing(Release, Receiver<Result<PathBuf, String>>),
    Failed(Release, String),
}

#[derive(Default)]
pub struct Updater { pub state: State }

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent(concat!("Eclipse/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "Could not start a secure connection to GitHub.".to_owned())
}

/// "v0.8.10" → [0, 8, 10]; anything else is not a version.
fn version(tag: &str) -> Option<[u64; 3]> {
    let mut parts = tag.trim().trim_start_matches(['v', 'V']).split('.').map(|p| p.parse::<u64>().ok());
    let result = [parts.next()??, parts.next()??, parts.next().unwrap_or(Some(0))?];
    parts.next().is_none().then_some(result)
}
fn newer(tag: &str, current: &str) -> bool { version(tag).zip(version(current)).is_some_and(|(a, b)| a > b) }

/// The latest published release, if it is newer than this build and carries Eclipse.exe and its checksum.
fn parse(release: &Value, current: &str) -> Option<Release> {
    let tag = release["tag_name"].as_str()?;
    if release["draft"].as_bool() == Some(true) || release["prerelease"].as_bool() == Some(true) || !newer(tag, current) { return None; }
    let prefix = format!("https://github.com/{REPO}/releases/download/");
    let asset = |name: &str| release["assets"].as_array()?.iter()
        .find(|a| a["name"].as_str() == Some(name))
        .and_then(|a| a["browser_download_url"].as_str())
        .filter(|url| url.starts_with(&prefix))
        .map(str::to_owned);
    Some(Release { version: tag.trim_start_matches(['v', 'V']).to_owned(), exe: asset("Eclipse.exe")?, sums: asset("SHA256SUMS.txt")? })
}

/// The leftover copy of the previous version, removed on the next launch.
fn old_copy(exe: &std::path::Path) -> PathBuf { exe.with_extension("exe.old") }

impl Updater {
    /// Starts the launch-time check in the background.
    pub fn check(&mut self, ctx: &egui::Context) {
        if let Ok(exe) = std::env::current_exe() { let _ = std::fs::remove_file(old_copy(&exe)); }
        let (tx, rx) = bounded(1);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let latest = client().ok()
                .and_then(|c| c.get(format!("https://api.github.com/repos/{REPO}/releases/latest")).header("Accept", "application/vnd.github+json").send().ok())
                .filter(|r| r.status().is_success())
                .and_then(|r| r.json::<Value>().ok())
                .and_then(|release| parse(&release, env!("CARGO_PKG_VERSION")));
            let _ = tx.send(latest);
            ctx.request_repaint();
        });
        self.state = State::Checking(rx);
    }
    /// Offline preview screenshots: the prompt, or the button after choosing No.
    pub fn preview(&mut self, later: bool) {
        let release = Release { version: "9.9.9".into(), exe: String::new(), sums: String::new() };
        self.state = if later { State::Later(release) } else { State::Prompt(release) };
    }
    /// Whether the green update button should show.
    pub fn later(&self) -> bool { matches!(self.state, State::Later(_) | State::Failed(..)) }
    pub fn reopen(&mut self) {
        if let State::Later(release) | State::Failed(release, _) = std::mem::take(&mut self.state) { self.state = State::Prompt(release); }
    }
    pub fn decline(&mut self) {
        if let State::Prompt(release) = std::mem::take(&mut self.state) { self.state = State::Later(release); }
    }
    pub fn install(&mut self, ctx: &egui::Context) {
        let State::Prompt(release) = std::mem::take(&mut self.state) else { return };
        let (tx, rx) = bounded(1);
        let (ctx, job) = (ctx.clone(), release.clone());
        std::thread::spawn(move || { let _ = tx.send(download_and_replace(&job)); ctx.request_repaint(); });
        self.state = State::Installing(release, rx);
    }
    /// Advances background work; returns the installed exe once it is ready to start.
    pub fn poll(&mut self) -> Option<PathBuf> {
        match std::mem::take(&mut self.state) {
            State::Checking(rx) => match rx.try_recv() {
                Ok(Some(release)) => self.state = State::Prompt(release),
                Ok(None) | Err(crossbeam_channel::TryRecvError::Disconnected) => {}
                Err(crossbeam_channel::TryRecvError::Empty) => self.state = State::Checking(rx),
            },
            State::Installing(release, rx) => match rx.try_recv() {
                Ok(Ok(exe)) => return Some(exe),
                Ok(Err(error)) => self.state = State::Failed(release, error),
                Err(crossbeam_channel::TryRecvError::Disconnected) => self.state = State::Failed(release, "The update stopped unexpectedly.".into()),
                Err(crossbeam_channel::TryRecvError::Empty) => self.state = State::Installing(release, rx),
            },
            other => self.state = other,
        }
        None
    }
}

fn download_and_replace(release: &Release) -> Result<PathBuf, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let client = client()?;
    let get = |url: &str, cap: u64| -> Result<Vec<u8>, String> {
        let response = client.get(url).send().map_err(|_| "Could not reach GitHub.".to_owned())?;
        if !response.status().is_success() { return Err(format!("GitHub refused the download ({}).", response.status().as_u16())); }
        let mut bytes = vec![];
        response.take(cap + 1).read_to_end(&mut bytes).map_err(|_| "The download was interrupted.".to_owned())?;
        if bytes.len() as u64 > cap { return Err("The download is unexpectedly large.".into()); }
        Ok(bytes)
    };
    let sums = String::from_utf8(get(&release.sums, 64 * 1024)?).map_err(|_| "The release checksum file is unreadable.".to_owned())?;
    let expected = sums.lines().find_map(|line| line.split_once("  ").filter(|(_, name)| name.trim() == "Eclipse.exe").map(|(hash, _)| hash.trim().to_ascii_lowercase()))
        .ok_or("The release has no checksum for Eclipse.exe.")?;
    let bytes = get(&release.exe, MAX_EXE)?;
    let actual: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    if actual != expected { return Err("The download did not match the release checksum, so it was not installed.".into()); }
    // Windows lets a running exe be renamed but not overwritten: move it aside, then write the new one.
    let exe = std::env::current_exe().map_err(|_| "Could not find Eclipse.exe.".to_owned())?;
    let old = old_copy(&exe);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).map_err(|_| "Could not replace Eclipse.exe. Is the folder read-only?".to_owned())?;
    if std::fs::write(&exe, &bytes).is_err() {
        let _ = std::fs::rename(&old, &exe);
        return Err("Could not write the new Eclipse.exe.".into());
    }
    Ok(exe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn versions_compare_numerically() {
        assert!(newer("v0.8.3", "0.8.2") && newer("v0.10.0", "0.9.9") && newer("1.0", "0.8.2"));
        assert!(!newer("v0.8.2", "0.8.2") && !newer("v0.8.1", "0.8.2") && !newer("latest", "0.8.2") && !newer("v0.8.3-beta", "0.8.2"));
    }
    #[test]
    fn only_newer_releases_with_both_assets_from_this_repo_are_offered() {
        let url = |name: &str| format!("https://github.com/{REPO}/releases/download/v0.8.3/{name}");
        let release = json!({"tag_name":"v0.8.3","assets":[{"name":"Eclipse.exe","browser_download_url":url("Eclipse.exe")},{"name":"SHA256SUMS.txt","browser_download_url":url("SHA256SUMS.txt")}]});
        assert_eq!(parse(&release, "0.8.2").map(|r| r.version), Some("0.8.3".into()));
        assert!(parse(&release, "0.8.3").is_none());
        let mut elsewhere = release.clone();
        elsewhere["assets"][0]["browser_download_url"] = json!("https://example.com/Eclipse.exe");
        assert!(parse(&elsewhere, "0.8.2").is_none());
        let mut draft = release.clone(); draft["prerelease"] = json!(true);
        assert!(parse(&draft, "0.8.2").is_none());
    }
    #[test]
    fn no_hides_the_prompt_and_shows_the_button_until_reopened() {
        let release = Release { version: "0.8.3".into(), exe: String::new(), sums: String::new() };
        let mut updater = Updater { state: State::Prompt(release) };
        assert!(!updater.later());
        updater.decline(); assert!(updater.later());
        updater.reopen(); assert!(matches!(updater.state, State::Prompt(_)) && !updater.later());
    }
}
