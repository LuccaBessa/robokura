//! The scroller draws as many items as it has been told about, not as many as it is
//! handed. A pane that keeps messages and never tells the scroller looks empty
//! while holding a whole conversation.

use gpui_kit::{
    AppContext as _, Bounds, Point, TestAppContext, WindowBounds, WindowOptions, px, size,
    test::TestWindowExt,
};
use robokura::ui::main_thread::ThreadPane;
use robokura_core::Core;

#[gpui_kit::test]
fn the_pane_tells_the_scroller_how_many_rows_there_are(cx: &mut TestAppContext) {
    let root = std::env::temp_dir()
        .join("robokura-main-thread-checks")
        .join("item-count");
    let _ = std::fs::remove_dir_all(&root);

    let mut core = Core::open_at(root).expect("the store opened");
    let assistant = core
        .create_assistant("Letters", "drafts", "writes in my voice")
        .expect("the assistant was made");
    for text in ["first", "second", "third"] {
        core.send(&assistant.id, text)
            .expect("the message was stored");
    }

    // Read back rather than counted by hand: whether an agent is installed changes
    // how many rows there are, and this has to hold on a machine with none.
    let stored = core
        .messages(&assistant.id)
        .expect("the thread reads")
        .len();
    assert!(stored >= 3, "the questions are on record");

    cx.update(gpui_kit::init);

    let assistant_id = assistant.id.clone();
    let (window, view) = cx.update(|cx| {
        let core = cx.new(|_| core);
        let assistant = core
            .read(cx)
            .assistant(&assistant_id)
            .expect("the assistant reads")
            .expect("the assistant is there");

        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point::default(),
                    size: size(px(900.), px(600.)),
                })),
                ..Default::default()
            },
            cx,
            move |window, cx| cx.new(|cx| ThreadPane::new(Some(core), assistant, window, cx)),
        )
        .expect("the window opened")
    });

    // Two frames: the first reads the thread and sets the count, and setting it
    // asks for another.
    for _ in 0..2 {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
        })
        .expect("the window is open");
    }

    let told = cx.update(|cx| view.read(cx).scroller.read(cx).item_count());
    assert_eq!(told, stored, "the scroller was told about every stored row");

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
    })
    .expect("the window is open");
    let again = cx.update(|cx| view.read(cx).scroller.read(cx).item_count());
    assert_eq!(
        again, stored,
        "and the count settles rather than resetting again"
    );
}
