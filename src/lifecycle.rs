//! Per-user installation and verified release replacement. Never needs elevation.
use super::*;
use sha2::Digest;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

static BUSY: AtomicBool = AtomicBool::new(false);
struct Operation;
impl Operation {
    fn begin() -> Result<Self> {
        BUSY.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "An installation or update is already in progress")?;
        Ok(Self)
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::Release);
    }
}
#[derive(Default, Serialize, Deserialize)]
struct State {
    origins: Vec<String>,
    updated_version: String,
    updated_at: u64,
}
#[cfg(target_os = "linux")]
fn base(var: &str, fallback: &str) -> Result<PathBuf> {
    if let Some(p) = std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        return Ok(p);
    }
    Ok(PathBuf::from(std::env::var_os("HOME").ok_or("Home directory unavailable")?).join(fallback))
}
pub fn root() -> Result<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        Ok(
            PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA unavailable")?)
                .join("FindOut"),
        )
    }
    #[cfg(target_os = "macos")]
    {
        Ok(mac_home()?.join("Library/Application Support/FindOut"))
    }
    #[cfg(target_os = "linux")]
    {
        Ok(base("XDG_DATA_HOME", ".local/share")?.join("findout"))
    }
}
fn executable() -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    return Ok(app_bundle()?.join("Contents/MacOS/findout-client"));
    #[cfg(not(target_os = "macos"))]
    Ok(root()?.join(if cfg!(windows) {
        "findout-client.exe"
    } else {
        "findout-client"
    }))
}
pub fn installed() -> bool {
    executable()
        .ok()
        .zip(std::env::current_exe().ok())
        .is_some_and(|(a, b)| a == b)
}
pub fn instance_lock() -> Result<Option<fs::File>> {
    if !installed() {
        return Ok(None);
    }
    fs::create_dir_all(root()?)?;
    #[cfg(target_os = "macos")]
    write_bundle_metadata()?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root()?.join("instance.lock"))?;
    file.try_lock()
        .map_err(|_| "FindOut is already running. Open it from the tray.")?;
    remove(&executable()?.with_extension("old"))?;
    Ok(Some(file))
}
fn state() -> Result<State> {
    match fs::read(root()?.join("install.json")) {
        Ok(b) => Ok(serde_json::from_slice(&b)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
        Err(e) => Err(e.into()),
    }
}
fn save(s: &State) -> Result<()> {
    fs::create_dir_all(root()?)?;
    let p = root()?.join("install.json.tmp");
    fs::write(&p, serde_json::to_vec(s)?)?;
    #[cfg(windows)]
    remove(&root()?.join("install.json"))?;
    fs::rename(p, root()?.join("install.json"))?;
    Ok(())
}
pub fn remember_origin(origin: &str) -> Result<()> {
    let mut s = state()?;
    if !s.origins.iter().any(|o| o == origin) {
        s.origins.push(origin.into());
        save(&s)?;
    }
    Ok(())
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn whats_new() -> bool {
    state().is_ok_and(|s| {
        s.updated_version == CURRENT_VERSION && now() >= s.updated_at && now() - s.updated_at < 3600
    })
}
fn remove(p: &Path) -> Result<()> {
    match fs::remove_file(p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
#[cfg(target_os = "linux")]
fn desktop_path(startup: bool) -> Result<PathBuf> {
    Ok(if startup {
        base("XDG_CONFIG_HOME", ".config")?.join("autostart")
    } else {
        base("XDG_DATA_HOME", ".local/share")?.join("applications")
    }
    .join("findout.desktop"))
}
#[cfg(windows)]
fn desktop_path(startup: bool) -> Result<PathBuf> {
    Ok(
        PathBuf::from(std::env::var_os("APPDATA").ok_or("APPDATA unavailable")?)
            .join("Microsoft/Windows/Start Menu/Programs")
            .join(if startup {
                "Startup/FindOut.lnk"
            } else {
                "FindOut.lnk"
            }),
    )
}
#[cfg(target_os = "macos")]
fn mac_home() -> Result<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("Home directory unavailable")?);
    if !home.is_absolute() {
        return Err("Home directory must be absolute".into());
    }
    Ok(home)
}
#[cfg(target_os = "macos")]
fn app_bundle() -> Result<PathBuf> {
    Ok(mac_home()?.join("Applications/FindOut.app"))
}
#[cfg(target_os = "macos")]
fn desktop_path(startup: bool) -> Result<PathBuf> {
    if startup {
        Ok(mac_home()?.join("Library/LaunchAgents/app.findout.client.plist"))
    } else {
        app_bundle()
    }
}
#[cfg(any(target_os = "macos", test))]
fn xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
#[cfg(any(target_os = "macos", test))]
fn launch_agent(exe: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>app.findout.client</string>
<key>ProgramArguments</key><array><string>{}</string></array>
<key>RunAtLoad</key><true/>
<key>LimitLoadToSessionType</key><string>Aqua</string>
</dict></plist>
"#,
        xml_text(exe)
    )
}
#[cfg(target_os = "macos")]
fn write_bundle_metadata() -> Result<()> {
    let contents = app_bundle()?.join("Contents");
    fs::create_dir_all(contents.join("MacOS"))?;
    fs::write(
        contents.join("Info.plist"),
        include_str!("../packaging/macos/Info.plist").replace("@VERSION@", CURRENT_VERSION),
    )?;
    Ok(())
}
#[cfg(target_os = "macos")]
fn launcher(startup: bool, enabled: bool) -> Result<()> {
    let path = desktop_path(startup)?;
    if !enabled {
        return if startup {
            remove(&path)
        } else {
            match fs::remove_dir_all(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.into()),
            }
        };
    }
    if !startup {
        return write_bundle_metadata();
    }
    fs::create_dir_all(path.parent().ok_or("Invalid launcher path")?)?;
    fs::write(
        path,
        launch_agent(executable()?.to_str().ok_or("Invalid executable path")?),
    )?;
    Ok(())
}
#[cfg(not(target_os = "macos"))]
fn launcher(startup: bool, enabled: bool) -> Result<()> {
    let p = desktop_path(startup)?;
    if !enabled {
        return remove(&p);
    }
    fs::create_dir_all(p.parent().ok_or("Invalid launcher path")?)?;
    let exe = executable()?.to_string_lossy().into_owned();
    if exe.contains(['\n', '\r']) {
        return Err("Installation path contains a newline".into());
    }
    #[cfg(target_os = "linux")]
    let content = format!(
        "[Desktop Entry]\nType=Application\nName=FindOut\nExec=\"{}\"\nTerminal=false\n",
        exe.replace('\\', "\\\\\\\\")
            .replace('"', "\\\\\"")
            .replace('`', "\\\\`")
            .replace('$', "\\\\$")
            .replace('%', "%%")
    );
    #[cfg(target_os = "linux")]
    fs::write(p, content)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let status=Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference='Stop'; $s=(New-Object -ComObject WScript.Shell).CreateShortcut($env:FINDOUT_LINK); $s.TargetPath=$env:FINDOUT_EXE; $s.Save()"])
            .env("FINDOUT_LINK",p).env("FINDOUT_EXE",exe).creation_flags(0x08000000).status()?;
        if !status.success() {
            return Err("Could not create FindOut shortcut".into());
        }
    }
    Ok(())
}
pub fn autostart() -> bool {
    desktop_path(true).is_ok_and(|p| p.exists())
}
pub fn set_autostart(enabled: bool) -> Result<()> {
    let _operation = Operation::begin()?;
    if !installed() {
        return Err("Install FindOut first".into());
    }
    launcher(true, enabled)
}
pub fn install(startup: bool, origin: &str) -> Result<()> {
    let _operation = Operation::begin()?;
    fs::create_dir_all(root()?)?;
    let target = executable()?;
    fs::create_dir_all(target.parent().ok_or("Invalid executable path")?)?;
    if !installed() {
        if target.exists() && fs::read(&target)? != fs::read(std::env::current_exe()?)? {
            return Err(
                "FindOut is already installed. Open it from your applications menu.".into(),
            );
        }
        if !target.exists() {
            fs::copy(std::env::current_exe()?, &target)?;
        }
    }
    remember_origin(origin)?;
    launcher(false, true)?;
    launcher(true, startup)?;
    restart(&target)
}
fn restart(target: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        // exec retains the PID and releases the old process resources before startup.
        use std::os::unix::process::CommandExt;
        Err(Command::new(target).exec().into())
    }
    #[cfg(windows)]
    {
        helper(target, false, false)?;
        Ok(())
    }
}
#[cfg(windows)]
fn helper(target: &Path, update: bool, uninstall: bool) -> Result<()> {
    // Pass paths as environment data, never interpolate them into script source.
    use std::os::windows::process::CommandExt;
    let script = "$ErrorActionPreference='Stop'; Wait-Process -Id $env:FINDOUT_PARENT -ErrorAction SilentlyContinue; if ($env:FINDOUT_REMOVE -eq '1') { Remove-Item -LiteralPath $env:FINDOUT_ROOT -Recurse -Force } else { if ($env:FINDOUT_UPDATE -eq '1') { Move-Item -LiteralPath $env:FINDOUT_TARGET -Destination ($env:FINDOUT_TARGET + '.old') -Force; try { Move-Item -LiteralPath ($env:FINDOUT_TARGET + '.new') -Destination $env:FINDOUT_TARGET } catch { Move-Item -LiteralPath ($env:FINDOUT_TARGET + '.old') -Destination $env:FINDOUT_TARGET; throw }; Remove-Item -LiteralPath ($env:FINDOUT_TARGET + '.old') -Force }; Start-Process -FilePath $env:FINDOUT_TARGET }";
    Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .current_dir(std::env::temp_dir())
        .env("FINDOUT_PARENT", std::process::id().to_string())
        .env("FINDOUT_TARGET", target)
        .env("FINDOUT_ROOT", root()?)
        .env("FINDOUT_UPDATE", if update { "1" } else { "0" })
        .env("FINDOUT_REMOVE", if uninstall { "1" } else { "0" })
        .creation_flags(0x08000000)
        .spawn()?;
    Ok(())
}
pub fn uninstall(origin: &str) -> Result<()> {
    let _operation = Operation::begin()?;
    if !installed() {
        return Err("Open the installed copy of FindOut to uninstall it.".into());
    }
    let mut s = state()?;
    s.origins.push(origin.into());
    s.origins.sort();
    s.origins.dedup();
    for entry in s
        .origins
        .iter()
        .map(|o| token_entry(o))
        .chain(std::iter::once(trial_device_entry()))
    {
        match entry?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => (),
            Err(e) => return Err(e.into()),
        }
    }
    launcher(true, false)?;
    launcher(false, false)?;
    #[cfg(target_os = "linux")]
    {
        let metrics = base("XDG_STATE_HOME", ".local/state")?.join("findout/dev-metrics.csv");
        remove(&metrics)?;
        if let Some(parent) = metrics.parent() {
            if parent.exists() {
                fs::remove_dir(parent)?;
            }
        }
        fs::remove_dir_all(root()?)?;
    }
    #[cfg(target_os = "macos")]
    fs::remove_dir_all(root()?)?;
    #[cfg(windows)]
    helper(&executable()?, false, true)?;
    Ok(())
}
#[derive(Clone, Deserialize)]
pub struct Asset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
    size: u64,
}
fn update_asset_name(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Some("findout-client-linux-x86_64"),
        ("windows", "x86_64") => Some("findout-client-windows-x86_64.exe"),
        ("macos", "aarch64") => Some("findout-client-macos-aarch64"),
        ("macos", "x86_64") => Some("findout-client-macos-x86_64"),
        _ => None,
    }
}
pub fn update(release: &GitHubRelease) -> Result<()> {
    let _operation = Operation::begin()?;
    if !installed() {
        return Err("Install FindOut before updating.".into());
    }
    let name = update_asset_name(std::env::consts::OS, std::env::consts::ARCH)
        .ok_or("No update is available for this platform or architecture")?;
    let a = release
        .assets
        .iter()
        .find(|a| a.name == name)
        .ok_or("This release has no automatic update asset. Please download it from GitHub.")?;
    let (_, releases) = github_release_urls().ok_or("Missing release repository")?;
    if !a
        .browser_download_url
        .starts_with(&format!("{releases}/download/{}/", release.tag_name))
    {
        return Err("Invalid update URL".into());
    }
    let limit = 100 * 1024 * 1024;
    if a.size == 0 || a.size > limit {
        return Err("Invalid update size".into());
    }
    let response = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(180))
        .build()
        .get(&a.browser_download_url)
        .call()?;
    if !response.get_url().starts_with("https://") {
        return Err("Update requires HTTPS".into());
    }
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    verify(&bytes, a)?;
    let target = executable()?;
    let staged = target.with_file_name(format!(
        "{}.new",
        target.file_name().unwrap().to_string_lossy()
    ));
    let mut f = fs::File::create(&staged)?;
    f.write_all(&bytes)?;
    f.sync_all()?;
    drop(f);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))?;
    }
    let mut s = state()?;
    s.updated_version = release.tag_name.trim_start_matches('v').into();
    s.updated_at = now();
    save(&s)?;
    #[cfg(unix)]
    {
        let backup = target.with_extension("old");
        fs::hard_link(&target, &backup)?;
        if let Err(e) = fs::rename(staged, &target) {
            let _ = remove(&backup);
            return Err(e.into());
        }
        if let Err(e) = restart(&target) {
            fs::rename(&backup, &target)?;
            return Err(e);
        }
    }
    #[cfg(windows)]
    helper(&target, true, false)?;
    Ok(())
}
fn digest(bytes: &[u8]) -> String {
    format!(
        "sha256:{}",
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}
fn verify(bytes: &[u8], a: &Asset) -> Result<()> {
    let digest = digest(bytes);
    if bytes.len() as u64 != a.size || a.digest.as_deref() != Some(digest.as_str()) {
        return Err("Update verification failed. Try again.".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn update_assets_match_platform_and_architecture() {
        assert_eq!(
            update_asset_name("macos", "aarch64"),
            Some("findout-client-macos-aarch64")
        );
        assert_eq!(
            update_asset_name("macos", "x86_64"),
            Some("findout-client-macos-x86_64")
        );
        assert_eq!(
            update_asset_name("windows", "x86_64"),
            Some("findout-client-windows-x86_64.exe")
        );
        assert_eq!(
            update_asset_name("linux", "x86_64"),
            Some("findout-client-linux-x86_64")
        );
        assert_eq!(update_asset_name("linux", "aarch64"), None);
    }
    #[test]
    fn launch_agent_escapes_paths_as_data() {
        let plist =
            launch_agent("/Users/a & <b>/Applications/FindOut.app/Contents/MacOS/findout-client");
        assert!(plist.contains("/Users/a &amp; &lt;b&gt;/Applications/"));
        assert!(plist.contains("<key>RunAtLoad</key><true/>"));
        assert!(!plist.contains("KeepAlive"));
    }
    #[test]
    fn rejects_corrupt_or_missing_digest() {
        let mut a = Asset {
            name: String::new(),
            browser_download_url: String::new(),
            digest: Some(digest(b"binary")),
            size: 6,
        };
        assert!(verify(b"binary", &a).is_ok());
        assert!(verify(b"binarx", &a).is_err());
        assert!(verify(b"", &a).is_err());
        a.digest = None;
        assert!(verify(b"binary", &a).is_err());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn isolated_install_files() {
        if std::env::var_os("FINDOUT_LIFECYCLE_TEST_CHILD").is_none() {
            let temp =
                std::env::temp_dir().join(format!("findout-lifecycle-test-{}", std::process::id()));
            fs::create_dir_all(&temp).unwrap();
            let status = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "lifecycle::tests::isolated_install_files"])
                .env("FINDOUT_LIFECYCLE_TEST_CHILD", "1")
                .env("XDG_DATA_HOME", temp.join("data space"))
                .env("XDG_CONFIG_HOME", temp.join("config"))
                .status()
                .unwrap();
            fs::remove_dir_all(temp).unwrap();
            assert!(status.success());
            return;
        }
        remember_origin("https://example.test").unwrap();
        remember_origin("https://example.test").unwrap();
        assert_eq!(state().unwrap().origins.len(), 1);
        launcher(false, true).unwrap();
        launcher(true, true).unwrap();
        assert!(autostart());
        let desktop = fs::read_to_string(desktop_path(true).unwrap()).unwrap();
        assert!(desktop.contains("Exec=\""));
        assert!(desktop.contains("data space"));
        launcher(true, false).unwrap();
        assert!(!autostart());
        launcher(false, false).unwrap();
        let mut s = state().unwrap();
        s.updated_version = CURRENT_VERSION.into();
        s.updated_at = now();
        save(&s).unwrap();
        assert!(whats_new());
        s.updated_at = now() - 3601;
        save(&s).unwrap();
        assert!(!whats_new());
        s.updated_at = now() + 60;
        save(&s).unwrap();
        assert!(!whats_new());
    }
}
