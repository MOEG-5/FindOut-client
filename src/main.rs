#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod lifecycle;

use base64::Engine as _;
use chrono::DateTime;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use hmac::{Hmac, KeyInit, Mac};
use image::{imageops, DynamicImage, ImageFormat, ImageReader, Rgba, RgbaImage};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
use slint::{CloseRequestResponse, ComponentHandle, Timer, Weak};
#[cfg(feature = "dev-metrics")]
use std::io::Write;
use std::io::{Cursor, Read};
#[cfg(feature = "dev-metrics")]
use std::path::PathBuf;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const PROTOCOL_VERSION: &str = "1";
const KEYRING_SERVICE: &str = "app.findout.client";
const TOKEN_KEYRING_USER: &str = "installation";
const TRIAL_DEVICE_KEYRING_USER: &str = "trial-device-v1";
const TRIAL_DEVICE_MESSAGE: &[u8] = b"findout/trial/device/v1";
const MAX_QUERY_CHARS: usize = 4_000;
const MAX_PREVIOUS_TURNS: usize = 4;
const MAX_TURN_CHARS: usize = 2_000;
const FEEDBACK_EMAIL: &str = "moeg-5@agentmail.to";
// Fits one base64-encoded image plus JSON below Vercel's 4.5 MB request limit.
const MAX_IMAGE_BYTES: usize = 3_000_000;
const MAX_IMAGE_BASE64_CHARS: usize = MAX_IMAGE_BYTES.div_ceil(3) * 4;
const MAX_SOURCE_IMAGE_DIMENSION: u32 = 16_384;
const MAX_SOURCE_IMAGE_PIXELS: u64 = 64_000_000;
const MAX_IMAGE_DIMENSION: u32 = 4_096;
const CANVAS_WIDTH: u32 = 1_920;
const CANVAS_HEIGHT: u32 = 1_080;
const CANVAS_BG: [u8; 4] = [14, 17, 24, 255];
const MAX_RESPONSE_BYTES: u64 = 128 * 1024;
const POPUP_WIDTH: i32 = 560;
const POPUP_HEIGHT: i32 = 320;
const FOCUS_LOSS_DEBOUNCE: Duration = Duration::from_millis(100);
const HIDE_GRACE: Duration = Duration::from_secs(10);
const UPDATE_CHECK_TIMEOUT: Duration = Duration::from_secs(5);
const THEME_LIGHT: i32 = 0;
const THEME_DARK: i32 = 1;
const THEME_RETRO: i32 = 2;

