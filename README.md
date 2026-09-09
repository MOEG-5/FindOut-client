# FindOut

**FindOut is a tiny desktop assistant for quickly asking questions without breaking your flow.**

Press a shortcut (Windows: Alt+Space, Linux: Super+Space, macOS: Option+Space), type what you want to know, and get an answer. FindOut can search the web when needed, understand images from your clipboard, and use a small amount of information about your computer to give more useful system-specific answers.

It runs on **Windows, Linux, and macOS** and is designed to stay lightweight, fast, and out of the way.

## Try it for free

Download the latest version from **Releases** and launch FindOut.

When it asks for an activation key, simply enter:

**trial**

You'll receive a free trial with **20 requests per day**. Your allowance resets automatically each day.

No API keys or separate accounts are needed.

## What can it do?

* Answer quick questions from anywhere on your desktop
* Search the web for current information
* Help with questions about your computer
* Understand screenshots and other images copied to your clipboard
* Give useful answers without opening a browser and digging through search results
* Stay hidden until you call it with a keyboard shortcut

FindOut lives in your system tray and can be opened whenever you need it.

<img src="docs/findout-screenshot.png" alt="FindOut in its retro theme, displaying an example answer and a follow-up question." width="560">

## For developers

FindOut is a small native Slint application. Provider keys, model selection, search, trial limits, and other service logic live on the FindOut server. The client stores only a revocable installation token in the operating system keychain.

Licensed under [GPL-3.0-only](LICENSE).

The current desktop client is a small native Slint application for Linux, Windows, and macOS. Provider keys and model/search logic stay on the FindOut server; the
client stores only a revocable installation token in the OS keychain.

Install a current stable Rust toolchain first. Linux additionally needs GTK 3,
a Secret Service-compatible keychain, and an X11-capable environment; on Arch:

```sh
sudo pacman -S gtk3 libsecret
```

macOS developers also need Xcode Command Line Tools (`xcode-select --install`).

Run it from the repository root:

```sh
cargo run
```

Development defaults to `http://127.0.0.1:8787`. Release builds must use an
HTTPS API origin:

```sh
# Linux/macOS
FINDOUT_API_ORIGIN=https://api.example.com cargo build --release

# Windows PowerShell
$env:FINDOUT_API_ORIGIN="https://api.example.com"; cargo build --release
```

For opt-in local latency logging, build the separate development variant:

```sh
FINDOUT_API_ORIGIN=https://findout-backend-staging.vercel.app \
  cargo build --release --features dev-metrics
```

It shows the last end-to-end round-trip time in the answer footer and appends
one CSV row per query to `$XDG_STATE_HOME/findout/dev-metrics.csv` (normally
`~/.local/state/findout/dev-metrics.csv`). Windows uses
`%LOCALAPPDATA%\\FindOut\\dev-metrics.csv`; macOS uses
`~/Library/Application Support/FindOut/dev-metrics.csv`. Columns are timestamp, round-trip
milliseconds, outcome, and search/force-search/image flags. The file never
contains questions, answers, images, activation keys, device identifiers, or
tokens. Normal builds neither show nor write metrics.

## Updates and releases

Published builds offer a per-user installation on first launch. “Start FindOut
when I sign in” is selected by default and can be unchecked before installing,
or changed later in the tray menu. “Not now” keeps the downloaded copy portable;
the tray menu lets you install later. Development builds do not prompt.

Installed builds check GitHub's public latest-release endpoint once at startup.
Clicking `UPDATE vX.Y.Z` downloads the matching executable, checks its size and
SHA-256 digest against GitHub's release metadata, replaces the installed binary,
and restarts FindOut. Failed updates show an error and can be retried. Releases
without a matching executable and digest require a manual download. After an
update, `WHAT’S NEW` links to that version's GitHub release notes for one hour;
restarting the client does not reset that hour.

The tray menu includes **Uninstall FindOut…**, followed by a Yes/No confirmation.
It removes the installed application, startup and application-menu shortcuts,
local installation state, development metrics, and keychain credentials for
backend origins recorded by this installation, plus the trial fallback identity.
A locked keychain stops cleanup with an error so it can be retried. Uninstall
applies to the current account; downloaded archives, other portable copies,
server-side records, and credentials from unrecorded legacy backend origins are
not removed.

Linux installs in `$XDG_DATA_HOME/findout` (normally `~/.local/share/findout`),
with desktop entries in the XDG applications and autostart directories. Windows
installs in `%LOCALAPPDATA%\FindOut`, with Start Menu and optional Startup
shortcuts. macOS installs in `~/Applications/FindOut.app`, stores local state in
`~/Library/Application Support/FindOut`, and uses
`~/Library/LaunchAgents/app.findout.client.plist` for optional startup at the next
login. None of these installations requires administrator access. Windows uses a
separate PowerShell process to replace or remove the executable after exit.

The release workflow builds Linux x86_64, Windows x86_64, macOS Apple Silicon
(aarch64), and macOS Intel (x86_64) archives when a `v*` tag is pushed, then
publishes the archives and standalone updater executables to a GitHub Release.
Each Mac build runs its tests on a matching native macOS runner and briefly
launches the packaged app in that disposable session to check startup. Manual workflow
runs build downloadable artifacts without publishing a release. Set the
repository Actions variable `FINDOUT_API_ORIGIN` to the production HTTPS API
origin before tagging. The workflow injects the repository name into the
binary, so local builds only enable update checking when built with both
`FINDOUT_API_ORIGIN` and `FINDOUT_GITHUB_REPOSITORY=OWNER/REPO`.

```sh
FINDOUT_API_ORIGIN=https://api.example.com \
FINDOUT_GITHUB_REPOSITORY=OWNER/REPO \
  cargo build --release
```

