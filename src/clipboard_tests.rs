use super::*;
use image::GenericImageView;

struct ImageFixture(std::path::PathBuf);

impl ImageFixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("findout-paste-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, format) in [
            ("image ü #.png", ImageFormat::Png),
            ("photo.jpg", ImageFormat::Jpeg),
        ] {
            DynamicImage::new_rgb8(16, 16)
                .save_with_format(dir.join(name), format)
                .unwrap();
        }
        std::fs::write(dir.join("text.png"), "not an image").unwrap();
        let large = std::fs::File::create(dir.join("large.png")).unwrap();
        large.set_len(MAX_IMAGE_BYTES as u64 + 1).unwrap();
        Self(dir)
    }

    fn uri(&self, name: &str) -> String {
        url::Url::from_file_path(self.0.join(name)).unwrap().into()
    }
}

impl Drop for ImageFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn copied_image_files() {
    let fixture = ImageFixture::new();
    for name in ["image ü #.png", "photo.jpg"] {
        let payload = read_copied_image(&[fixture.uri(name)]).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(payload.data)
            .unwrap();
        assert_eq!(payload.mime_type, "image/png");
        assert_eq!(
            image::load_from_memory(&bytes).unwrap().dimensions(),
            (CANVAS_WIDTH, CANVAS_HEIGHT)
        );
    }
    for name in ["text.png", "large.png", "missing.png", ""] {
        assert!(read_copied_image(&[fixture.uri(name)]).is_err(), "{name}");
    }
    for uri in [
        "https://example.com/image.png",
        "file://remote/tmp/image.png",
        "/tmp/image.png",
    ] {
        assert!(read_copied_image(&[uri]).is_err(), "{uri}");
    }
    assert!(read_copied_image(&[] as &[&str]).is_err());
    assert!(read_copied_image(&[fixture.uri("photo.jpg"), fixture.uri("image ü #.png")]).is_err());
}

// dbus-run-session --config-file=tests/dbus-session.conf -- xvfb-run -a \
//   env -u WAYLAND_DISPLAY -u XDG_RUNTIME_DIR cargo test --features local-trial clipboard_paste_ui -- --ignored
#[test]
#[ignore = "requires an isolated X11 display and xclip"]
fn clipboard_paste_ui() {
    use slint::platform::{Key, WindowEvent};
    use std::io::Write as _;
    use std::process::Stdio;
    use winit::platform::x11::EventLoopBuilderExtX11;

    let mut event_loop =
        winit::event_loop::EventLoop::<slint::winit_030::SlintEvent>::with_user_event();
    event_loop.with_x11().with_any_thread(true);
    slint::BackendSelector::new()
        .backend_name("winit-software".into())
        .with_winit_event_loop_builder(event_loop)
        .select()
        .unwrap();
    gtk::init().unwrap();
    let fixture = ImageFixture::new();
    let ui = FindOutWindow::new().unwrap();
    let attached = Arc::new(Mutex::new(None));
    ui.on_paste_image({
        let ui = ui.as_weak();
        let attached = attached.clone();
        move || paste_clipboard_image(&ui.unwrap(), &attached)
    });
    ui.set_activated(true);
    ui.set_motion(false);
    ui.show().unwrap();
    ui.invoke_focus_input();
    let paste = || {
        for text in [slint::SharedString::from(Key::Control), "v".into()] {
            ui.window().dispatch_event(WindowEvent::KeyPressed { text });
        }
        for text in [slint::SharedString::from("v"), Key::Control.into()] {
            ui.window()
                .dispatch_event(WindowEvent::KeyReleased { text });
        }
    };
    let clipboard = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
    let copy_files = |uris: Vec<String>| {
        assert!(clipboard.set_with_data(
            &[
                gtk::TargetEntry::new("text/uri-list", gtk::TargetFlags::empty(), 0),
                gtk::TargetEntry::new("UTF8_STRING", gtk::TargetFlags::empty(), 1),
            ],
            move |_, data, info| {
                if info == 0 {
                    data.set_uris(&uris.iter().map(String::as_str).collect::<Vec<_>>());
                } else {
                    data.set_text("THIS PATH MUST NOT BE PASTED");
                }
            }
        ));
    };
    ui.set_question("describe this".into());
    for name in ["image ü #.png", "photo.jpg"] {
        copy_files(vec![fixture.uri(name)]);
        paste();
        assert!(ui.get_has_image());
        assert_eq!(ui.get_question(), "describe this");
        let image = attached.lock().unwrap().clone().unwrap();
        let request = Conversation::default()
            .prepare(&ui.get_question(), false, Some(image))
            .unwrap();
        assert!(
            serde_json::to_value(request).unwrap()["image"]["data"]
                .as_str()
                .unwrap()
                .len()
                > 100
        );
    }
    let previous = attached.lock().unwrap().clone().unwrap().data;
    for uris in [
        vec![fixture.uri("missing.png")],
        vec![fixture.uri("text.png")],
        vec![fixture.uri("photo.jpg"); 2],
    ] {
        copy_files(uris);
        paste();
        assert_eq!(ui.get_question(), "describe this");
        assert_eq!(attached.lock().unwrap().as_ref().unwrap().data, previous);
        assert!(!ui.get_status().starts_with("Image attached"));
    }
    let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_file(fixture.0.join("image ü #.png")).unwrap();
    clipboard.set_image(&pixbuf);
    paste();
    assert!(ui.get_status().starts_with("Image attached"));
    assert_eq!(ui.get_question(), "describe this");

    // An independent owner lets the default Slint text-paste code read X11
    // without relying on this test thread to service GTK events.
    let mut owner = Command::new("xclip")
        .args(["-selection", "clipboard", "-in"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    owner
        .stdin
        .take()
        .unwrap()
        .write_all(b"/tmp/ordinary-path.png")
        .unwrap();
    assert!(owner.wait().unwrap().success());
    while gtk::events_pending() {
        gtk::main_iteration_do(false);
    }
    assert!(read_clipboard_image().unwrap().is_none());
    ui.set_question("".into());
    ui.set_status("unchanged".into());
    paste();
    assert_eq!(ui.get_status(), "unchanged");
    assert_eq!(ui.get_question(), "/tmp/ordinary-path.png");
    ui.hide().unwrap();
}