slint::slint! {
    import { Palette, ScrollView, Button, CheckBox } from "std-widgets.slint";

    export component FindOutWindow inherits Window {
        title: "FindOut";
        always-on-top: true;
        no-frame: true;
        width: 560px;
        height: 320px;
        // ponytail: translucent tint, not backdrop blur; add a platform compositor hook if real blur is required.
        background: transparent;

        in property <bool> activated: false;
        in property <bool> busy: false;
        in property <bool> feedback-copied: false;
        in property <bool> has-image: false;
        in property <bool> can-force-search: false;
        in property <string> answer: "";
        in property <string> status: "";
        in property <string> update_available: "";
        in property <bool> dev_metrics: false;
        in property <string> shortcut-label: "SUPER + SPACE";
        in property <string> paste-label: "CTRL+V";
        in property <string> roundtrip: "";
        in-out property <string> question: "";
        in-out property <string> activation_key: "";
        in property <int> theme: 0;
        init => { Palette.color-scheme = root.theme == 0 ? ColorScheme.light : ColorScheme.dark; }
        changed theme => { Palette.color-scheme = root.theme == 0 ? ColorScheme.light : ColorScheme.dark; }

        private property <color> panel_bg: root.theme == 0 ? #fffaf2ee : root.theme == 1 ? #10151de8 : #160e0be6;
        private property <color> field_bg: root.theme == 0 ? #fffdf8e6 : root.theme == 1 ? #171d27e6 : #24140dd9;
        private property <color> answer_bg: root.theme == 0 ? #fffaf0cc : root.theme == 1 ? #131a24cc : #21130db8;
        private property <color> primary_text: root.theme == 0 ? #8b3f1f : root.theme == 1 ? #e28a4e : #d27839;
        private property <color> accent: root.theme == 0 ? #a94f24 : root.theme == 1 ? #e07839 : #c6632f;
        private property <color> bright_accent: root.theme == 0 ? #b95b2a : root.theme == 1 ? #f0a065 : #f0a065;
        private property <color> muted_text: root.theme == 0 ? #8c5a3f : root.theme == 1 ? #9c735d : #9a5b36;
        private property <color> faint_text: root.theme == 0 ? #a57d63 : root.theme == 1 ? #735b4a : #70432a;
        private property <color> border: root.theme == 0 ? #b96b3da6 : root.theme == 1 ? #9b5b3c99 : #753c1fa6;
        private property <color> field_border: root.theme == 0 ? #b96b3d99 : root.theme == 1 ? #744b3d99 : #6d3b2299;
        private property <color> selection_bg: root.theme == 0 ? #e29b6aa6 : root.theme == 1 ? #c66a3da6 : #a9552fa6;
        private property <color> selection_fg: root.theme == 0 ? #3a1b0e : root.theme == 1 ? #24150d : #21120b;

        callback setup-tray();
        callback activate(string);
        callback submit(string, bool);
        callback paste-image();
        callback clear-image();
        callback copy-answer();
        callback copy-feedback();
        callback open-releases();
        callback escape();

        panel := Rectangle {
            x: 4px;
            y: 4px;
            width: parent.width - 8px;
            height: parent.height - 8px;
            background: root.panel_bg;
            border-radius: 16px;
            border-width: 1px;
            border-color: root.border;
            clip: true;

            VerticalLayout {
                padding: 16px;
                spacing: 9px;

                HorizontalLayout {
                    height: 18px;
                    Text {
                        text: "FINDOUT";
                        color: root.accent;
                        font-size: 13px;
                        font-weight: 600;
                        horizontal-stretch: 1;
                    }
                    Text {
                        text: !root.activated ? "ACTIVATE ONCE" :
                            root.dev_metrics ? "DEV · " + root.shortcut-label : root.shortcut-label;
                        color: root.muted_text;
                        font-size: 10px;
                    }
                }

                Rectangle {
                    width: 26px;
                    height: 2px;
                    background: root.accent;
                    border-radius: 1px;
                }

                if !root.activated: VerticalLayout {
                    spacing: 9px;

                    Text {
                        text: "Enter trial for 20 free requests/day, or an activation key. A private app-specific device ID prevents duplicate trials; raw machine IDs stay here.";
                        color: root.muted_text;
                        font-size: 12px;
                        wrap: word-wrap;
                    }

                    activation_shell := Rectangle {
                        height: 44px;
                        background: root.field_bg;
                        border-radius: 10px;
                        border-width: 1px;
                        border-color: activation_input.has-focus ? root.accent : root.field_border;
                        clip: true;

                        activation_input := TextInput {
                            x: 13px;
                            width: parent.width - 26px;
                            height: parent.height;
                            text <=> root.activation_key;
                            color: root.primary_text;
                            selection-background-color: root.selection_bg;
                            selection-foreground-color: root.selection_fg;
                            font-size: 15px;
                            input-type: password;
                            vertical-alignment: center;
                            enabled: !root.busy;
                            accepted => { root.activate(self.text); }
                            key-pressed(event) => {
                                if (event.text == Key.Escape) { root.escape(); accept }
                                reject
                            }
                        }
                    }
                }

                if root.activated: VerticalLayout {
                    spacing: 9px;

                    input_shell := Rectangle {
                        height: 44px;
                        background: root.field_bg;
                        border-radius: 10px;
                        border-width: 1px;
                        border-color: input.has-focus ? root.accent : root.field_border;
                        clip: true;

                        Rectangle {
                            x: 13px;
                            width: paste_button.x - self.x - 8px;
                            height: parent.height;
                            clip: true;

                            input := TextInput {
                                private property <length> scroll-x;
                                x: min(0px, max(parent.width - self.width, self.scroll-x));
                                width: max(parent.width, self.preferred-width + self.text-cursor-width);
                                single-line: true;
                                cursor-position-changed(pos) => {
                                    self.scroll-x = max(-pos.x + 4px,
                                        min(self.scroll-x, parent.width - pos.x - self.text-cursor-width - 4px));
                                }
                                height: parent.height;
                                text <=> root.question;
                                color: root.primary_text;
                                selection-background-color: root.selection_bg;
                                selection-foreground-color: root.selection_fg;
                                font-size: 16px;
                                vertical-alignment: center;
                                enabled: !root.busy;
                                accepted => { root.submit(self.text, false); }
                                key-pressed(event) => {
                                    if (event.text == Key.Escape) { root.escape(); accept }
                                    if ((event.modifiers.control || event.modifiers.meta) &&
                                        (event.text == "v" || event.text == "V")) {
                                        root.paste-image();
                                    }
                                    reject
                                }
                            }
                        }

                        paste_button := Rectangle {
                            x: parent.width - 104px;
                            y: 4px;
                            width: 42px;
                            height: parent.height - 8px;
                            background: paste_area.pressed ? root.accent : root.field_border;
                            border-radius: 7px;
                            Text {
                                text: root.has-image ? "IMG" : "＋";
                                color: root.accent;
                                font-size: root.has-image ? 10px : 18px;
                                horizontal-alignment: center;
                                vertical-alignment: center;
                            }
                            paste_area := TouchArea {
                                enabled: !root.busy;
                                clicked => { root.paste-image(); }
                            }
                        }

                        send_button := Rectangle {
                            x: parent.width - 56px;
                            y: 4px;
                            width: 42px;
                            height: parent.height - 8px;
                            background: send_area.pressed ? root.accent : root.accent;
                            border-radius: 7px;
                            Text {
                                text: root.busy ? "…" : "→";
                                color: root.bright_accent;
                                font-size: 18px;
                                horizontal-alignment: center;
                                vertical-alignment: center;
                            }
                            send_area := TouchArea {
                                enabled: !root.busy;
                                clicked => { root.submit(input.text, false); }
                            }
                        }
                    }

                    if root.has-image: Rectangle {
                        height: 18px;
                        Text {
                            text: "IMAGE ATTACHED";
                            color: root.accent;
                            font-size: 10px;
                            horizontal-stretch: 1;
                        }
                        TouchArea {
                            enabled: !root.busy;
                            clicked => { root.clear-image(); }
                        }
                    }

                    answer_shell := Rectangle {
                        vertical-stretch: 1;
                        background: root.answer_bg;
                        border-radius: 10px;
                        border-width: 1px;
                        border-color: root.field_border;
                        clip: true;

                        answer_scroll := ScrollView {
                            x: 12px;
                            y: 9px;
                            width: parent.width - 24px;
                            height: parent.height - 18px;
                            viewport-width: self.visible-width;
                            viewport-height: answer_text.height;
                            horizontal-scrollbar-policy: always-off;
                            mouse-drag-pan-enabled: false;

                            answer_text := TextInput {
                                width: answer_scroll.visible-width - 14px;
                                height: max(answer_scroll.visible-height, self.preferred-height);
                                page-height: answer_scroll.visible-height;
                                changed text => { answer_scroll.viewport-y = 0px; }
                                cursor-position-changed(pos) => {
                                    answer_scroll.viewport-y = min(0px,
                                        max(answer_scroll.visible-height - self.height,
                                            max(-pos.y, min(answer_scroll.viewport-y,
                                                answer_scroll.visible-height - pos.y - 20px))));
                                }
                                text: root.answer.is-empty ? "Ask a question to begin" : root.answer;
                                color: root.answer.is-empty ? root.faint_text : root.primary_text;
                                font-size: 15px;
                                read-only: true;
                                single-line: false;
                                wrap: word-wrap;
                                vertical-alignment: top;
                                selection-background-color: root.selection_bg;
                                selection-foreground-color: root.selection_fg;
                                key-pressed(event) => {
                                    if (event.text == Key.Escape) { root.escape(); accept }
                                    reject
                                }
                            }
                        }
                    }

                    HorizontalLayout {
                        height: 18px;
                        Text {
                            text: root.paste-label + (root.has-image ? " TO REPLACE IMAGE" : " TO ATTACH IMAGE");
                            color: root.faint_text;
                            font-size: 10px;
                            horizontal-stretch: 1;
                        }
                        if root.dev_metrics && !root.roundtrip.is-empty: Text {
                            width: 70px;
                            text: root.roundtrip;
                            color: root.muted_text;
                            font-size: 10px;
                            horizontal-alignment: right;
                        }
                        if root.can-force-search: Rectangle {
                            width: 82px;
                            Text {
                                text: "SEARCH WEB";
                                color: root.accent;
                                font-size: 10px;
                                horizontal-alignment: center;
                                vertical-alignment: center;
                            }
                            TouchArea {
                                clicked => { root.submit("", true); }
                            }
                        }
                        if !root.answer.is-empty: Rectangle {
                            width: 36px;
                            Text {
                                text: "COPY";
                                color: root.muted_text;
                                font-size: 10px;
                                horizontal-alignment: center;
                                vertical-alignment: center;
                            }
                            TouchArea {
                                clicked => { root.copy-answer(); }
                            }
                        }
                    }
                }

                HorizontalLayout {
                    height: 18px;
                    Rectangle {
                        width: 193px;
                        Text {
                            width: parent.width;
                            height: parent.height;
                            horizontal-alignment: left;
                            text: root.feedback-copied ? "copied to clipboard" : "feedback: moeg-5@agentmail.to";
                            color: feedback-area.has-hover ? root.accent : root.muted_text;
                            font-size: 10px;
                            vertical-alignment: center;
                        }
                        feedback-area := TouchArea {
                            mouse-cursor: pointer;
                            clicked => { root.copy-feedback(); }
                        }
                    }
                    Text {
                        text: root.status;
                        color: root.muted_text;
                        font-size: 10px;
                        horizontal-stretch: 1;
                        overflow: elide;
                    }
                    if !root.update_available.is-empty: Rectangle {
                        width: 112px;
                        Text {
                            text: root.update_available;
                            color: root.accent;
                            font-size: 10px;
                            horizontal-alignment: right;
                            vertical-alignment: center;
                        }
                        TouchArea {
                            clicked => { root.open-releases(); }
                        }
                    }
                }
            }
        }

    }

    export component InstallDialog inherits Window {
        title: "Install FindOut";
        width: 420px;
        height: 230px;
        in-out property <bool> autostart: true;
        in property <string> error: "";
        callback install(bool);
        callback cancel();
        VerticalLayout {
            padding: 24px;
            spacing: 16px;
            Text { text: "Keep FindOut a shortcut away"; font-size: 20px; }
            Text { text: "Install for your account to enable automatic updates."; wrap: word-wrap; }
            CheckBox { text: "Start FindOut when I sign in"; checked <=> root.autostart; }
            Text { text: root.error; wrap: word-wrap; }
            HorizontalLayout {
                Button { text: "Not now"; clicked => { root.cancel(); } }
                Button { text: "Install"; clicked => { root.install(root.autostart); } }
            }
        }
    }
    export component UninstallDialog inherits Window {
        title: "Uninstall FindOut?";
        width: 420px;
        height: 240px;
        in property <string> error: "";
        callback confirm();
        callback cancel();
        VerticalLayout {
            padding: 24px;
            spacing: 16px;
            Text { text: "Are you sure?"; font-size: 20px; }
            Text { text: "This removes FindOut, its sign-in credentials, startup entry and local app data from this account."; wrap: word-wrap; }
            Text { text: root.error; wrap: word-wrap; }
            HorizontalLayout {
                Button { text: "No, keep FindOut"; clicked => { root.cancel(); } }
                Button { text: "Yes, uninstall"; clicked => { root.confirm(); } }
            }
        }
    }

    export component FindOutTray inherits SystemTrayIcon {
        icon: @image-url("../assets/findout-tray.svg");
        tooltip: "FindOut";
        title: "FindOut";
        in property <int> theme: 0;

        callback show-window();
        callback select-theme(int);
        callback quit();
        callback install();
        callback uninstall();
        callback toggle-autostart();
        in property <bool> installed: false;
        in property <bool> autostart: false;

        Menu {
            MenuItem {
                title: "Show FindOut";
                activated => { root.show-window(); }
            }
            MenuSeparator { }
            MenuItem {
                title: "Light";
                checkable: true;
                checked: root.theme == 0;
                activated => { root.select-theme(0); }
            }
            MenuItem {
                title: "Dark";
                checkable: true;
                checked: root.theme == 1;
                activated => { root.select-theme(1); }
            }
            MenuItem {
                title: "Retro";
                checkable: true;
                checked: root.theme == 2;
                activated => { root.select-theme(2); }
            }
            MenuSeparator { }
            MenuItem {
                title: root.installed ? "Start when I sign in" : "Install FindOut…";
                checkable: root.installed;
                checked: root.autostart;
                activated => { if root.installed { root.toggle-autostart(); } else { root.install(); } }
            }
            MenuItem {
                title: "Uninstall FindOut…";
                enabled: root.installed;
                activated => { root.uninstall(); }
            }
            MenuSeparator { }
            MenuItem {
                title: "Quit";
                activated => { root.quit(); }
            }
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct ImagePayload {
    mime_type: String,
    data: String,
}

#[derive(Serialize)]
struct ActivationRequest<'a> {
    activation_key: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_id: Option<&'a str>,
}

#[derive(Deserialize)]
struct ActivationResponse {
    device_token: String,
}

#[derive(Clone, Serialize)]
struct AskRequest {
    query: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    previous_turns: Vec<ConversationTurn>,
    force_search: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<ImagePayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_context: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct ConversationTurn {
    query: String,
    answer: String,
}

#[derive(Default)]
struct Conversation {
    turns: Vec<ConversationTurn>,
    last_request: Option<AskRequest>,
}

impl Conversation {
    fn prepare(
        &mut self,
        text: &str,
        force_search: bool,
        image: Option<ImagePayload>,
    ) -> Result<AskRequest, String> {
        let request = if force_search {
            let mut request = self.last_request.clone().ok_or("Enter a question")?;
            request.force_search = true;
            request
        } else {
            AskRequest {
                query: validate_query(text)?.to_owned(),
                previous_turns: self.turns.clone(),
                force_search: false,
                image,
                system_context: None,
            }
        };
        self.last_request = Some(request.clone());
        Ok(request)
    }

    fn complete(&mut self, request: &AskRequest, answer: &str) {
        // Rebuild from the original context so SEARCH WEB replaces its answer.
        self.turns = request.previous_turns.clone();
        self.turns.push(ConversationTurn {
            query: request.query.chars().take(MAX_TURN_CHARS).collect(),
            answer: answer.chars().take(MAX_TURN_CHARS).collect(),
        });
        let excess = self.turns.len().saturating_sub(MAX_PREVIOUS_TURNS);
        self.turns.drain(..excess);
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

#[derive(Deserialize)]
struct AskResponse {
    answer: String,
    searched: bool,
}

#[derive(Clone, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    #[serde(default)]
    assets: Vec<lifecycle::Asset>,
}

#[derive(Deserialize)]
struct ErrorBody {
    message: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct QuotaMetadata {
    limit: u64,
    remaining: u64,
    reset: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ResponseMetadata {
    quota: Option<QuotaMetadata>,
    retry_after: Option<u64>,
}

struct HttpResponse<T> {
    body: T,
    metadata: ResponseMetadata,
}

enum RequestError {
    Http(u16, String, ResponseMetadata),
    Transport,
    InvalidResponse,
}

fn api_origin() -> Result<String, String> {
    let value = std::env::var("FINDOUT_API_ORIGIN")
        .ok()
        .or_else(|| option_env!("FINDOUT_API_ORIGIN").map(str::to_owned))
        .or_else(|| cfg!(debug_assertions).then(|| "http://127.0.0.1:8787".to_owned()))
        .ok_or_else(|| "FINDOUT_API_ORIGIN must be set when building a release".to_owned())?;
    let parsed = url::Url::parse(&value).map_err(|_| "FINDOUT_API_ORIGIN is not a valid URL")?;
    let host = parsed
        .host_str()
        .ok_or_else(|| "FINDOUT_API_ORIGIN must include a host".to_owned())?;
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err("FINDOUT_API_ORIGIN cannot contain a query or fragment".to_owned());
    }
    let loopback =
        matches!(host, "localhost" | "127.0.0.1" | "::1") || host.ends_with(".localhost");
    if !cfg!(debug_assertions) && parsed.scheme() != "https" && !loopback {
        return Err("FINDOUT_API_ORIGIN must use HTTPS in release builds".to_owned());
    }
    Ok(value.trim_end_matches('/').to_owned())
}

fn credential_entry(origin: &str, user: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(&format!("{KEYRING_SERVICE}:{origin}"), user)
        .map_err(|_| "System keychain is unavailable".to_owned())
}

fn token_entry(origin: &str) -> Result<keyring::Entry, String> {
    credential_entry(origin, TOKEN_KEYRING_USER)
}

fn trial_device_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, TRIAL_DEVICE_KEYRING_USER)
        .map_err(|_| "System keychain is unavailable".to_owned())
}

fn token(origin: &str) -> Result<Option<String>, String> {
    match token_entry(origin)?.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err("System keychain is locked or unavailable".to_owned()),
    }
}

fn validate_activation_key(value: &str) -> Result<&str, String> {
    let value = value.trim();
    if !value.eq_ignore_ascii_case("trial") && !(8..=256).contains(&value.len()) {
        return Err("Enter trial or an 8–256 character activation key".to_owned());
    }
    Ok(value)
}

fn derive_trial_device_id(machine_identity: &[u8]) -> Result<String, String> {
    let mut hmac = Hmac::<Sha256>::new_from_slice(machine_identity)
        .map_err(|_| "Could not derive trial device identity".to_owned())?;
    hmac.update(TRIAL_DEVICE_MESSAGE);
    Ok(hmac
        .finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(target_os = "linux")]
fn platform_machine_identity() -> Option<Vec<u8>> {
    std::fs::read_to_string("/etc/machine-id")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .map(String::into_bytes)
}

#[cfg(target_os = "windows")]
fn platform_machine_identity() -> Option<Vec<u8>> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

    let wide = |value: &str| value.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let subkey = wide("SOFTWARE\\Microsoft\\Cryptography");
    let name = wide("MachineGuid");
    let mut bytes = 0_u32;
    if unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut bytes,
        )
    } != ERROR_SUCCESS
        || bytes < 4
        || bytes > 512
    {
        return None;
    }
    let mut buffer = vec![0_u16; bytes.div_ceil(2) as usize];
    if unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut bytes,
        )
    } != ERROR_SUCCESS
    {
        return None;
    }
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16(&buffer[..end])
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .map(String::into_bytes)
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn platform_machine_identity() -> Option<Vec<u8>> {
    None
}

