use super::*;

#[test]
fn legacy_history_and_relative_times() {
    let old: HistoryTurn = serde_json::from_str(
        r#"{"query":"Hello?","answer":"Hello.","searched":false,"had_image":false}"#,
    )
    .unwrap();
    assert_eq!(old.answered_at, 0);
    assert_eq!(relative_age(0, 500), "");
    assert_eq!(relative_age(600, 500), "Just now");
    assert_eq!(relative_age(100, 220), "2m ago");
    assert_eq!(relative_age(100, 7300), "2h ago");
    assert_eq!(relative_age(100, 90_000), "Yesterday");
    assert_eq!(relative_age(100, 260_000), "3d ago");
}

#[test]
fn recent_sessions_keep_full_threads_in_newest_first_order() {
    use slint::Model;
    let mut c = Conversation::default();
    let r = c.prepare("First question", false, None).unwrap();
    c.complete(&r, "First answer");
    let r = c.prepare("Follow-up", false, None).unwrap();
    c.complete(&r, "Follow-up answer");
    c.clear();
    let r = c.prepare("New session", false, None).unwrap();
    c.complete(&r, "New answer");
    let model = recent_sessions(&c);
    assert_eq!(model.row_count(), 2);
    assert_eq!(model.row_data(0).unwrap().title, "New session");
    let first = model.row_data(1).unwrap();
    assert_eq!(first.title, "First question");
    assert!(first.transcript.contains("First answer"));
    assert!(first.transcript.contains("Follow-up answer"));
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires an isolated X11 display"]
fn monolith_ui() {
    use slint::platform::{PointerEventButton, WindowEvent};
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
    let tray = FindOutTray::new().unwrap();
    bind_tray_theme(&tray, &ui);
    assert!(ui.get_light_theme() && tray.get_light_theme());
    tray.invoke_toggle_theme();
    assert!(!ui.get_light_theme() && !tray.get_light_theme());
    tray.invoke_toggle_theme();
    assert!(ui.get_light_theme() && tray.get_light_theme());
    if std::env::var_os("FINDOUT_UI_DARK").is_some() {
        tray.invoke_toggle_theme();
    }
    bind_tray_motion(&tray, &ui);
    assert!(!tray.get_reduced_motion());
    tray.invoke_toggle_motion();
    assert!(!ui.get_motion() && tray.get_reduced_motion());
    tray.invoke_toggle_motion();
    assert!(ui.get_motion() && !tray.get_reduced_motion());
    let feedback = FeedbackWindow::new().unwrap();
    ui.on_open_feedback({
        let feedback = feedback.as_weak();
        move || feedback.upgrade().unwrap().show().unwrap()
    });
    let clicked_answer = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let copied = std::rc::Rc::new(std::cell::RefCell::new("untouched".to_string()));
    ui.on_copy_answer({
        let copied = copied.clone();
        let ui = ui.as_weak();
        move || *copied.borrow_mut() = ui.upgrade().unwrap().get_answer().to_string()
    });
    ui.on_copy_text({
        let copied = copied.clone();
        move |text| *copied.borrow_mut() = text.to_string()
    });
    ui.on_open_history({
        let ui = ui.as_weak();
        move || {
            let ui = ui.upgrade().unwrap();
            ui.set_expanded_session(-1);
            ui.set_recent_open(true);
        }
    });
    let resumed = std::rc::Rc::new(std::cell::Cell::new(-1));
    ui.on_resume_session({
        let resumed = resumed.clone();
        move |i| resumed.set(i)
    });
    let mut c = Conversation::default();
    for (q, a) in [
        ("What makes a habit stick?", "Repeat a small action in a consistent context."),
        ("The difference between focus and flow", "Focus is directing attention. Flow is a state of deep absorption."),
        ("How much sleep do we actually need?", "Most adults need seven to nine hours."),
        ("A quieter way to start the morning", "Give yourself a few minutes before checking your phone."),
        ("Why does coffee taste different as it cools?", "Temperature changes how we perceive sweetness, acidity and bitterness. As coffee cools, its subtle flavors become easier to notice."),
    ] {
        c.clear(); let request = c.prepare(q, false, None).unwrap(); c.complete(&request, a);
    }
    ui.set_sessions(recent_sessions(&c));
    fn snapshot(ui: &FindOutWindow, name: &str) {
        let shot = ui.window().take_snapshot().unwrap();
        let directory = std::env::var("FINDOUT_UI_CAPTURE_DIR")
            .unwrap_or_else(|_| "/tmp/findout-monolith".into());
        std::fs::create_dir_all(&directory).unwrap();
        image::save_buffer(
            format!("{directory}/{name}.png"),
            shot.as_bytes(),
            shot.width(),
            shot.height(),
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
    fn answer_pixels(ui: &FindOutWindow) -> Vec<u8> {
        let shot = ui.window().take_snapshot().unwrap();
        (73..220)
            .flat_map(|y| {
                let offset = ((y * shot.width() + 24) * 4) as usize;
                shot.as_bytes()[offset..offset + 502 * 4].to_vec()
            })
            .collect()
    }
    fn click(ui: &FindOutWindow, x: f32, y: f32, button: PointerEventButton) {
        let position = slint::LogicalPosition::new(x, y);
        ui.window()
            .dispatch_event(WindowEvent::PointerPressed { position, button });
        ui.window()
            .dispatch_event(WindowEvent::PointerReleased { position, button });
    }
    let opened_updates = std::rc::Rc::new(std::cell::Cell::new(0));
    ui.on_open_releases({
        let opened = opened_updates.clone();
        move || opened.set(opened.get() + 1)
    });
    ui.show().unwrap();
    ui.set_update_available("UPDATE v0.1.7".into());
    click(&ui, 470., 50., PointerEventButton::Left);
    assert_eq!(opened_updates.get(), 1);
    ui.set_busy(true);
    click(&ui, 470., 50., PointerEventButton::Left);
    assert_eq!(opened_updates.get(), 1);
    ui.set_busy(false);
    ui.set_update_available("WHAT’S NEW".into());
    click(&ui, 470., 50., PointerEventButton::Left);
    assert_eq!(opened_updates.get(), 2);
    ui.set_update_available("".into());
    ui.invoke_focus_input();
    Timer::single_shot(Duration::from_millis(100), {
        let ui = ui.as_weak();
        let copied = copied.clone();
        move || {
            let ui = ui.upgrade().unwrap();
            click(&ui, 80., 120., PointerEventButton::Right);
            assert_eq!(
                *copied.borrow(),
                "untouched",
                "empty answer must not replace the clipboard"
            );
            snapshot(&ui, "01-empty");
            present_answer(
                &ui,
                "When is the next solar eclipse?",
                "AUG 12",
                true,
                false,
            );
        }
    });
    Timer::single_shot(Duration::from_millis(320), {
        let ui = ui.as_weak();
        move || snapshot(&ui.upgrade().unwrap(), "02-moving")
    });
    for (time, name) in [(430, "02-flow-a"), (490, "02-flow-b"), (560, "02-flow-c")] {
        Timer::single_shot(Duration::from_millis(time), {
            let ui = ui.as_weak();
            move || snapshot(&ui.upgrade().unwrap(), name)
        });
    }
    Timer::single_shot(Duration::from_millis(750), {
        let ui = ui.as_weak();
        let clicked_answer = clicked_answer.clone();
        move || {
            let ui = ui.upgrade().unwrap();
            let before = answer_pixels(&ui);
            click(&ui, 80., 120., PointerEventButton::Left);
            let after = answer_pixels(&ui);
            assert!(
                before == after,
                "clicking the answer must not reveal a caret"
            );
            *clicked_answer.borrow_mut() = after;
        }
    });
    Timer::single_shot(Duration::from_millis(1350), {
        let ui = ui.as_weak();
        let clicked_answer = clicked_answer.clone();
        let copied = copied.clone();
        move || {
            let ui = ui.upgrade().unwrap();
            assert!(
                *clicked_answer.borrow() == answer_pixels(&ui),
                "focused answer must not blink"
            );
            snapshot(&ui, "03-answer");
            // Actual right press in the text region must reach copy; no overlay can steal selection.
            click(&ui, 80., 120., PointerEventButton::Right);
            assert_eq!(*copied.borrow(), "AUG 12");
            // Selection is visible after a real left drag across the answer.
            let before = ui.window().take_snapshot().unwrap();
            ui.window().dispatch_event(WindowEvent::PointerPressed {
                position: slint::LogicalPosition::new(35., 110.),
                button: PointerEventButton::Left,
            });
            ui.window().dispatch_event(WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(150., 110.),
            });
            ui.window().dispatch_event(WindowEvent::PointerReleased {
                position: slint::LogicalPosition::new(150., 110.),
                button: PointerEventButton::Left,
            });
            let after = ui.window().take_snapshot().unwrap();
            assert!(
                before.as_bytes() != after.as_bytes(),
                "left dragging must select answer text"
            );
            snapshot(&ui, "04-selection");
            click(&ui, 506., 22., PointerEventButton::Left);
            assert!(ui.get_recent_open());
        }
    });
    Timer::single_shot(Duration::from_millis(1700), {
        let ui = ui.as_weak();
        move || {
            let ui = ui.upgrade().unwrap();
            snapshot(&ui, "05-recent");
            click(&ui, 120., 89., PointerEventButton::Left);
            assert_eq!(ui.get_expanded_session(), 0);
        }
    });
    Timer::single_shot(Duration::from_millis(1850), {
        let ui = ui.as_weak();
        move || snapshot(&ui.upgrade().unwrap(), "06-expanding")
    });
    Timer::single_shot(Duration::from_millis(2200), {
        let ui = ui.as_weak();
        let copied = copied.clone();
        let resumed = resumed.clone();
        move || {
            let ui = ui.upgrade().unwrap();
            snapshot(&ui, "07-expanded");
            click(&ui, 100., 130., PointerEventButton::Right);
            assert!(copied.borrow().contains("Temperature changes"));
            // Continue belongs to the row that was expanded.
            click(&ui, 100., 170., PointerEventButton::Left);
            assert_eq!(resumed.get(), 0);
            click(&ui, 120., 89., PointerEventButton::Left);
            assert_eq!(ui.get_expanded_session(), -1);
        }
    });
    Timer::single_shot(Duration::from_millis(2750), {
        let ui = ui.as_weak();
        let feedback = feedback.as_weak();
        move || {
            let ui = ui.upgrade().unwrap();
            snapshot(&ui, "08-collapsed");
            click(&ui, 506., 22., PointerEventButton::Left);
            assert!(!ui.get_recent_open());
            ui.set_motion(false);
            reset_presentation(&ui);
            ui.set_answer("".into());
            ui.set_activated(false);
            snapshot(&ui, "09-activation");
            click(&ui, 498., 231., PointerEventButton::Left);
            let feedback = feedback.upgrade().unwrap();
            assert!(
                feedback.window().is_visible(),
                "Feedback text must open the feedback window"
            );
            feedback.hide().unwrap();
            slint::quit_event_loop().unwrap();
        }
    });
    slint::run_event_loop_until_quit().unwrap();
}