Free-trial enrollment is initiated by entering `trial` in the client. Issued
paid or private activation keys remain a backend/admin operation: do not generate
or publish them from the client release workflow. Until a private delivery
channel or commerce provider exists, issue those keys from the backend admin
tooling and send each key privately.

## Client responsibilities

- Frameless, translucent popup with the platform hotkey: Super+Space on Linux
  Alt+Space on Windows, and Option+Space on macOS. It opens around the pointer, stays inside the
  monitor work area, and has no taskbar entry. Windows explicitly disables the
  borderless-window shadow; X11 uses a utility window hint, while any remaining
  compositor shadow is controlled by the desktop.
- A native tray icon keeps the hidden client reachable. Its menu selects the
  Light, Dark, or Retro palette; Light is the default.
- Activation UI with issued-key support, free-trial enrollment, and OS keychain
  storage. Tokens are isolated by configured backend origin.
- Text queries with cursor-following horizontal scrolling, optional forced web
  search, and selectable, scrollable answers with copying.
- Follow-up questions send the last four successful question/answer pairs, oldest
  first, capped at 2,000 characters per question or answer. History stays in
  memory; the server remains stateless. Search Web retries the original request
  with its context and image, replacing that turn's answer in history. Earlier
  images are not included in subsequent follow-up questions.
- Click the bottom-left feedback address to copy it; confirmation lasts two seconds.
- Clipboard image paste. PNG/JPEG inputs are checked before decode, rejected
  above 3 MB or the source safety envelope, downscaled to a 4096-pixel maximum edge,
  padded to a 1920×1080 canvas when small, and sent as PNG.
- A minimal one-line device context so system-specific “how do I…” answers fit
  the user's machine.

Escape, the global hotkey, clicking away, or losing focus hides the popup. A
hidden popup keeps its current view for 10 seconds so an accidental dismissal
can be recovered; after that the question, answer, status, pending image,
and conversation history are cleared. Ctrl-C stops the development process.

## macOS downloads

Choose `findout-client-macos-aarch64.zip` for Apple Silicon or
`findout-client-macos-x86_64.zip` for Intel. These ZIPs contain the actual
`FindOut.app` bundle. The similarly named files without `.zip` are internal
updater executables, not the app download; renaming one to `.app` will not work.
Extract and open `FindOut.app`, then
use the installation dialog to install for your account. You can also move the
app to `~/Applications` yourself. The menu-bar icon opens the popup.

These builds have ad-hoc executable signatures, but are not Developer ID signed
or notarized. macOS may block the first launch; use the system's Privacy &
Security “Open Anyway” flow only if you trust the downloaded release. Public
notarization requires an Apple Developer signing identity and credentials.

To create a bundle locally on a Mac after building the release executable:

```sh
python3 scripts/package-macos.py --arch aarch64  # x86_64 on an Intel Mac
```

## Platform notes

- Linux clipboard image acquisition uses GTK3; the global hotkey requires an
  X11-capable environment, the popup uses X11 utility/override-redirect hints,
  and the tray uses StatusNotifierItem/AppIndicator over D-Bus. Desktops need
  a compatible tray host; GNOME may require its AppIndicator extension. The
  keychain uses Secret Service.
- macOS 12 or newer uses Keychain, the native clipboard, and a menu-bar icon.
  Option+Space opens the popup; Cmd+V pastes images. Popup positioning accounts
  for Retina scaling and the menu bar/Dock work area. Trial identity uses the
  existing persistent random Keychain fallback. The app has no Dock icon.
- Windows uses Credential Manager through the keyring crate and Alt+Space,
  because Win+Space is reserved for keyboard-layout switching.

## UI regression checks

On Linux, run the scrolling check in a disposable display (requires Xvfb):

```sh
dbus-run-session -- xvfb-run -a env -u WAYLAND_DISPLAY SLINT_SCALE_FACTOR=1 \
  cargo test scrolling_ui -- --ignored
```

The scrolling check uses synthetic questions and answers; it does not access the
keychain or backend. To check tray registration when the desktop panel starts late
(requires Xvfb and Python GObject bindings):

```sh
/usr/bin/python3 tests/tray-host.py target/release/findout-client-dev
```

This script creates its own disposable display and D-Bus session, with keychain
service activation disabled. Pass the regular client binary to check that variant.
Linux waits for a ready tray host for up to 30 attempts during startup, without
blocking the popup or hotkey.

## Server contract

The client expects an HTTPS server implementing this contract.

```http
POST /v1/activate
Content-Type: application/json
X-FindOut-Protocol: 1

{"activation_key":"..."}
```

Entering `trial` instead sends an app-specific, stable device identifier derived
locally from the platform machine identity (or a persisted random fallback):

```json
{"activation_key":"trial","device_id":"<64 lowercase hex characters>"}
```

Raw machine identifiers never leave the device. Tokens are scoped to the
configured backend origin. Trial query responses may include server-controlled
`X-FindOut-Daily-Limit`, `X-FindOut-Daily-Remaining`, and
`X-FindOut-Daily-Reset` headers, which the client displays without enforcing a
local counter.

```http
POST /v1/query
Authorization: Bearer <device_token>
Content-Type: application/json
X-FindOut-Protocol: 1

{"query":"...",
 "force_search":false,
 "previous_turns":[{"query":"earlier question","answer":"earlier answer"}],
 "image":{"mime_type":"image/png","data":"<base64, optional>"},
 "system_context":"CachyOS | pkg:pacman | shell:/usr/bin/zsh"}
```

The backend remains authoritative for request body, query, image, token,
protocol, activation, tool-loop, concurrency, and provider-cost limits.