fn trial_device_id() -> Result<String, String> {
    let machine_id = platform_machine_identity();
    let entry = match trial_device_entry() {
        Ok(entry) => entry,
        Err(_) => {
            return machine_id
                .as_deref()
                .ok_or_else(|| "Stable trial device identity is unavailable".to_owned())
                .and_then(derive_trial_device_id)
        }
    };
    match entry.get_password() {
        Ok(value)
            if value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) =>
        {
            Ok(value)
        }
        Err(keyring::Error::NoEntry) => {
            if let Some(identity) = machine_id {
                let value = derive_trial_device_id(&identity)?;
                let _ = entry.set_password(&value);
                return Ok(value);
            }
            let mut bytes = [0_u8; 32];
            getrandom::fill(&mut bytes)
                .map_err(|_| "Could not create trial device identity".to_owned())?;
            let value = bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            entry.set_password(&value).map_err(|_| {
                "Could not save trial device identity in the system keychain".to_owned()
            })?;
            Ok(value)
        }
        Ok(_) => Err("Stored trial device identity is invalid".to_owned()),
        Err(_) => machine_id
            .as_deref()
            .ok_or_else(|| "Stable trial device identity is unavailable".to_owned())
            .and_then(derive_trial_device_id),
    }
}

fn validate_query(value: &str) -> Result<&str, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Enter a question".to_owned());
    }
    if value.chars().count() > MAX_QUERY_CHARS {
        return Err(format!(
            "Question must be at most {MAX_QUERY_CHARS} characters"
        ));
    }
    Ok(value)
}

fn validate_image_dimensions(width: u32, height: u32) -> Result<(), String> {
    if width == 0
        || height == 0
        || width > MAX_SOURCE_IMAGE_DIMENSION
        || height > MAX_SOURCE_IMAGE_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_SOURCE_IMAGE_PIXELS
    {
        return Err("Image dimensions are out of range".to_owned());
    }
    Ok(())
}

fn encode_png(image: RgbaImage) -> Result<ImagePayload, String> {
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut output, ImageFormat::Png)
        .map_err(|_| "Could not encode the image".to_owned())?;
    let bytes = output.into_inner();
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("Image is too large".to_owned());
    }
    Ok(ImagePayload {
        mime_type: "image/png".to_owned(),
        data: base64::engine::general_purpose::STANDARD.encode(bytes),
    })
}

fn normalize_rgba_image(mut image: RgbaImage) -> Result<ImagePayload, String> {
    let (width, height) = image.dimensions();
    validate_image_dimensions(width, height)?;

    if width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION {
        let longest = u64::from(width.max(height));
        let new_width =
            ((u64::from(width) * u64::from(MAX_IMAGE_DIMENSION)) / longest).max(1) as u32;
        let new_height =
            ((u64::from(height) * u64::from(MAX_IMAGE_DIMENSION)) / longest).max(1) as u32;
        image = DynamicImage::ImageRgba8(image)
            .resize_exact(new_width, new_height, imageops::FilterType::Triangle)
            .to_rgba8();
    }

    let (width, height) = image.dimensions();
    if width <= CANVAS_WIDTH && height <= CANVAS_HEIGHT {
        let mut canvas = RgbaImage::from_pixel(CANVAS_WIDTH, CANVAS_HEIGHT, Rgba(CANVAS_BG));
        let x = i64::from((CANVAS_WIDTH - width) / 2);
        let y = i64::from((CANVAS_HEIGHT - height) / 2);
        imageops::overlay(&mut canvas, &image, x, y);
        image = canvas;
    }
    encode_png(image)
}

fn normalize_image_bytes(mime_type: &str, bytes: &[u8]) -> Result<ImagePayload, String> {
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
        return Err("Image is too large or empty".to_owned());
    }
    let format = match mime_type {
        "image/png" => ImageFormat::Png,
        "image/jpeg" => ImageFormat::Jpeg,
        _ => return Err("Unsupported image type — send PNG or JPEG".to_owned()),
    };
    let dimensions = ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|_| "Unsupported or corrupt image".to_owned())?;
    validate_image_dimensions(dimensions.0, dimensions.1)?;
    let decoded = ImageReader::with_format(Cursor::new(bytes), format)
        .decode()
        .map_err(|_| "Unsupported or corrupt image".to_owned())?;
    normalize_rgba_image(decoded.to_rgba8())
}

fn normalize_image_payload(image: ImagePayload) -> Result<ImagePayload, String> {
    if image.data.len() > MAX_IMAGE_BASE64_CHARS {
        return Err("Image is too large".to_owned());
    }
    let mime_type = image.mime_type.to_ascii_lowercase();
    if mime_type != "image/png" && mime_type != "image/jpeg" {
        return Err("Unsupported image type — send PNG or JPEG".to_owned());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&image.data)
        .map_err(|_| "Invalid image data".to_owned())?;
    if base64::engine::general_purpose::STANDARD.encode(&bytes) != image.data {
        return Err("Invalid image data".to_owned());
    }
    normalize_image_bytes(&mime_type, &bytes)
}

fn bounded_body(response: ureq::Response) -> Result<Vec<u8>, RequestError> {
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| RequestError::InvalidResponse)?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(RequestError::InvalidResponse);
    }
    Ok(bytes)
}

fn parse_quota_metadata(limit: &str, remaining: &str, reset: &str) -> Option<QuotaMetadata> {
    let reset_time = DateTime::parse_from_rfc3339(reset).ok()?;
    if reset_time.offset().local_minus_utc() != 0 {
        return None;
    }
    Some(QuotaMetadata {
        limit: limit.parse().ok()?,
        remaining: remaining.parse().ok()?,
        reset: reset.to_owned(),
    })
}

fn quota_metadata(response: &ureq::Response) -> Option<QuotaMetadata> {
    parse_quota_metadata(
        response.header("X-FindOut-Daily-Limit")?,
        response.header("X-FindOut-Daily-Remaining")?,
        response.header("X-FindOut-Daily-Reset")?,
    )
}

fn response_metadata(response: &ureq::Response) -> ResponseMetadata {
    ResponseMetadata {
        quota: quota_metadata(response),
        retry_after: response
            .header("Retry-After")
            .and_then(|value| value.parse().ok())
            .filter(|seconds| *seconds > 0),
    }
}

fn response_json<T: DeserializeOwned>(response: ureq::Response) -> Result<T, RequestError> {
    let bytes = bounded_body(response)?;
    serde_json::from_slice(&bytes).map_err(|_| RequestError::InvalidResponse)
}

fn error_message(status: u16, response: ureq::Response) -> String {
    let message = bounded_body(response)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<ErrorBody>(&bytes).ok())
        .and_then(|body| body.message)
        .filter(|message| !message.is_empty())
        .map(|message| message.chars().take(512).collect::<String>());
    let message = message.unwrap_or_else(|| match status {
        401 => "Activation required".to_owned(),
        413 => "Request is too large".to_owned(),
        426 => "This FindOut version is no longer supported; update required".to_owned(),
        429 => "Too many requests; try again shortly".to_owned(),
        _ => format!("FindOut server error ({status})"),
    });
    message
}

fn post_json<T: DeserializeOwned>(
    request: ureq::Request,
    body: impl Serialize,
) -> Result<HttpResponse<T>, RequestError> {
    match request.send_json(body) {
        Ok(response) => {
            let metadata = response_metadata(&response);
            response_json(response).map(|body| HttpResponse { body, metadata })
        }
        Err(ureq::Error::Status(status, response)) => {
            let metadata = response_metadata(&response);
            Err(RequestError::Http(
                status,
                error_message(status, response),
                metadata,
            ))
        }
        Err(_) => Err(RequestError::Transport),
    }
}

fn request_message(error: RequestError) -> String {
    match error {
        RequestError::Http(_, message, _) => message,
        RequestError::Transport => "Cannot reach the FindOut server".to_owned(),
        RequestError::InvalidResponse => "Invalid response from the FindOut server".to_owned(),
    }
}

fn quota_reset(reset: &str) -> String {
    DateTime::parse_from_rfc3339(reset)
        .map(|reset| reset.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|_| reset.to_owned())
}

fn quota_status(quota: &QuotaMetadata) -> String {
    format!(
        "{}/{} left · reset {}",
        quota.remaining,
        quota.limit,
        quota_reset(&quota.reset)
    )
}

fn retryable_error(error: RequestError) -> String {
    match error {
        RequestError::Http(429, message, metadata) => metadata
            .retry_after
            .map(|seconds| format!("{message} · retry in {seconds}s"))
            .unwrap_or(message),
        error => request_message(error),
    }
}

fn quota_error(error: RequestError) -> String {
    match error {
        RequestError::Http(429, _, metadata) if matches!(metadata.quota.as_ref(), Some(quota) if quota.remaining == 0) =>
        {
            format!(
                "Daily limit reached · reset {}",
                quota_reset(&metadata.quota.unwrap().reset)
            )
        }
        RequestError::Http(429, message, metadata) => metadata
            .retry_after
            .map(|seconds| format!("{message} · retry in {seconds}s"))
            .unwrap_or(message),
        error => request_message(error),
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(60))
        .redirects(0)
        .build()
}

