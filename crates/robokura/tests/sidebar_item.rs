//! What an item in the sidebar says, and where each part of it lands.
//!
//! These go through a real window rather than by asking the render what it was
//! given, because what has to be true is where the parts of a row ended up
//! relative to one another.

use gpui_kit::{
    AppContext as _, Bounds, Point, TestAppContext, WindowBounds, WindowOptions,
    component::ThemeMode, px, size, test::TestWindowExt as _,
};
use robokura::ui::sidebar::AssistantPane;
use robokura_core::{Core, Message, paths, store::Store};

/// A window over a store of its own, holding whatever `fill` puts in it, and the ids
/// `fill` hands back.
fn window_over(
    cx: &mut TestAppContext,
    name: &str,
    fill: impl FnOnce(&mut Core, &std::path::Path) -> Vec<String>,
) -> (gpui_kit::AnyWindowHandle, Vec<String>) {
    cx.update(gpui_kit::init);
    cx.update(|cx| robokura::theme::apply(cx, ThemeMode::Dark));

    let root = std::env::temp_dir().join("robokura-item-checks").join(name);
    let _ = std::fs::remove_dir_all(&root);

    let mut core = Core::open_at(root.clone()).expect("the store opened");
    let ids = fill(&mut core, &root);

    let (window, _) = cx.update(|cx| {
        let store = cx.new(|_| core);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point::default(),
                    size: size(px(1400.), px(800.)),
                })),
                ..Default::default()
            },
            cx,
            move |window, cx| cx.new(|cx| AssistantPane::new(Some(store), None, window, cx)),
        )
        .expect("the window opened")
    });

    for _ in 0..3 {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
        })
        .expect("the window is open");
    }
    (window, ids)
}

/// One assistant, and `said` in its thread when there is something to say.
///
/// The message is written to the store rather than sent, because sending starts an
/// agent wherever one is installed, and what the item shows would then depend on
/// the machine running the check.
fn window_over_one(
    cx: &mut TestAppContext,
    name: &str,
    said: Option<&str>,
) -> (gpui_kit::AnyWindowHandle, String) {
    let (window, mut ids) = window_over(cx, name, |core, root| {
        let assistant = core
            .create_assistant("Letters", "drafts my letters", "")
            .expect("the assistant was made");

        if let Some(text) = said {
            say_in(root, &assistant.id, text);
        }

        vec![assistant.id]
    });
    (window, ids.remove(0))
}

/// A message written straight into an assistant's thread, leaving the store open for the
/// window to read.
fn say_in(root: &std::path::Path, assistant: &str, text: &str) {
    let store = Store::open(&paths::db_file(root)).expect("the store opened again");
    let thread = store
        .thread_for(assistant)
        .expect("read")
        .expect("the assistant has a thread");
    let seq = store.next_seq(&thread.id).expect("read");
    store
        .add_message(&Message::person(&thread.id, seq, text))
        .expect("stored");
}

fn middle(bounds: gpui_kit::Bounds<gpui_kit::Pixels>) -> gpui_kit::Pixels {
    bounds.origin.y + bounds.size.height * 0.5
}

#[gpui_kit::test]
fn an_item_says_the_name_the_last_message_and_when_it_was_said(cx: &mut TestAppContext) {
    let (window, id) = window_over_one(
        cx,
        "item-shape",
        Some("write me a letter about the invoice"),
    );

    cx.update_window(window, |_, window, _| {
        let name = window
            .try_find(format!("assistant-{id}-name"))
            .expect("the name is drawn");
        let when = window
            .try_find(format!("assistant-{id}-when"))
            .expect("the time the last message was said is drawn");
        let last = window
            .try_find(format!("assistant-{id}-last"))
            .expect("the last message is drawn");
        let initials = window
            .try_find(format!("assistant-{id}-initials"))
            .expect("the initials are drawn");

        assert!(
            (middle(name.bounds()) - middle(when.bounds())).abs() < px(8.),
            "the name and the time are on one line. The time belongs beside the name, because \
             it is when the name was last spoken to, not when the message under it was written."
        );
        assert!(
            name.bounds().origin.x < when.bounds().origin.x,
            "with the time on the far side of the name rather than at the far end of the item"
        );

        assert!(
            last.bounds().origin.y > when.bounds().origin.y + when.bounds().size.height,
            "and the last message is under both of them. A name, what it last said and when it \
             said it are three things, so the item grows to three lines rather than two."
        );

        assert!(
            initials.bounds().origin.x < name.bounds().origin.x
                && initials.bounds().origin.x < when.bounds().origin.x,
            "the initials lead the item, before the name rather than after the time"
        );
    })
    .expect("the window is open");
}

#[gpui_kit::test]
fn an_item_with_nothing_in_its_thread_centres_the_name(cx: &mut TestAppContext) {
    let (window, id) = window_over_one(cx, "item-nothing-said", None);

    cx.update_window(window, |_, window, _| {
        let row = window
            .try_find(format!("assistant-{id}"))
            .expect("the item is drawn");
        let name = window
            .try_find(format!("assistant-{id}-name"))
            .expect("the name is drawn");

        assert!(
            window.try_find(format!("assistant-{id}-last")).is_none(),
            "nothing is drawn under the name. There is no last message to draw, and a second \
             line of nothing reads as an assistant that has gone quiet."
        );
        assert!(
            window.try_find(format!("assistant-{id}-when")).is_none(),
            "and no time either, because there is no message for one to be the time of"
        );

        assert!(
            (middle(row.bounds()) - middle(name.bounds())).abs() < px(2.),
            "so the name stands on the middle of the item rather than sitting above a line that \
             says nothing. It sat {:?} off the middle.",
            middle(row.bounds()) - middle(name.bounds())
        );
    })
    .expect("the window is open");
}

#[gpui_kit::test]
fn every_item_is_the_same_height(cx: &mut TestAppContext) {
    let (window, ids) = window_over(cx, "item-same-height", |core, root| {
        let quiet = core
            .create_assistant("Letters", "drafts my letters", "")
            .expect("the assistant was made");
        let talking = core
            .create_assistant("Accounts", "reads my accounts", "")
            .expect("the assistant was made");
        say_in(root, &talking.id, "the invoice is on the second page");

        vec![quiet.id, talking.id]
    });

    cx.update_window(window, |_, window, _| {
        let quiet = window
            .try_find(format!("assistant-{}", ids[0]))
            .expect("the quiet item is drawn");
        let talking = window
            .try_find(format!("assistant-{}", ids[1]))
            .expect("the talking item is drawn");

        let quiet_height = quiet.bounds().size.height;
        let talking_height = talking.bounds().size.height;

        // Held to a pixel rather than exactly equal, because the tall row keeps the
        // height its own content comes to and only the short one is lifted to it.
        assert!(
            (quiet_height - talking_height).abs() < px(1.),
            "an assistant with nothing in its thread takes the same room as one with a message \
             in it, or the list moves every time a message arrives. The quiet item is {:?} and \
             the talking one {:?}.",
            quiet_height,
            talking_height
        );
    })
    .expect("the window is open");
}
