# FindOut

**FindOut is a tiny desktop assistant for quickly asking questions without breaking your flow.**

Press a shortcut (Windows: alt+space Linux: super + space), type what you want to know, and get an answer. FindOut can search the web when needed, understand images from your clipboard, and use a small amount of information about your computer to give more useful system-specific answers.

It runs on **Windows and Linux** and is designed to stay lightweight, fast, and out of the way.

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

The current desktop client is a small native Slint application for Linux and
Windows. Provider keys and model/search logic stay on the FindOut server; the
client stores only a revocable installation token in the OS keychain.

Install a current stable Rust toolchain first. Linux additionally needs GTK 3,
a Secret Service-compatible keychain, and an X11-capable environment; on Arch:

```sh
sudo pacman -S gtk3 libsecret
```

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
`%LOCALAPPDATA%\\FindOut\\dev-metrics.csv`. Columns are timestamp, round-trip
milliseconds, outcome, and search/force-search/image flags. The file never
contains questions, answers, images, activation keys, device identifiers, or
tokens. Normal builds neither show nor write metrics.

## Updates and releases

Published builds check GitHub's public latest-release endpoint once at startup.
When it finds a newer stable `vMAJOR.MINOR.PATCH` release, an `UPDATE vX.Y.Z`
link appears in the bottom-right footer; clicking it opens the repository's
Releases page. A failed or unavailable check stays silent.

The release workflow builds Linux x86_64 and Windows x86_64 archives when a
`v*` tag is pushed, then publishes both files to a GitHub Release. Set the
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
  and Alt+Space on Windows. It opens around the pointer, stays inside the
  monitor work area, and has no taskbar entry. Windows explicitly disables the
  borderless-window shadow; X11 uses a utility window hint, while any remaining
  compositor shadow is controlled by the desktop.
- A native tray icon keeps the hidden client reachable. Its menu selects the
  Light, Dark, or Retro palette; Retro is the default for now.
- Activation UI with issued-key support, free-trial enrollment, and OS keychain
  storage. Tokens are isolated by configured backend origin.
- Text queries with cursor-following horizontal scrolling, optional forced web
  search, and selectable, scrollable answers with copying.
- Clipboard image paste. PNG/JPEG inputs are checked before decode, rejected
  above 3 MB or the source safety envelope, downscaled to a 4096-pixel maximum edge,
  padded to a 1920×1080 canvas when small, and sent as PNG.
- A minimal one-line device context so system-specific “how do I…” answers fit
  the user's machine.

Escape, the global hotkey, clicking away, or losing focus hides the popup. A
hidden popup keeps its current view for 10 seconds so an accidental dismissal
can be recovered; after that the question, answer, status, and pending image
are cleared. Ctrl-C stops the development process.

## Platform notes

- Linux clipboard image acquisition uses GTK3; the global hotkey requires an
  X11-capable environment, the popup uses X11 utility/override-redirect hints,
  and the tray uses StatusNotifierItem/AppIndicator over D-Bus. Desktops need
  a compatible tray host; GNOME may require its AppIndicator extension. The
  keychain uses Secret Service.
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
 "image":{"mime_type":"image/png","data":"<base64, optional>"},
 "system_context":"CachyOS | pkg:pacman | shell:/usr/bin/zsh"}
```

The backend remains authoritative for request body, query, image, token,
protocol, activation, tool-loop, concurrency, and provider-cost limits.