fn version_parts(value: &str) -> Option<[u64; 3]> {
    let mut parts = value.strip_prefix('v').unwrap_or(value).split('.');
    let version = [
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ];
    parts.next().is_none().then_some(version)
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    match (version_parts(latest), version_parts(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

fn github_release_urls() -> Option<(String, String)> {
    let repository = option_env!("FINDOUT_GITHUB_REPOSITORY")?;
    let mut parts = repository.split('/');
    let owner = parts.next()?;
    let name = parts.next()?;
    let valid_segment = |value: &str| {
        !value.is_empty()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    };
    if parts.next().is_some() || !valid_segment(owner) || !valid_segment(name) {
        return None;
    }
    Some((
        format!("https://api.github.com/repos/{repository}/releases/latest"),
        format!("https://github.com/{repository}/releases"),
    ))
}

fn check_for_update() -> Option<GitHubRelease> {
    let (api_url, _) = github_release_urls()?;
    let response = agent()
        .get(&api_url)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", &format!("FindOut/{CURRENT_VERSION}"))
        .timeout(UPDATE_CHECK_TIMEOUT)
        .call()
        .ok()?;
    let release: GitHubRelease = response_json(response).ok()?;
    is_newer_version(&release.tag_name, CURRENT_VERSION).then_some(release)
}

fn start_update_check(ui: Weak<FindOutWindow>, available: Arc<Mutex<Option<GitHubRelease>>>) {
    std::thread::spawn(move || {
        let Some(update) = check_for_update() else {
            return;
        };
        let label = format!("UPDATE {}", update.tag_name);
        *available.lock().unwrap() = Some(update);
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = ui.upgrade() {
                ui.set_update_available(label.into());
            }
        });
    });
}

fn open_releases_page() -> Result<(), String> {
    let (_, mut releases_url) = github_release_urls()
        .ok_or_else(|| "Update link is not configured for this build".to_owned())?;
    if lifecycle::whats_new() {
        releases_url.push_str(&format!("/tag/v{CURRENT_VERSION}"));
    }
    #[cfg(target_os = "windows")]
    let result = Command::new("cmd")
        .args(["/C", "start", "", releases_url.as_str()])
        .spawn();
    #[cfg(target_os = "linux")]
    let result = Command::new("xdg-open").arg(releases_url).spawn();
    #[cfg(target_os = "macos")]
    let result = Command::new("/usr/bin/open").arg(releases_url).spawn();
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    let result: Result<std::process::Child, std::io::Error> = Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "opening URLs is unsupported on this platform",
    ));
    result
        .map(|_| ())
        .map_err(|_| "Could not open the FindOut releases page".to_owned())
}

#[cfg(feature = "dev-metrics")]
fn metrics_path() -> std::io::Result<PathBuf> {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    return lifecycle::root()
        .map(|path| path.join("dev-metrics.csv"))
        .map_err(|error| std::io::Error::other(error.to_string()));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")));
    #[cfg(not(target_os = "macos"))]
    base.map(|path| path.join("findout").join("dev-metrics.csv"))
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "No local state directory")
        })
}

#[cfg(feature = "dev-metrics")]
fn append_metric(
    _request: &str,
    _response: &str,
    elapsed: Duration,
    outcome: &str,
    searched: bool,
    force_search: bool,
    had_image: bool,
) -> std::io::Result<()> {
    let path = metrics_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let needs_header = std::fs::metadata(&path)
        .map(|metadata| metadata.len() == 0)
        .unwrap_or(true);
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    if needs_header {
        writeln!(
            file,
            "timestamp_unix_ms,roundtrip_ms,outcome,searched,force_search,had_image"
        )?;
    }
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    writeln!(
        file,
        "{timestamp},{},{outcome},{searched},{force_search},{had_image}",
        elapsed.as_millis()
    )
}

#[cfg(any(feature = "dev-metrics", test))]
fn format_roundtrip(elapsed: Duration) -> String {
    if elapsed < Duration::from_secs(1) {
        format!("RT {} ms", elapsed.as_millis())
    } else {
        format!("RT {:.1} s", elapsed.as_secs_f64())
    }
}

fn activate(origin: String, key: String, ui: Weak<FindOutWindow>, in_flight: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            let key = validate_activation_key(&key)?;
            let trial_device = key
                .eq_ignore_ascii_case("trial")
                .then(trial_device_id)
                .transpose()?;
            let response: HttpResponse<ActivationResponse> = post_json(
                agent()
                    .post(&format!("{origin}/v1/activate"))
                    .set("X-FindOut-Protocol", PROTOCOL_VERSION),
                &ActivationRequest {
                    activation_key: if trial_device.is_some() { "trial" } else { key },
                    device_id: trial_device.as_deref(),
                },
            )
            .map_err(retryable_error)?;
            let body = response.body;
            if !(32..=4096).contains(&body.device_token.len())
                || !body
                    .device_token
                    .chars()
                    .all(|character| character.is_ascii_graphic())
            {
                return Err("Invalid activation response".to_owned());
            }
            token_entry(&origin)?
                .set_password(&body.device_token)
                .map_err(|_| "Could not save activation in the system keychain".to_owned())
        })();
        in_flight.store(false, Ordering::Release);
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = ui.upgrade() {
                ui.set_busy(false);
                match result {
                    Ok(()) => {
                        ui.set_activated(true);
                        ui.set_activation_key("".into());
                        ui.set_status("Activated".into());
                    }
                    Err(error) => ui.set_status(error.into()),
                }
            }
        });
    });
}

fn system_context() -> String {
    #[cfg(target_os = "linux")]
    {
        let mut parts = Vec::new();
        if let Ok(os_release) = std::fs::read_to_string("/etc/os-release") {
            if let Some(name) = os_release.lines().find_map(|line| {
                line.strip_prefix("NAME=\"")
                    .map(|name| name.trim_end_matches('"').to_owned())
            }) {
                parts.push(name);
            }
        }
        let package_managers = [
            "pacman",
            "apt-get",
            "dnf",
            "zypper",
            "apk",
            "xbps-install",
            "emerge",
        ];
        if let Some(path) = std::env::var_os("PATH") {
            let dirs: Vec<_> = std::env::split_paths(&path).collect();
            if let Some(manager) = package_managers
                .iter()
                .find(|manager| dirs.iter().any(|dir| dir.join(manager).is_file()))
            {
                parts.push(format!("pkg:{manager}"));
            }
        }
        if let Ok(shell) = std::env::var("SHELL") {
            if !shell.is_empty() {
                parts.push(format!("shell:{shell}"));
            }
        }
        parts.join(" | ")
    }
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        let info = os_info::get();
        format!("{} {}", info.os_type(), info.version())
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        String::new()
    }
}

#[allow(clippy::too_many_arguments)]
fn query(
    origin: String,
    mut request: AskRequest,
    conversation: Arc<Mutex<Conversation>>,
    attached_image: Arc<Mutex<Option<ImagePayload>>>,
    ui: Weak<FindOutWindow>,
    generation: Arc<Mutex<u64>>,
    request_generation: u64,
    in_flight: Arc<AtomicBool>,
    _started: Instant,
) {
    std::thread::spawn(move || {
        #[cfg(feature = "dev-metrics")]
        let logged_request = request.query.clone();
        #[cfg(feature = "dev-metrics")]
        let had_image = request.image.is_some();
        let result = (|| -> Result<HttpResponse<AskResponse>, String> {
            request.image = request
                .image
                .take()
                .map(normalize_image_payload)
                .transpose()?;
            request.system_context = Some(system_context());
            let device_token = token(&origin)?.ok_or_else(|| "Activation required".to_owned())?;
            let response: Result<HttpResponse<AskResponse>, RequestError> = post_json(
                agent()
                    .post(&format!("{origin}/v1/query"))
                    .set("X-FindOut-Protocol", PROTOCOL_VERSION)
                    .set("Authorization", &format!("Bearer {device_token}")),
                &request,
            );
            match response {
                Err(RequestError::Http(401, _, _)) => {
                    let _ = token_entry(&origin)?.delete_credential();
                    Err("Activation required".to_owned())
                }
                Err(error) => Err(quota_error(error)),
                Ok(answer) => Ok(answer),
            }
        })();
        #[cfg(feature = "dev-metrics")]
        let elapsed = _started.elapsed();
        #[cfg(feature = "dev-metrics")]
        {
            let (response, outcome, searched) = match &result {
                Ok(answer) => (answer.body.answer.as_str(), "ok", answer.body.searched),
                Err(error) => (error.as_str(), "error", false),
            };
            if let Err(error) = append_metric(
                &logged_request,
                response,
                elapsed,
                outcome,
                searched,
                request.force_search,
                had_image,
            ) {
                eprintln!("FindOut metrics log unavailable: {error}");
            }
        }
        #[cfg(feature = "dev-metrics")]
        let roundtrip = format_roundtrip(elapsed);
        let _ = slint::invoke_from_event_loop(move || {
            in_flight.store(false, Ordering::Release);
            if !is_current_generation(&generation, request_generation) {
                return;
            }
            if let Some(ui) = ui.upgrade() {
                ui.set_busy(false);
                #[cfg(feature = "dev-metrics")]
                ui.set_roundtrip(roundtrip.into());
                match result {
                    Ok(answer) => {
                        if let Ok(mut conversation) = conversation.lock() {
                            conversation.complete(&request, &answer.body.answer);
                        }
                        if let Ok(mut image) = attached_image.lock() {
                            *image = None;
                        }
                        ui.set_has_image(false);
                        ui.set_question("".into());
                        ui.set_answer(answer.body.answer.into());
                        ui.set_can_force_search(!answer.body.searched);
                        ui.set_status(
                            answer
                                .metadata
                                .quota
                                .as_ref()
                                .map(quota_status)
                                .unwrap_or_else(|| {
                                    if answer.body.searched {
                                        "Searched".to_owned()
                                    } else {
                                        "From knowledge".to_owned()
                                    }
                                })
                                .into(),
                        );
                    }
                    Err(error) => {
                        if error == "Activation required" {
                            ui.set_activated(false);
                        }
                        ui.set_status(error.into());
                    }
                }
            }
        });
    });
}

#[cfg(target_os = "linux")]
fn read_clipboard_image() -> Result<Option<Vec<u8>>, String> {
    gtk::init().map_err(|_| "Clipboard is unavailable".to_owned())?;
    let Some(pixbuf) = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD).wait_for_image() else {
        return Ok(None);
    };
    pixbuf
        .save_to_bufferv("png", &[])
        .map(Some)
        .map_err(|_| "Could not read the clipboard image".to_owned())
}

#[cfg(not(target_os = "linux"))]
fn read_clipboard_image() -> Result<Option<Vec<u8>>, String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|_| "Clipboard is unavailable".to_owned())?;
    let image = match clipboard.get_image() {
        Ok(image) => image,
        Err(arboard::Error::ContentNotAvailable) => return Ok(None),
        Err(_) => return Err("Could not read the clipboard image".to_owned()),
    };
    let width =
        u32::try_from(image.width).map_err(|_| "Clipboard image is out of range".to_owned())?;
    let height =
        u32::try_from(image.height).map_err(|_| "Clipboard image is out of range".to_owned())?;
    let expected = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| usize::try_from(bytes).ok())
        .unwrap_or(usize::MAX);
    if image.bytes.len() != expected {
        return Err("Clipboard image is out of range".to_owned());
    }
    let rgba = RgbaImage::from_raw(width, height, image.bytes.into_owned())
        .ok_or_else(|| "Clipboard image is out of range".to_owned())?;
    let payload = normalize_rgba_image(rgba)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.data)
        .map_err(|_| "Could not encode the clipboard image".to_owned())?;
    Ok(Some(bytes))
}

