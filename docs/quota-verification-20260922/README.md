# Desktop quota verification — 2026-09-22

Passed against the real Linux desktop executable built with `cargo build`, driven through its UI using xdotool. The test creates a private Xvfb display, private D-Bus with service activation disabled, its own explicitly started Secret Service daemon, temporary XDG directories, and a loopback mock HTTP backend. No host desktop or real credentials are used. Screenshot files show only synthetic content.

| Check | Observed behavior |
| --- | --- |
| Before activation | Activation form is visible. |
| Activation | Synthetic activation token saved through real Secret Service; form disappears; quota blank; zero queries consumed. |
| Successful query | `12/37 left · reset 2026-09-22 00:00 UTC` from response headers. |
| Exhaustion | `Daily limit reached · 0/37 left · reset 2026-09-22 00:00 UTC`; old 12 remaining removed. |
| Full process restart | Saved token restored; no second activation; no discovery query; old quota absent. |
| Changed backend values | Next response displays `4/9 left · reset 2026-09-23 00:00 UTC`, replacing previous limit/reset. |
| Error without quota headers | `Please try again later`; previous allowance/reset removed. |

The backend counted exactly one activation and four queries and checked the persisted token on every query. `result.json` records the seven successful checkpoints. Numbered screenshots preserve each visible state. The screenshot/OCR checks assert identifying text and quota/day markers; at this UI's 8px status font OCR confuses some zero/year digits, so the full reset timestamps and zero balance were additionally verified visually in screenshots. This is Linux/X11 at scale 1, not a Windows/macOS or high-DPI certification.

Reproduce from the client root:

```sh
cargo build
python3 tests/quota-desktop.py
cargo test quota
```

Requirements: Python 3, dbus-run-session, xvfb-run/Xvfb, xfwm4, xdotool, gnome-keyring-daemon, ImageMagick (`import`, `convert`, `magick`), and Tesseract. Output defaults to `target/quota-desktop`; an optional positional path selects another output directory. The script cleans up its desktop, keychain process, mock backend and temporary storage. Three parser/formatter quota tests also passed. No application code changes were needed.