#[cfg(target_os = "linux")]
fn write_clipboard_text(text: String) -> Result<(), String> {
    gtk::init().map_err(|_| "Clipboard is unavailable".to_owned())?;
    // GTK must service paste requests while Slint runs, even with the popup hidden.
    thread_local! {
        static CLIPBOARD_EVENTS: Timer = {
            let timer = Timer::default();
            timer.start(slint::TimerMode::Repeated, Duration::from_millis(50), || {
                while gtk::events_pending() {
                    gtk::main_iteration_do(false);
                }
            });
            timer
        };
    }
    CLIPBOARD_EVENTS.with(|_| {});
    gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD).set_text(&text);
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn write_clipboard_text(text: String) -> Result<(), String> {
    arboard::Clipboard::new()
        .map_err(|_| "Clipboard is unavailable".to_owned())?
        .set_text(text)
        .map_err(|_| "Clipboard is unavailable".to_owned())
}

fn copy_answer(answer: String, ui: Weak<FindOutWindow>) {
    let result = write_clipboard_text(answer);
    if let Some(ui) = ui.upgrade() {
        if let Err(error) = result {
            ui.set_status(error.into());
        } else {
            ui.set_status("Copied".into());
        }
    }
}

fn copy_feedback(ui: Weak<FindOutWindow>, timer: &Timer) {
    let Some(window) = ui.upgrade() else {
        return;
    };
    match write_clipboard_text(FEEDBACK_EMAIL.to_owned()) {
        Ok(()) => {
            window.set_feedback_copied(true);
            timer.start(
                slint::TimerMode::SingleShot,
                Duration::from_secs(2),
                move || {
                    if let Some(ui) = ui.upgrade() {
                        ui.set_feedback_copied(false);
                    }
                },
            );
        }
        Err(error) => window.set_status(error.into()),
    }
}

fn popup_shortcut() -> HotKey {
    let modifiers = if cfg!(any(target_os = "windows", target_os = "macos")) {
        Modifiers::ALT
    } else {
        Modifiers::SUPER
    };
    HotKey::new(Some(modifiers), Code::Space)
}

#[derive(Clone, Copy, Debug)]
struct PopupGeometry {
    cursor_x: i32,
    cursor_y: i32,
    work_x: i32,
    work_y: i32,
    work_width: i32,
    work_height: i32,
    scale_factor: f32,
}

fn popup_position(geometry: PopupGeometry, width: i32, height: i32) -> slint::PhysicalPosition {
    let max_x = geometry.work_x + (geometry.work_width - width).max(0);
    let max_y = geometry.work_y + (geometry.work_height - height).max(0);
    slint::PhysicalPosition::new(
        (geometry.cursor_x - width / 2).clamp(geometry.work_x, max_x),
        (geometry.cursor_y - height / 2).clamp(geometry.work_y, max_y),
    )
}

#[cfg(target_os = "linux")]
fn popup_geometry() -> Option<PopupGeometry> {
    use gtk::prelude::*;

    gtk::init().ok()?;
    let display = gtk::gdk::Display::default()?;
    let pointer = display.default_seat()?.pointer()?;
    let (_, cursor_x, cursor_y) = pointer.position();
    let monitor = display.monitor_at_point(cursor_x, cursor_y)?;
    let workarea = monitor.workarea();
    Some(PopupGeometry {
        cursor_x,
        cursor_y,
        work_x: workarea.x(),
        work_y: workarea.y(),
        work_width: workarea.width(),
        work_height: workarea.height(),
        scale_factor: monitor.scale_factor() as f32,
    })
}

#[cfg(target_os = "windows")]
fn popup_geometry() -> Option<PopupGeometry> {
    use std::mem::size_of;
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) } == 0 {
        return None;
    }
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_null() {
        return None;
    }
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return None;
    }
    let workarea = info.rcWork;
    Some(PopupGeometry {
        cursor_x: point.x,
        cursor_y: point.y,
        work_x: workarea.left,
        work_y: workarea.top,
        work_width: workarea.right - workarea.left,
        work_height: workarea.bottom - workarea.top,
        scale_factor: 1.0,
    })
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn popup_geometry() -> Option<PopupGeometry> {
    None
}

#[cfg(not(target_os = "macos"))]
fn popup_size(ui: &FindOutWindow, scale_factor: f32) -> (i32, i32) {
    let size = ui.window().size();
    if size.width > 0 && size.height > 0 {
        (size.width as i32, size.height as i32)
    } else {
        let scale_factor = scale_factor.clamp(1.0, 4.0);
        (
            (POPUP_WIDTH as f32 * scale_factor).round() as i32,
            (POPUP_HEIGHT as f32 * scale_factor).round() as i32,
        )
    }
}

#[cfg(target_os = "macos")]
fn place_popup(ui: &FindOutWindow) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSEvent, NSScreen};
    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    let screens = NSScreen::screens(main_thread);
    let Some(primary) = screens.firstObject() else {
        return;
    };
    let primary_top = primary.frame().size.height;
    let cursor = NSEvent::mouseLocation();
    for screen in screens.iter() {
        let frame = screen.frame();
        if cursor.x < frame.origin.x
            || cursor.x >= frame.origin.x + frame.size.width
            || cursor.y < frame.origin.y
            || cursor.y >= frame.origin.y + frame.size.height
        {
            continue;
        }
        let visible = screen.visibleFrame();
        let size = ui.window().size().to_logical(ui.window().scale_factor());
        let width = if size.width > 0.0 {
            size.width
        } else {
            POPUP_WIDTH as f32
        };
        let height = if size.height > 0.0 {
            size.height
        } else {
            POPUP_HEIGHT as f32
        };
        let geometry = PopupGeometry {
            cursor_x: cursor.x.round() as i32,
            cursor_y: (primary_top - cursor.y).round() as i32,
            work_x: visible.origin.x.round() as i32,
            work_y: (primary_top - visible.origin.y - visible.size.height).round() as i32,
            work_width: visible.size.width.round() as i32,
            work_height: visible.size.height.round() as i32,
            scale_factor: screen.backingScaleFactor() as f32,
        };
        let position = popup_position(geometry, width.round() as i32, height.round() as i32);
        ui.window().set_position(slint::LogicalPosition::new(
            position.x as f32,
            position.y as f32,
        ));
        break;
    }
}

#[cfg(not(target_os = "macos"))]
fn place_popup(ui: &FindOutWindow) {
    if let Some(geometry) = popup_geometry() {
        let (width, height) = popup_size(ui, geometry.scale_factor);
        ui.window()
            .set_position(popup_position(geometry, width, height));
    }
}

fn next_generation(generation: &Arc<Mutex<u64>>) -> u64 {
    let mut value = generation
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *value = value.wrapping_add(1);
    *value
}

fn is_current_generation(generation: &Arc<Mutex<u64>>, expected: u64) -> bool {
    generation
        .lock()
        .map(|value| *value == expected)
        .unwrap_or(false)
}

fn show_window(ui: &FindOutWindow, generation: &Arc<Mutex<u64>>, focused: &Arc<AtomicBool>) {
    focused.store(false, Ordering::Release);
    next_generation(generation);
    place_popup(ui);
    let _ = ui.show();
    place_popup(ui);
    ui.window()
        .with_winit_window(|window| window.focus_window());
    if !ui.get_activated() && ui.get_status().is_empty() {
        ui.set_status("Activation required".into());
    }
}

fn hide_window(
    ui: &FindOutWindow,
    attached_image: &Arc<Mutex<Option<ImagePayload>>>,
    conversation: &Arc<Mutex<Conversation>>,
    generation: &Arc<Mutex<u64>>,
    request_generation: &Arc<Mutex<u64>>,
    focused: &Arc<AtomicBool>,
) {
    focused.store(false, Ordering::Release);
    if !ui.window().is_visible() {
        return;
    }
    let hide_generation = next_generation(generation);
    ui.set_activation_key("".into());
    let _ = ui.hide();

    let ui = ui.as_weak();
    let attached_image = attached_image.clone();
    let conversation = conversation.clone();
    let generation = generation.clone();
    let request_generation = request_generation.clone();
    Timer::single_shot(HIDE_GRACE, move || {
        if !is_current_generation(&generation, hide_generation) {
            return;
        }
        next_generation(&request_generation);
        if let Some(ui) = ui.upgrade() {
            if ui.window().is_visible() {
                return;
            }
            ui.set_busy(false);
            ui.set_has_image(false);
            ui.set_can_force_search(false);
            ui.set_answer("".into());
            ui.set_roundtrip("".into());
            ui.set_question("".into());
            ui.set_activation_key("".into());
            ui.set_status("".into());
        }
        if let Ok(mut image) = attached_image.lock() {
            *image = None;
        }
        if let Ok(mut conversation) = conversation.lock() {
            conversation.clear();
        }
    });
}

#[cfg(target_os = "linux")]
fn wait_for_tray_host() -> Result<(), String> {
    use gtk::{gio, glib::variant::ToVariant};
    let connection = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
        .map_err(|error| error.to_string())?;
    for _ in 0..30 {
        let ready = connection
            .call_sync(
                Some("org.kde.StatusNotifierWatcher"),
                "/StatusNotifierWatcher",
                "org.freedesktop.DBus.Properties",
                "Get",
                Some(
                    &(
                        "org.kde.StatusNotifierWatcher",
                        "IsStatusNotifierHostRegistered",
                    )
                        .to_variant(),
                ),
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                500,
                gio::Cancellable::NONE,
            )
            .ok()
            .and_then(|reply| reply.get::<(gtk::glib::Variant,)>())
            .and_then(|(value,)| value.get::<bool>())
            .unwrap_or(false);
        if ready {
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    Err("No desktop tray host became ready; enable a StatusNotifier/AppIndicator tray host and restart FindOut".into())
}

fn create_tray(
    ui: &FindOutWindow,
    generation: &Arc<Mutex<u64>>,
    focused: &Arc<AtomicBool>,
    install: Weak<InstallDialog>,
    uninstall: Weak<UninstallDialog>,
) -> Option<FindOutTray> {
    let tray = match FindOutTray::new() {
        Ok(tray) => {
            if let Err(error) = tray.show() {
                eprintln!("FindOut tray unavailable: {error}");
            }
            Some(tray)
        }
        Err(error) => {
            eprintln!("FindOut tray unavailable: {error}");
            None
        }
    };
    if let Some(tray) = tray.as_ref() {
        tray.set_installed(lifecycle::installed());
        tray.set_autostart(lifecycle::autostart());
        tray.on_install(move || {
            if let Some(dialog) = install.upgrade() {
                let _ = dialog.show();
            }
        });
        tray.on_uninstall(move || {
            if let Some(dialog) = uninstall.upgrade() {
                let _ = dialog.show();
            }
        });
        tray.on_toggle_autostart({
            let tray = tray.as_weak();
            let ui = ui.as_weak();
            move || {
                if let Some(tray) = tray.upgrade() {
                    match lifecycle::set_autostart(!tray.get_autostart()) {
                        Ok(()) => tray.set_autostart(lifecycle::autostart()),
                        Err(e) => {
                            if let Some(ui) = ui.upgrade() {
                                ui.set_status(e.to_string().into());
                                let _ = ui.show();
                            }
                        }
                    }
                }
            }
        });
        tray.set_theme(ui.get_theme());
        tray.on_show_window({
            let ui = ui.as_weak();
            let generation = generation.clone();
            let focused = focused.clone();
            move || {
                if let Some(ui) = ui.upgrade() {
                    show_window(&ui, &generation, &focused);
                }
            }
        });
        tray.on_select_theme({
            let ui = ui.as_weak();
            let tray = tray.as_weak();
            move |theme| {
                let theme = match theme {
                    THEME_LIGHT | THEME_DARK | THEME_RETRO => theme,
                    _ => THEME_LIGHT,
                };
                if let Some(ui) = ui.upgrade() {
                    ui.set_theme(theme);
                }
                if let Some(tray) = tray.upgrade() {
                    tray.set_theme(theme);
                }
            }
        });
        tray.on_quit(|| {
            let _ = slint::quit_event_loop();
        });
    }
    // Slint initializes trays on the next event-loop tick, including while hidden.
    Timer::single_shot(Duration::ZERO, || {});
    tray
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let origin = api_origin()?;
    let _instance_lock = lifecycle::instance_lock().map_err(|e| e.to_string())?;
    let backend = slint::BackendSelector::new().backend_name("winit-software".into());
    #[cfg(target_os = "macos")]
    let backend = {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        let mut event_loop =
            winit::event_loop::EventLoop::<slint::winit_030::SlintEvent>::with_user_event();
        event_loop.with_activation_policy(ActivationPolicy::Accessory);
        backend.with_winit_event_loop_builder(event_loop)
    };
    backend
        .with_winit_window_attributes_hook(|mut attributes| {
            #[cfg(target_os = "linux")]
            {
                use slint::winit_030::winit::platform::x11::{WindowAttributesExtX11, WindowType};
                // ponytail: X11 has no portable shadow-off hint; Utility keeps focus and
                // lets common WMs apply SKIP_TASKBAR without override-redirect.
                attributes = attributes.with_x11_window_type(vec![WindowType::Utility]);
            }
            #[cfg(target_os = "windows")]
            {
                use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;
                attributes = attributes
                    .with_skip_taskbar(true)
                    .with_undecorated_shadow(false);
            }
            attributes
        })
        .select()?;

    let manager = GlobalHotKeyManager::new()?;
    let hotkey = popup_shortcut();
    manager.register(hotkey)?;

    let ui = FindOutWindow::new()?;
    ui.set_dev_metrics(cfg!(feature = "dev-metrics"));
    if cfg!(target_os = "macos") {
        ui.set_shortcut_label("OPTION + SPACE".into());
        ui.set_paste_label("CMD+V".into());
    } else if cfg!(target_os = "windows") {
        ui.set_shortcut_label("ALT + SPACE".into());
    }
    let activated = matches!(token(&origin), Ok(Some(_)));
    ui.set_activated(activated);
    if !activated {
        ui.set_status("Activation required".into());
    }
    ui.hide()?;
    let available = Arc::new(Mutex::new(None::<GitHubRelease>));
    start_update_check(ui.as_weak(), available.clone());
    if lifecycle::installed() {
        lifecycle::remember_origin(&origin).map_err(|e| e.to_string())?;
    }
    let news_timer = Timer::default();
    if lifecycle::whats_new() {
        ui.set_update_available("WHAT’S NEW".into());
        let ui = ui.as_weak();
        news_timer.start(
            slint::TimerMode::Repeated,
            Duration::from_secs(15),
            move || {
                if !lifecycle::whats_new() {
                    if let Some(ui) = ui.upgrade() {
                        if ui.get_update_available() == "WHAT’S NEW" {
                            ui.set_update_available("".into());
                        }
                    }
                }
            },
        );
    }
    let install_dialog = InstallDialog::new()?;
    install_dialog.on_cancel({
        let d = install_dialog.as_weak();
        move || {
            if let Some(d) = d.upgrade() {
                let _ = d.hide();
            }
        }
    });
    install_dialog.on_install({
        let d = install_dialog.as_weak();
        let origin = origin.clone();
        move |startup| match lifecycle::install(startup, &origin) {
            Ok(()) => {
                let _ = slint::quit_event_loop();
            }
            Err(e) => {
                if let Some(d) = d.upgrade() {
                    d.set_error(e.to_string().into());
                }
            }
        }
    });
    let uninstall_dialog = UninstallDialog::new()?;
    uninstall_dialog.on_cancel({
        let d = uninstall_dialog.as_weak();
        move || {
            if let Some(d) = d.upgrade() {
                let _ = d.hide();
            }
        }
    });
    uninstall_dialog.on_confirm({
        let d = uninstall_dialog.as_weak();
        let origin = origin.clone();
        move || match lifecycle::uninstall(&origin) {
            Ok(()) => {
                let _ = slint::quit_event_loop();
            }
            Err(e) => {
                if let Some(d) = d.upgrade() {
                    d.set_error(format!("Could not finish uninstalling: {e}").into());
                }
            }
        }
    });
    if !cfg!(debug_assertions) && !lifecycle::installed() {
        install_dialog.show()?;
    }

    let attached_image = Arc::new(Mutex::new(None::<ImagePayload>));
    let conversation = Arc::new(Mutex::new(Conversation::default()));
    let generation = Arc::new(Mutex::new(0_u64));
    let request_generation = Arc::new(Mutex::new(0_u64));
    let focused = Arc::new(AtomicBool::new(false));
    let activation_in_flight = Arc::new(AtomicBool::new(false));
    let query_in_flight = Arc::new(AtomicBool::new(false));
    ui.on_activate({
        let origin = origin.clone();
        let ui = ui.as_weak();
        let in_flight = activation_in_flight.clone();
        move |key: slint::SharedString| {
            let key = match validate_activation_key(&key) {
                Ok(key) => key.to_owned(),
                Err(error) => {
                    if let Some(ui) = ui.upgrade() {
                        ui.set_status(error.into());
                    }
                    return;
                }
            };
            if in_flight.swap(true, Ordering::AcqRel) {
                return;
            }
            let Some(window) = ui.upgrade() else {
                in_flight.store(false, Ordering::Release);
                return;
            };
            window.set_busy(true);
            window.set_status("Activating…".into());
            activate(origin.clone(), key, ui.clone(), in_flight.clone());
        }
    });

    ui.on_submit({
        let origin = origin.clone();
        let ui = ui.as_weak();
        let attached_image = attached_image.clone();
        let conversation = conversation.clone();
        let request_state = request_generation.clone();
        let in_flight = query_in_flight.clone();
        move |text, force_search| {
            if in_flight.swap(true, Ordering::AcqRel) {
                return;
            }
            let image = attached_image.lock().ok().and_then(|image| image.clone());
            let request = conversation
                .lock()
                .unwrap()
                .prepare(&text, force_search, image);
            let request = match request {
                Ok(request) => request,
                Err(error) => {
                    in_flight.store(false, Ordering::Release);
                    if let Some(ui) = ui.upgrade() {
                        ui.set_status(error.into());
                    }
                    return;
                }
            };
            let started = Instant::now();
            let request_id = next_generation(&request_state);
            if let Some(ui) = ui.upgrade() {
                ui.set_busy(true);
                ui.set_answer("".into());
                ui.set_roundtrip("".into());
                ui.set_can_force_search(false);
                ui.set_status(
                    if force_search {
                        "Searching…"
                    } else {
                        "Finding out…"
                    }
                    .into(),
                );
            }
            query(
                origin.clone(),
                request,
                conversation.clone(),
                attached_image.clone(),
                ui.clone(),
                request_state.clone(),
                request_id,
                in_flight.clone(),
                started,
            );
        }
    });

    ui.on_paste_image({
        let ui = ui.as_weak();
        let attached_image = attached_image.clone();
        move || match read_clipboard_image() {
            Ok(Some(bytes)) => match normalize_image_bytes("image/png", &bytes) {
                Ok(payload) => {
                    if let Ok(mut image) = attached_image.lock() {
                        *image = Some(payload);
                    }
                    if let Some(ui) = ui.upgrade() {
                        ui.set_has_image(true);
                        ui.set_status("Image attached — describe what to do with it".into());
                    }
                }
                Err(error) => {
                    if let Some(ui) = ui.upgrade() {
                        ui.set_status(error.into());
                    }
                }
            },
            Ok(None) => {
                if let Some(ui) = ui.upgrade() {
                    ui.set_status("No image on the clipboard".into());
                }
            }
            Err(error) => {
                if let Some(ui) = ui.upgrade() {
                    ui.set_status(error.into());
                }
            }
        }
    });

    ui.on_clear_image({
        let attached_image = attached_image.clone();
        let ui = ui.as_weak();
        move || {
            if let Ok(mut image) = attached_image.lock() {
                *image = None;
            }
            if let Some(ui) = ui.upgrade() {
                ui.set_has_image(false);
                ui.set_status("Image removed".into());
            }
        }
    });

    ui.on_copy_answer({
        let ui = ui.as_weak();
        move || {
            let answer = ui
                .upgrade()
                .map(|ui| ui.get_answer().to_string())
                .unwrap_or_default();
            copy_answer(answer, ui.clone());
        }
    });

    ui.on_copy_feedback({
        let ui = ui.as_weak();
        let timer = Timer::default();
        move || copy_feedback(ui.clone(), &timer)
    });

    let install_weak = install_dialog.as_weak();
    ui.on_open_releases({
        let install_dialog = install_weak.clone();
        let ui = ui.as_weak();
        let in_flight = Arc::new(AtomicBool::new(false));
        move || {
            if in_flight.load(Ordering::Acquire) {
                return;
            }
            let release = available.lock().unwrap().clone();
            if let Some(release) = release {
                if !lifecycle::installed() {
                    if let Some(d) = install_dialog.upgrade() {
                        let _ = d.show();
                    }
                    return;
                }
                in_flight.store(true, Ordering::Release);
                if let Some(ui) = ui.upgrade() {
                    ui.set_update_available("UPDATING…".into());
                }
                let ui = ui.clone();
                let in_flight = in_flight.clone();
                std::thread::spawn(move || {
                    let result = lifecycle::update(&release);
                    let _ = ui.upgrade_in_event_loop(move |ui| {
                        in_flight.store(false, Ordering::Release);
                        match result {
                            Ok(()) => {
                                let _ = slint::quit_event_loop();
                            }
                            Err(e) => {
                                ui.set_update_available(
                                    format!("UPDATE {}", release.tag_name).into(),
                                );
                                ui.set_status(e.to_string().into());
                            }
                        }
                    });
                });
            } else if let Err(error) = open_releases_page() {
                if let Some(ui) = ui.upgrade() {
                    ui.set_status(error.into());
                }
            }
        }
    });

    ui.on_escape({
        let ui = ui.as_weak();
        let attached_image = attached_image.clone();
        let conversation = conversation.clone();
        let generation = generation.clone();
        let request_generation = request_generation.clone();
        let focused = focused.clone();
        move || {
            if let Some(ui) = ui.upgrade() {
                hide_window(
                    &ui,
                    &attached_image,
                    &conversation,
                    &generation,
                    &request_generation,
                    &focused,
                );
            }
        }
    });

    ui.window().on_close_requested({
        let ui = ui.as_weak();
        let attached_image = attached_image.clone();
        let conversation = conversation.clone();
        let generation = generation.clone();
        let request_generation = request_generation.clone();
        let focused = focused.clone();
        move || {
            if let Some(ui) = ui.upgrade() {
                hide_window(
                    &ui,
                    &attached_image,
                    &conversation,
                    &generation,
                    &request_generation,
                    &focused,
                );
            }
            CloseRequestResponse::KeepWindowShown
        }
    });

    ui.window().on_winit_window_event({
        let ui = ui.as_weak();
        let attached_image = attached_image.clone();
        let conversation = conversation.clone();
        let generation = generation.clone();
        let request_generation = request_generation.clone();
        let focused = focused.clone();
        move |_window, event| {
            if matches!(event, winit::event::WindowEvent::Focused(true)) {
                focused.store(true, Ordering::Release);
            } else if matches!(event, winit::event::WindowEvent::Focused(false)) {
                focused.store(false, Ordering::Release);
                let event_generation = generation.lock().map(|value| *value).unwrap_or_default();
                let ui = ui.clone();
                let attached_image = attached_image.clone();
                let conversation = conversation.clone();
                let generation = generation.clone();
                let request_generation = request_generation.clone();
                let focused = focused.clone();
                Timer::single_shot(FOCUS_LOSS_DEBOUNCE, move || {
                    if focused.load(Ordering::Acquire)
                        || !is_current_generation(&generation, event_generation)
                    {
                        return;
                    }
                    if let Some(ui) = ui.upgrade() {
                        hide_window(
                            &ui,
                            &attached_image,
                            &conversation,
                            &generation,
                            &request_generation,
                            &focused,
                        );
                    }
                });
            }
            EventResult::Propagate
        }
    });

    let tray = std::rc::Rc::new(std::cell::RefCell::new(None));
    let install_tray = {
        let install = install_dialog.as_weak();
        let uninstall = uninstall_dialog.as_weak();
        let tray = tray.clone();
        let ui = ui.as_weak();
        let generation = generation.clone();
        let focused = focused.clone();
        move || {
            if let Some(ui) = ui.upgrade() {
                *tray.borrow_mut() = create_tray(
                    &ui,
                    &generation,
                    &focused,
                    install.clone(),
                    uninstall.clone(),
                );
            }
        }
    };
    ui.on_setup_tray(install_tray);
    #[cfg(target_os = "linux")]
    {
        // Slint does not retry if its first tray creation precedes the panel at login.
        let ui = ui.as_weak();
        std::thread::spawn(move || match wait_for_tray_host() {
            Ok(()) => {
                let _ = ui.upgrade_in_event_loop(|ui| ui.invoke_setup_tray());
            }
            Err(error) => eprintln!("FindOut tray unavailable: {error}"),
        });
    }
    #[cfg(not(target_os = "linux"))]
    ui.invoke_setup_tray();

    let toggle_ui = ui.as_weak();
    let toggle_attached_image = attached_image.clone();
    let toggle_conversation = conversation.clone();
    let toggle_generation = generation.clone();
    let toggle_request_generation = request_generation.clone();
    let toggle_focused = focused.clone();
    std::thread::spawn(move || {
        for event in GlobalHotKeyEvent::receiver() {
            if event.state == HotKeyState::Pressed && event.id == hotkey.id() {
                let toggle_ui = toggle_ui.clone();
                let attached_image = toggle_attached_image.clone();
                let conversation = toggle_conversation.clone();
                let generation = toggle_generation.clone();
                let request_generation = toggle_request_generation.clone();
                let focused = toggle_focused.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = toggle_ui.upgrade() {
                        if ui.window().is_visible() {
                            hide_window(
                                &ui,
                                &attached_image,
                                &conversation,
                                &generation,
                                &request_generation,
                                &focused,
                            );
                        } else {
                            show_window(&ui, &generation, &focused);
                        }
                    }
                });
            }
        }
    });

    slint::run_event_loop_until_quit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;

    #[test]
    fn followups_serialize_successful_turns_in_order() {
        let mut conversation = Conversation::default();
        let first = conversation
            .prepare("  how do I update Linux?  ", false, None)
            .unwrap();
        assert!(serde_json::to_value(&first)
            .unwrap()
            .get("previous_turns")
            .is_none());
        conversation.complete(&first, "Use your package manager.");
        let second = conversation.prepare("and Windows?", false, None).unwrap();
        conversation.complete(&second, "Use Windows Update.");
        let third = conversation.prepare("where is that?", false, None).unwrap();
        let wire = serde_json::to_value(&third).unwrap();
        assert_eq!(
            wire["previous_turns"],
            serde_json::json!([
                {"query": "how do I update Linux?", "answer": "Use your package manager."},
                {"query": "and Windows?", "answer": "Use Windows Update."}
            ])
        );
        assert_eq!(wire["query"], "where is that?");
    }

    #[test]
    fn history_is_bounded_and_unicode_safe() {
        let mut conversation = Conversation::default();
        for i in 0..6 {
            let request = conversation
                .prepare(&format!("{i}{}", "🦀".repeat(2500)), false, None)
                .unwrap();
            conversation.complete(&request, &"ä🦀".repeat(2000));
        }
        let request = conversation.prepare("next", false, None).unwrap();
        assert_eq!(request.previous_turns.len(), 4);
        assert!(request.previous_turns[0].query.starts_with('2'));
        assert!(request.previous_turns[3].query.starts_with('5'));
        for turn in &request.previous_turns {
            assert_eq!(turn.query.chars().count(), 2000);
            assert_eq!(turn.answer.chars().count(), 2000);
        }
    }

    #[test]
    fn search_retry_reuses_context_and_image_and_replaces_answer() {
        let mut conversation = Conversation::default();
        let first = conversation.prepare("first", false, None).unwrap();
        conversation.complete(&first, "first answer");
        let second = conversation
            .prepare(
                "identify this",
                false,
                Some(ImagePayload {
                    mime_type: "image/png".into(),
                    data: "original image".into(),
                }),
            )
            .unwrap();
        conversation.complete(&second, "initial answer");
        let retry = conversation.prepare("", true, None).unwrap();
        assert_eq!(retry.query, second.query);
        assert_eq!(retry.previous_turns, second.previous_turns);
        assert_eq!(retry.image.as_ref().unwrap().data, "original image");
        assert!(retry.force_search);
        conversation.complete(&retry, "verified answer");
        let next = conversation.prepare("follow up", false, None).unwrap();
        assert_eq!(next.previous_turns.len(), 2);
        assert_eq!(next.previous_turns[1].answer, "verified answer");
        assert!(next.image.is_none());
    }

    #[test]
    fn failed_requests_are_not_history_and_clear_starts_fresh() {
        let mut conversation = Conversation::default();
        let first = conversation.prepare("first", false, None).unwrap();
        conversation.complete(&first, "answer");
        let _failed = conversation.prepare("fails", false, None).unwrap();
        assert!(conversation.prepare("  ", false, None).is_err());
        let next = conversation.prepare("next", false, None).unwrap();
        assert_eq!(next.previous_turns.len(), 1);
        assert_eq!(next.previous_turns[0].query, "first");
        conversation.clear();
        assert!(conversation.prepare("", true, None).is_err());
        assert!(conversation
            .prepare("fresh", false, None)
            .unwrap()
            .previous_turns
            .is_empty());
    }

    // Run only in a disposable X11 and D-Bus session, like clipboard_copy below.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires an isolated X11 display, xdotool and xclip"]
    fn feedback_click_copies_and_resets() {
        use winit::platform::x11::EventLoopBuilderExtX11;
        let mut event_loop =
            winit::event_loop::EventLoop::<slint::winit_030::SlintEvent>::with_user_event();
        event_loop.with_x11().with_any_thread(true);
        slint::BackendSelector::new()
            .backend_name("winit-software".into())
            .with_winit_event_loop_builder(event_loop)
            .select()
            .unwrap();
        let ui = FindOutWindow::new().unwrap();
        ui.set_activated(true);
        ui.set_answer("Use Windows Update in Settings.".into());
        ui.on_copy_feedback({
            let ui = ui.as_weak();
            let timer = Timer::default();
            move || copy_feedback(ui.clone(), &timer)
        });
        ui.show().unwrap();
        let weak = ui.as_weak();
        let reader = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            let window = Command::new("xdotool")
                .args(["search", "--name", "^FindOut$"])
                .output()
                .unwrap();
            let id = String::from_utf8(window.stdout).unwrap();
            let id = id.lines().next().unwrap();
            let click = Command::new("xdotool")
                .args(["mousemove", "--window", id, "120", "287", "click", "1"])
                .status()
                .unwrap();
            std::thread::sleep(Duration::from_millis(250));
            weak.upgrade_in_event_loop(|ui| assert!(ui.get_feedback_copied()))
                .unwrap();
            let copied = Command::new("timeout")
                .args(["5s", "xclip", "-selection", "clipboard", "-out"])
                .output()
                .unwrap();
            std::thread::sleep(Duration::from_millis(2200));
            slint::quit_event_loop().unwrap();
            (click, copied)
        });
        slint::run_event_loop_until_quit().unwrap();
        let (click, copied) = reader.join().unwrap();
        assert!(click.success());
        assert!(copied.status.success());
        assert_eq!(String::from_utf8(copied.stdout).unwrap(), FEEDBACK_EMAIL);
        assert!(!ui.get_feedback_copied());
    }

    // dbus-run-session --config-file=tests/dbus-session.conf -- xvfb-run -a \
    //   env -u WAYLAND_DISPLAY cargo test clipboard_copy -- --ignored
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires an isolated X11 display and xclip"]
    fn clipboard_copy() {
        use winit::platform::x11::EventLoopBuilderExtX11;

        let mut event_loop =
            winit::event_loop::EventLoop::<slint::winit_030::SlintEvent>::with_user_event();
        event_loop.with_x11().with_any_thread(true);
        slint::BackendSelector::new()
            .backend_name("winit-software".into())
            .with_winit_event_loop_builder(event_loop)
            .select()
            .unwrap();
        let ui = FindOutWindow::new().unwrap();
        let answer = "A copied answer.\nUnicode: ä € 🦀";
        copy_answer(answer.to_owned(), ui.as_weak());
        assert_eq!(ui.get_status(), "Copied");
        // The popup is hidden, and the reader must be a separate process:
        // GTK can satisfy an in-process read without servicing any X11 events.
        let reader = std::thread::spawn(|| {
            let output = Command::new("timeout")
                .args(["5s", "xclip", "-selection", "clipboard", "-out"])
                .output();
            slint::quit_event_loop().unwrap();
            output.unwrap()
        });
        slint::run_event_loop_until_quit().unwrap();
        let output = reader.join().unwrap();
        assert!(output.status.success(), "external paste failed: {output:?}");
        assert_eq!(String::from_utf8(output.stdout).unwrap(), answer);
    }

    // Run only in a disposable display:
    // dbus-run-session -- xvfb-run -a cargo test scrolling_ui -- --ignored
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires an isolated X11 display"]
    fn scrolling_ui() {
        use slint::platform::{Key, PointerEventButton, WindowEvent};
        use winit::platform::x11::EventLoopBuilderExtX11;

        let mut event_loop =
            winit::event_loop::EventLoop::<slint::winit_030::SlintEvent>::with_user_event();
        event_loop.with_x11().with_any_thread(true);
        slint::BackendSelector::new()
            .backend_name("winit-software".into())
            .with_winit_event_loop_builder(event_loop)
            .select()
            .unwrap();
        let ui = FindOutWindow::new().unwrap();
        ui.set_activated(true);
        ui.set_dev_metrics(cfg!(feature = "dev-metrics"));
        ui.set_roundtrip("RT 1.2 s".into());
        ui.set_answer(
            (0..50)
                .map(|n| format!("Line {n}: a long answer worth reading.\n"))
                .collect::<String>()
                .into(),
        );
        ui.show().unwrap();
        let window = ui.window();
        let region = |x: u32, y: u32, width: u32, height: u32| {
            let snapshot = window.take_snapshot().unwrap();
            (y..y + height)
                .flat_map(|row| {
                    let start = ((row * snapshot.width() + x) * 4) as usize;
                    snapshot.as_bytes()[start..start + width as usize * 4].to_vec()
                })
                .collect::<Vec<_>>()
        };
        let click = |x, y| {
            let position = slint::LogicalPosition::new(x, y);
            window.dispatch_event(WindowEvent::PointerPressed {
                position,
                button: PointerEventButton::Left,
            });
            window.dispatch_event(WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            });
        };
        let key = |text: slint::SharedString| {
            window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
            window.dispatch_event(WindowEvent::KeyReleased { text });
        };
        // Typing at the end must change the visible field, without touching the buttons.
        click(80., 80.);
        let buttons = region(435, 70, 100, 30);
        key("a long question with lots of words ".repeat(15).into());
        let before_typing = region(34, 72, 390, 25);
        key("VISIBLE END".into());
        assert!(
            before_typing != region(34, 72, 390, 25),
            "typing must stay visible"
        );
        assert!(
            buttons == region(435, 70, 100, 30),
            "text must not overlap buttons"
        );
        let end = region(34, 72, 390, 25);
        key(Key::Home.into());
        assert!(
            end != region(34, 72, 390, 25),
            "Home must scroll to the start"
        );
        // Selecting a visible line must not move the answer under the pointer.
        let lower_lines = region(34, 175, 470, 60);
        click(80., 150.);
        assert!(
            lower_lines == region(34, 175, 470, 60),
            "visible text must not jump on click"
        );
        // Wheel and keyboard navigation must reach later parts of a long answer.
        let top = region(34, 130, 470, 110);
        window.dispatch_event(WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(100., 170.),
            delta_x: 0.,
            delta_y: -200.,
        });
        assert!(
            top != region(34, 130, 470, 110),
            "wheel must scroll the answer"
        );
        click(80., 160.);
        let before_page = region(34, 130, 470, 110);
        key(Key::PageDown.into());
        assert!(
            before_page != region(34, 130, 470, 110),
            "PageDown must scroll the answer"
        );
        ui.hide().unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires an isolated X11 display"]
    fn lifecycle_dialogs_ui() {
        use winit::platform::x11::EventLoopBuilderExtX11;
        let mut event_loop =
            winit::event_loop::EventLoop::<slint::winit_030::SlintEvent>::with_user_event();
        event_loop.with_x11().with_any_thread(true);
        slint::BackendSelector::new()
            .backend_name("winit-software".into())
            .with_winit_event_loop_builder(event_loop)
            .select()
            .unwrap();
        let install = InstallDialog::new().unwrap();
        assert!(install.get_autostart());
        install.set_autostart(false);
        let selected = std::rc::Rc::new(std::cell::Cell::new(true));
        install.on_install({
            let selected = selected.clone();
            move |value| selected.set(value)
        });
        install.invoke_install(install.get_autostart());
        assert!(!selected.get());
        install.set_autostart(true);
        install.show().unwrap();
        let shot = install.window().take_snapshot().unwrap();
        image::save_buffer(
            "/tmp/findout-install-dialog.png",
            shot.as_bytes(),
            shot.width(),
            shot.height(),
            image::ColorType::Rgba8,
        )
        .unwrap();
        install.hide().unwrap();
        let uninstall = UninstallDialog::new().unwrap();
        let confirmed = std::rc::Rc::new(std::cell::Cell::new(false));
        uninstall.on_confirm({
            let confirmed = confirmed.clone();
            move || confirmed.set(true)
        });
        uninstall.show().unwrap();
        uninstall.invoke_cancel();
        assert!(!confirmed.get());
        let shot = uninstall.window().take_snapshot().unwrap();
        image::save_buffer(
            "/tmp/findout-uninstall-dialog.png",
            shot.as_bytes(),
            shot.width(),
            shot.height(),
            image::ColorType::Rgba8,
        )
        .unwrap();
        uninstall.invoke_confirm();
        assert!(confirmed.get());
        uninstall.hide().unwrap();
    }

    #[test]
    fn validates_contract_limits() {
        assert_eq!(validate_activation_key("trial").unwrap(), "trial");
        assert_eq!(validate_activation_key(" Trial ").unwrap(), "Trial");
        assert_eq!(validate_activation_key("TRIAL").unwrap(), "TRIAL");
        assert!(validate_activation_key("short").is_err());
        assert!(validate_activation_key(" activation-key ").is_ok());
        assert!(validate_query("  ").is_err());
        assert!(validate_query(&"a".repeat(MAX_QUERY_CHARS)).is_ok());
        assert!(validate_query(&"a".repeat(MAX_QUERY_CHARS + 1)).is_err());
        assert!(validate_image_dimensions(16_385, 1).is_err());
        assert!(validate_image_dimensions(10_000, 10_000).is_err());
    }

    #[test]
    fn derives_stable_private_trial_device_ids() {
        let first = derive_trial_device_id(b"raw-machine-identity").unwrap();
        let second = derive_trial_device_id(b"raw-machine-identity").unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert!(first
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
        assert_ne!(first, derive_trial_device_id(b"another-machine").unwrap());
    }

    #[test]
    fn serializes_device_id_only_for_trial_activation() {
        let ordinary = serde_json::to_value(ActivationRequest {
            activation_key: "fo_issued-key",
            device_id: None,
        })
        .unwrap();
        assert_eq!(
            ordinary,
            serde_json::json!({"activation_key": "fo_issued-key"})
        );
        let trial = serde_json::to_value(ActivationRequest {
            activation_key: "trial",
            device_id: Some("a"),
        })
        .unwrap();
        assert_eq!(
            trial,
            serde_json::json!({"activation_key": "trial", "device_id": "a"})
        );
    }

    #[test]
    fn parses_quota_headers_case_insensitively() {
        let response: ureq::Response = "HTTP/1.1 200 OK\r\nx-findout-daily-limit: 37\r\nX-FINDOUT-DAILY-REMAINING: 12\r\nX-FindOut-Daily-Reset: 2026-09-08T00:00:00.000Z\r\n\r\n{}"
            .parse()
            .unwrap();
        assert_eq!(
            quota_metadata(&response),
            Some(QuotaMetadata {
                limit: 37,
                remaining: 12,
                reset: "2026-09-08T00:00:00.000Z".to_owned(),
            })
        );
    }

    #[test]
    fn parses_only_complete_valid_utc_quota_metadata() {
        let reset = "2026-09-08T00:00:00.000Z";
        assert_eq!(
            parse_quota_metadata("37", "12", reset),
            Some(QuotaMetadata {
                limit: 37,
                remaining: 12,
                reset: reset.to_owned(),
            })
        );
        assert!(parse_quota_metadata("bad", "12", reset).is_none());
        assert!(parse_quota_metadata("37", "bad", reset).is_none());
        assert!(parse_quota_metadata("37", "12", "tomorrow").is_none());
        assert!(parse_quota_metadata("37", "12", "2026-09-08T01:00:00+01:00").is_none());
    }

    #[test]
    fn formats_server_quota_without_hardcoding_the_limit() {
        let quota = QuotaMetadata {
            limit: 37,
            remaining: 12,
            reset: "2026-09-08T00:00:00.000Z".to_owned(),
        };
        assert_eq!(
            quota_status(&quota),
            "12/37 left · reset 2026-09-08 00:00 UTC"
        );
        assert_eq!(
            quota_error(RequestError::Http(
                429,
                "ignored".to_owned(),
                ResponseMetadata {
                    quota: Some(QuotaMetadata {
                        remaining: 0,
                        ..quota
                    }),
                    retry_after: Some(30),
                }
            )),
            "Daily limit reached · reset 2026-09-08 00:00 UTC"
        );
        assert_eq!(
            quota_error(RequestError::Http(
                429,
                "Server busy".to_owned(),
                ResponseMetadata {
                    quota: None,
                    retry_after: Some(30),
                }
            )),
            "Server busy · retry in 30s"
        );
    }

    #[test]
    fn compares_release_versions() {
        assert!(is_newer_version("v0.2.0", "0.1.0"));
        assert!(is_newer_version("0.1.1", "0.1.0"));
        assert!(is_newer_version("v0.10.0", "0.9.0"));
        assert!(!is_newer_version("v0.1.0", "0.1.0"));
        assert!(!is_newer_version("v0.0.9", "0.1.0"));
        assert!(!is_newer_version("latest", "0.1.0"));
    }

    #[test]
    fn normalizes_small_and_large_images() {
        let small = RgbaImage::from_pixel(2, 1, Rgba([255, 0, 0, 255]));
        let padded = normalize_rgba_image(small).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(padded.data)
            .unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (CANVAS_WIDTH, CANVAS_HEIGHT));
        assert_eq!(decoded.get_pixel(0, 0).0, CANVAS_BG);

        let wide = RgbaImage::from_pixel(4_097, 1, Rgba([0, 255, 0, 255]));
        let resized = normalize_rgba_image(wide).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(resized.data)
            .unwrap();
        assert_eq!(
            image::load_from_memory(&bytes).unwrap().dimensions(),
            (MAX_IMAGE_DIMENSION, 1)
        );
    }

    #[test]
    fn keeps_popup_inside_workarea() {
        let geometry = PopupGeometry {
            cursor_x: 1_910,
            cursor_y: 1_070,
            work_x: 0,
            work_y: 0,
            work_width: 1_920,
            work_height: 1_080,
            scale_factor: 1.0,
        };
        let position = popup_position(geometry, POPUP_WIDTH, POPUP_HEIGHT);
        assert_eq!(position, slint::PhysicalPosition::new(1_360, 760));
    }

    #[test]
    fn formats_dev_roundtrip_at_a_glance() {
        assert_eq!(format_roundtrip(Duration::from_millis(812)), "RT 812 ms");
        assert_eq!(format_roundtrip(Duration::from_millis(1_250)), "RT 1.2 s");
    }
}
