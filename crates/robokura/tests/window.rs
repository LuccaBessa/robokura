//! What the window does with the assistant's own pane.
//!
//! These go through the same path a person does: the toggle in the bar, the delete
//! control, and the question it asks.

use gpui_kit::{
    AppContext as _, Bounds, Entity, TestAppContext, WindowBounds, WindowOptions,
    component::{ActiveTheme as _, ThemeMode, WindowExt as _},
    px, size,
    test::TestWindowExt as _,
};
use robokura::ui::{AppView, layout, sidebar::AssistantPane};
use robokura_core::{Core, Message, paths, store::Store};

/// A window over a store of its own, with `assistants` assistants already in it.
fn window_with(
    cx: &mut TestAppContext,
    name: &str,
    assistants: &[(&str, &str, &str)],
) -> (gpui_kit::AnyWindowHandle, Entity<AppView>, Vec<String>) {
    window_over(cx, name, |core, _root| {
        assistants
            .iter()
            .map(|(name, title, description)| {
                core.create_assistant(*name, *title, *description)
                    .expect("the assistant was made")
                    .id
            })
            .collect()
    })
}

/// The same, over a store that `fill` puts whatever it likes into. A check that has to
/// say something in a thread has to do it before the window is built, because the
/// window decides which assistant it opens on as it is built.
fn window_over(
    cx: &mut TestAppContext,
    name: &str,
    fill: impl FnOnce(&mut Core, &std::path::Path) -> Vec<String>,
) -> (gpui_kit::AnyWindowHandle, Entity<AppView>, Vec<String>) {
    cx.update(gpui_kit::init);
    cx.update(|cx| robokura::theme::apply(cx, ThemeMode::Dark));

    let root = std::env::temp_dir()
        .join("robokura-window-checks")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);

    let mut core = Core::open_at(root.clone()).expect("the store opened");
    let ids = fill(&mut core, &root);

    let (window, view) = cx.update(|cx| {
        let store = cx.new(|_| core);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Default::default(),
                    size: size(px(1400.), px(800.)),
                })),
                ..Default::default()
            },
            cx,
            move |window, cx| {
                let list = cx.new(|cx| AssistantPane::new(Some(store.clone()), None, window, cx));
                cx.new(|cx| AppView::new(Some(store), None, list, window, cx))
            },
        )
        .expect("the window opened")
    });

    (window, view, ids)
}

/// A message written straight into an assistant's thread, leaving the store open for the
/// window to read. Sending would start an agent wherever one is installed.
fn say_in(root: &std::path::Path, assistant: &str, at: i64, text: &str) {
    let store = Store::open(&paths::db_file(root)).expect("the store opened again");
    let thread = store
        .thread_for(assistant)
        .expect("read")
        .expect("the assistant has a thread");
    let seq = store.next_seq(&thread.id).expect("read");
    let mut message = Message::person(&thread.id, seq, text);
    message.created_at = at;
    message.updated_at = at;
    store.add_message(&message).expect("stored");
}

fn settle(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| {
        for _ in 0..4 {
            window.render_frame(cx);
        }
    })
    .expect("the window is open");
}

fn open_assistant(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, id: &str) {
    settle(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.click(format!("assistant-{id}"), cx);
    })
    .expect("the window is open");
    settle(cx, window);
}

/// `details-toggle` is a wrapper, and a lookup matches only the last element on a
/// path, so the wrapper is a scope to search within rather than something to find.
/// The button beneath it answers to `collapse`.
fn press_toggle(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| {
        window.within("details-toggle").click("collapse", cx);
    })
    .expect("the window is open");
    settle(cx, window);
}

fn press_new_assistant(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle) {
    settle(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.click("new-assistant", cx);
    })
    .expect("the window is open");
    settle(cx, window);
}

fn search_for(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    view: &Entity<AppView>,
    query: &str,
) {
    settle(cx, window);
    cx.update_window(window, |_, window, cx| {
        let list = view.read(cx).list().clone();
        list.update(cx, |list, cx| list.search_for(query, window, cx));
    })
    .expect("the window is open");
    settle(cx, window);
}

/// What is in the store, read back from the file rather than from the window, so
/// a check cannot pass on a list that drew something the database never got.
fn stored(name: &str) -> Vec<robokura_core::Assistant> {
    Core::open_at(root_of(name))
        .expect("the store opened again")
        .assistants()
        .expect("the assistants read")
}

fn root_of(name: &str) -> std::path::PathBuf {
    std::env::temp_dir()
        .join("robokura-window-checks")
        .join(name)
}

fn count_stored(name: &str) -> usize {
    Core::open_at(root_of(name))
        .expect("the store opened again")
        .assistants()
        .expect("the assistants read")
        .len()
}

/// The bar's control for the pane, looked up by the button's own id.
fn panes_control_is_drawn(window: &gpui_kit::Window) -> bool {
    window.try_find("collapse").is_some()
}

fn right_edge(window: &gpui_kit::Window, id: &'static str) -> gpui_kit::Pixels {
    let found = window
        .try_find(id)
        .unwrap_or_else(|| panic!("{id} is on screen"));
    found.bounds().origin.x + found.bounds().size.width
}

fn left_edge(window: &gpui_kit::Window, id: &'static str) -> gpui_kit::Pixels {
    window
        .try_find(id)
        .unwrap_or_else(|| panic!("{id} is on screen"))
        .bounds()
        .origin
        .x
}

fn window_right(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle) -> gpui_kit::Pixels {
    cx.update_window(window, |_, window, _| window.viewport_size().width)
        .expect("the window is open")
}

/// Opens the pane and confirms it is there.
fn open_pane(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, ids: &[String]) {
    open_assistant(cx, window, &ids[0]);
    press_toggle(cx, window);
}

#[gpui_kit::test]
fn one_press_makes_an_assistant_and_opens_it(cx: &mut TestAppContext) {
    let (window, view, _) = window_with(cx, "presses-once", &[]);

    press_new_assistant(cx, window);

    let kept = stored("presses-once");
    assert_eq!(kept.len(), 1, "one press made one assistant");
    assert_eq!(
        (
            kept[0].name.as_str(),
            kept[0].title.as_str(),
            kept[0].description.as_str()
        ),
        (
            robokura_core::NEW_ASSISTANT_NAME,
            robokura_core::NEW_ASSISTANT_TITLE,
            robokura_core::NEW_ASSISTANT_DESCRIPTION
        ),
        "and it was made with the words a new one starts with, with nothing asked for first"
    );

    cx.read(|cx| {
        assert_eq!(
            view.read(cx).list().read(cx).open_id().as_deref(),
            Some(kept[0].id.as_str()),
            "the assistant just made is the one the window is about"
        );
        assert!(
            view.read(cx).thread().is_some(),
            "and the middle is that assistant's thread. Making one and having to go and find it \
             afterwards is two steps where the product says one."
        );
    });
}

#[gpui_kit::test]
fn each_press_makes_its_own_assistant_and_the_last_one_is_open(cx: &mut TestAppContext) {
    let (window, view, _) = window_with(cx, "presses-twice", &[]);

    press_new_assistant(cx, window);
    press_new_assistant(cx, window);

    let kept = stored("presses-twice");
    assert_eq!(kept.len(), 2, "two presses made two assistants");

    cx.read(|cx| {
        assert_eq!(
            view.read(cx).list().read(cx).open_id().as_deref(),
            Some(kept[0].id.as_str()),
            "and the window is about the one just made, not the one before it. The list is \
             newest first, so the one just made is the one at the top."
        );
    });
}

#[gpui_kit::test]
fn opening_a_window_that_has_assistants_in_it_opens_the_first_one(cx: &mut TestAppContext) {
    // The second one is the one somebody was last speaking in, so it is the first row.
    let (window, view, ids) = window_over(cx, "opens-the-first", |core, root| {
        let first = core
            .create_assistant("Letters", "drafts", "")
            .expect("the assistant was made");
        let second = core
            .create_assistant("Research", "finds sources", "")
            .expect("the second assistant was made");

        say_in(root, &second.id, 300, "asked just now");
        say_in(root, &first.id, 100, "asked a while ago");

        vec![first.id, second.id]
    });

    settle(cx, window);

    cx.read(|cx| {
        assert_eq!(
            view.read(cx).list().read(cx).open_id().as_deref(),
            Some(ids[1].as_str()),
            "the window opens onto the first row, which is the thread that was last spoken in. \
             Somebody reopening the window is coming back to a conversation, and opening on \
             nothing makes them go and find which one it was."
        );
    });
    cx.update_window(window, |_, window, _| {
        assert!(
            window.try_find("composer-send").is_some(),
            "and that assistant's thread is in the middle rather than the empty middle. The row \
             marked open beside a middle that is not its thread would be the two disagreeing."
        );
        assert!(
            panes_control_is_drawn(window),
            "so the bar offers the control for that assistant's own pane. An open assistant with \
             no way to reach its pane is a control that is not drawn for one that is open."
        );
    })
    .expect("the window is open");
}

#[gpui_kit::test]
fn opening_a_window_with_nothing_in_it_opens_on_nothing(cx: &mut TestAppContext) {
    let (window, view, _) = window_with(cx, "opens-on-nothing", &[]);

    settle(cx, window);

    cx.read(|cx| {
        assert!(
            view.read(cx).list().read(cx).open_id().is_none(),
            "there is no assistant to open on, so no row is marked open. Marking one would leave \
             the list claiming the window is about something the middle is not showing."
        );
        assert!(
            view.read(cx).thread().is_none(),
            "and the middle is the empty one that asks for an assistant to be made"
        );
    });
}

#[gpui_kit::test]
fn making_an_assistant_does_not_open_its_own_pane(cx: &mut TestAppContext) {
    let (window, view, _) = window_with(cx, "press-keeps-pane-closed", &[]);

    press_new_assistant(cx, window);

    cx.read(|cx| {
        assert!(
            !view.read(cx).details().is_some(),
            "the one control does not open the assistant's own pane. The pane is somewhere they \
             go when they want to change something, and one press that both makes an assistant \
             and takes the width for its fields is doing two things."
        );
        assert!(
            view.read(cx).details().is_none(),
            "and nothing was built for it, so it costs nothing until it is asked for"
        );
    });
}

#[gpui_kit::test]
fn the_search_matches_names_and_leaves_the_open_assistant_alone(cx: &mut TestAppContext) {
    let (window, view, ids) = window_with(
        cx,
        "search-names-only",
        &[
            ("Letters", "drafts my letters", ""),
            ("Recipes", "rewrites in my voice", ""),
        ],
    );

    open_assistant(cx, window, &ids[1]);

    // "rewrites" is Recipes' title and neither assistant's name. Matching titles
    // too would make the search a second way to find an assistant.
    search_for(cx, window, &view, "rewrites");
    cx.update_window(window, |_, window, _| {
        assert!(
            window.try_find(format!("assistant-{}", ids[1])).is_none(),
            "a search on a title leaves no row standing, because the search is on names"
        );
        assert!(
            window.try_find(format!("assistant-{}", ids[0])).is_none(),
            "so an assistant whose title happens not to match is hidden with the rest"
        );
    })
    .expect("the window is open");

    search_for(cx, window, &view, "Let");
    cx.update_window(window, |_, window, _| {
        assert!(
            window.try_find(format!("assistant-{}", ids[0])).is_some(),
            "a search on a name leaves that row standing"
        );
        assert!(
            window.try_find(format!("assistant-{}", ids[1])).is_none(),
            "and hides the one whose name does not match"
        );
    })
    .expect("the window is open");

    cx.read(|cx| {
        assert_eq!(
            view.read(cx).list().read(cx).open_id().as_deref(),
            Some(ids[1].as_str()),
            "and the assistant the window is about has not moved. A search narrows what the \
             list shows; it is not a request to open something else."
        );
        assert!(
            view.read(cx).thread().is_some(),
            "so the conversation is still there. A row a search hides must not take the thread \
             with it."
        );
    });
}

#[gpui_kit::test]
fn a_list_with_no_rows_draws_nothing_and_offers_nothing(cx: &mut TestAppContext) {
    let (searched, view, ids) = window_with(cx, "no-rows-by-search", &[("Letters", "drafts", "")]);
    open_assistant(cx, searched, &ids[0]);
    search_for(cx, searched, &view, "nothing here matches this");

    let (empty, _, _) = window_with(cx, "no-rows-no-assistants", &[]);

    for (window, why, assistant) in [
        (searched, "a search that matched no name", Some(&ids[0])),
        (empty, "no assistants at all", None),
    ] {
        cx.update_window(window, |_, window, _| {
            assert!(
                window.try_find("list-blank").is_some(),
                "the list draws its blank area and nothing else when {why}. A panel explaining \
                 the empty list is a thing to read before there is a first assistant and again \
                 after every search that comes back with nothing."
            );
            if let Some(assistant) = assistant {
                assert!(
                    window.try_find(format!("assistant-{assistant}")).is_none(),
                    "and no row is drawn for that assistant when {why}"
                );
            }
            assert!(
                window.try_find("new-assistant").is_some(),
                "while the bar's +, the one control that makes an assistant, is there when {why}"
            );
        })
        .expect("the window is open");
    }

    let (with_rows, _, _) = window_with(cx, "rows-not-blank", &[("Letters", "drafts", "")]);
    cx.update_window(with_rows, |_, window, _| {
        assert!(
            window.try_find("list-blank").is_none(),
            "the blank area is not drawn over the rows. An empty state drawn on top of a list \
             that has something in it is two answers at once."
        );
    })
    .expect("the window is open");

    cx.read(|cx| {
        assert_eq!(
            view.read(cx).list().read(cx).open_id().as_deref(),
            Some(ids[0].as_str()),
            "the open assistant is untouched by a search that found nothing"
        );
        assert!(
            view.read(cx).thread().is_some(),
            "and its thread is still being shown"
        );
    });
}

#[gpui_kit::test]
fn the_bar_offers_the_panes_control_only_when_an_assistant_is_open(cx: &mut TestAppContext) {
    let (window, _view, _) = window_with(cx, "toggle-needs-an-assistant", &[]);

    settle(cx, window);
    cx.update_window(window, |_, window, _| {
        assert!(
            !panes_control_is_drawn(window),
            "with no assistant open there is no pane to show, so the control showing one is not \
             drawn. A button that answers a press and then shows nothing says the pane is there \
             and it is not."
        );
    })
    .expect("the window is open");

    press_new_assistant(cx, window);
    cx.update_window(window, |_, window, _| {
        assert!(
            panes_control_is_drawn(window),
            "and it is there once one is open, because now there is a pane to open"
        );
    })
    .expect("the window is open");
}

#[gpui_kit::test]
fn deleting_the_open_assistant_takes_the_panes_control_with_it(cx: &mut TestAppContext) {
    let (window, view, ids) = window_with(cx, "toggle-after-delete", &[("Letters", "drafts", "")]);

    open_pane(cx, window, &ids);
    cx.update_window(window, |_, window, _| {
        assert!(
            panes_control_is_drawn(window),
            "the control is drawn while the pane is, which is the state this starts from"
        );
    })
    .expect("the window is open");

    answer_the_delete(cx, window, "ok");

    cx.read(|cx| {
        assert!(
            view.read(cx).list().read(cx).open_id().is_none(),
            "the assistant is gone, so nothing is open"
        );
    });
    cx.update_window(window, |_, window, _| {
        assert!(
            !panes_control_is_drawn(window),
            "and the control that would open its pane is gone with it, so no press is offered \
             with nothing behind it"
        );
    })
    .expect("the window is open");
}

/// Presses delete, waits for the question, and answers it.
fn answer_the_delete(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    answer: &'static str,
) {
    cx.update_window(window, |_, window, cx| {
        window.click("remove-assistant", cx);
    })
    .expect("the window is open");
    cx.update_window(window, |_, window, cx| {
        for _ in 0..4 {
            window.render_frame(cx);
        }
        window.click(answer, cx);
    })
    .expect("the window is open");
    settle(cx, window);
}

#[gpui_kit::test]
fn the_pane_reaches_the_windows_edge_and_the_buttons_lie_on_it(cx: &mut TestAppContext) {
    let (window, _view, ids) = window_with(cx, "pane-at-the-edge", &[("Letters", "drafts", "")]);
    open_pane(cx, window, &ids);

    let right = window_right(cx, window);

    cx.update_window(window, |_, window, _| {
        let title = window
            .try_find("details-title")
            .expect("the pane's title is drawn");

        // The delete control spans the pane's own column, so its far edge plus the
        // right inset is the pane's far edge.
        assert_eq!(
            right_edge(window, "remove-assistant") + layout::DETAILS_INSET_RIGHT,
            right,
            "the pane runs to the window's edge, so the window's own buttons lie on the pane \
             instead of beside it. Inset from the edge they sit in a strip of the conversation \
             and read as belonging to something else."
        );

        assert!(
            title.bounds().origin.y + title.bounds().size.height * 0.5 < layout::BAR_HEIGHT,
            "and the pane's colour is at the top of the window, under the bar, which is only \
             true if the pane itself reaches there"
        );
    })
    .expect("the window is open");
}

#[gpui_kit::test]
fn the_pane_uses_its_whole_width(cx: &mut TestAppContext) {
    let (window, _view, ids) = window_with(cx, "pane-full-width", &[("Letters", "drafts", "")]);
    open_pane(cx, window, &ids);

    let right = window_right(cx, window);
    let span = layout::DETAILS_WIDTH - layout::DETAILS_INSET_LEFT - layout::DETAILS_INSET_RIGHT;

    cx.update_window(window, |_, window, _| {
        assert_eq!(
            right_edge(window, "remove-assistant") - left_edge(window, "remove-assistant"),
            span,
            "the pane's content runs the pane's whole width. Held back by the width the \
             window's own buttons hold, everything under them was blank space and the pane \
             looked half empty."
        );
        assert_eq!(
            right_edge(window, "remove-assistant") + layout::DETAILS_INSET_RIGHT,
            right,
            "and it finishes where the pane finishes. The window's buttons are drawn over the \
             pane's far corner, but they are in the bar's strip and this content starts below \
             it."
        );
    })
    .expect("the window is open");
}

#[gpui_kit::test]
fn the_bar_s_control_for_the_pane_is_never_inside_it(cx: &mut TestAppContext) {
    let (window, _view, ids) =
        window_with(cx, "control-outside-pane", &[("Letters", "drafts", "")]);
    open_assistant(cx, window, &ids[0]);

    let right = window_right(cx, window);
    let pane_left = right - layout::DETAILS_WIDTH;

    press_toggle(cx, window);

    cx.update_window(window, |_, window, _| {
        assert_eq!(
            right_edge(window, "collapse"),
            pane_left - layout::PANE_CONTROL_GAP,
            "the control's far edge stops short of the pane's near edge, so it stands outside \
             the pane with a gap between them. Inside, it sits on the pane's own colour looking \
             like one of its fields."
        );
        assert!(
            right_edge(window, "collapse") <= left_edge(window, "remove-assistant"),
            "and nothing of the pane's own content begins before it, which is the other half of \
             the same claim"
        );
    })
    .expect("the window is open");

    press_toggle(cx, window);
    let bar_right = cx
        .update_window(window, |_, window, _| {
            right_edge(window, "collapse") + layout::window_buttons()
        })
        .expect("the window is open");
    assert_eq!(
        bar_right,
        right - layout::PANE_CONTROL_GAP,
        "with the pane closed the control is the last thing in the bar, a gap short of the \
         window's own buttons rather than hard against them"
    );
}

#[gpui_kit::test]
fn the_panes_title_is_in_the_bar_and_centred_over_the_pane(cx: &mut TestAppContext) {
    let (window, _view, ids) = window_with(cx, "title-in-bar", &[("Letters", "drafts", "")]);
    open_pane(cx, window, &ids);

    cx.update_window(window, |_, window, _| {
        let title = window
            .try_find("details-title")
            .expect("the pane's title is drawn");
        let bounds = title.bounds();

        assert!(
            bounds.origin.y + bounds.size.height * 0.5 < layout::BAR_HEIGHT,
            "the title is said in the bar, not in a header further down the pane. The pane \
             reaches the bar, so a title inside it would be a heading with the window's own \
             title bar sitting directly above it."
        );

        // Centred on the pane's content column, which is not quite the pane's own
        // middle: the insets are 16 on one side and 10 on the other.
        let content_left = left_edge(window, "remove-assistant");
        let content_right = right_edge(window, "remove-assistant");
        let title_middle = bounds.origin.x + bounds.size.width * 0.5;
        let content_middle = content_left + (content_right - content_left) * 0.5;
        assert!(
            (title_middle - content_middle).abs() < px(1.),
            "and on the same line as the rest of the pane's content. The title, the portrait, \
             the fields and the delete control share one centre line."
        );
    })
    .expect("the window is open");
}

#[gpui_kit::test]
fn the_pane_has_no_controls_of_its_own(cx: &mut TestAppContext) {
    let (window, _view, ids) = window_with(cx, "no-pane-controls", &[("Letters", "drafts", "")]);
    open_pane(cx, window, &ids);

    for gone in ["details-back", "details-close"] {
        cx.update_window(window, |_, window, _| {
            assert!(
                window.try_find(gone).is_none(),
                "{gone} is not drawn. The pane reaches the bar, the bar names it, and the \
                 control on that bar is what opens and closes it, so a second control inside the \
                 pane is the same action in two places."
            );
        })
        .expect("the window is open");
    }
}

#[gpui_kit::test]
fn the_delete_control_spans_the_pane_and_is_not_a_footnote(cx: &mut TestAppContext) {
    let (window, _view, ids) = window_with(cx, "delete-spans", &[("Letters", "drafts", "")]);
    open_pane(cx, window, &ids);

    cx.update_window(window, |_, window, _| {
        let button = window
            .try_find("remove-assistant")
            .expect("the delete control is drawn");
        let bounds = button.bounds();
        let span = layout::DETAILS_WIDTH - layout::DETAILS_INSET_LEFT - layout::DETAILS_INSET_RIGHT;

        assert_eq!(
            bounds.size.width, span,
            "the control is as wide as the pane's own column. Sized to its own label it sits in \
             a corner under three fields, and reads as a footnote beside the one thing on this \
             pane that cannot be taken back."
        );
        assert!(
            bounds.size.height > px(24.),
            "and it is not the small size. Height was {:?}.",
            bounds.size.height
        );
    })
    .expect("the window is open");

    cx.update_window(window, |_, window, cx| {
        window.click("remove-assistant", cx);
    })
    .expect("the window is open");
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.has_active_dialog(cx),
            "widening it did not turn it into something that skips the question"
        );
    })
    .expect("the window is open");
}

#[gpui_kit::test]
fn the_pane_starts_closed(cx: &mut TestAppContext) {
    let (window, view, ids) = window_with(cx, "starts-closed", &[("Letters", "drafts", "")]);
    open_assistant(cx, window, &ids[0]);

    cx.read(|cx| {
        assert!(
            !view.read(cx).details().is_some(),
            "opening an assistant does not open its own pane. The thread is what a person opens \
             the window for, so it takes no width until it is asked for."
        );
        assert!(
            view.read(cx).details().is_none(),
            "and nothing was built for it, so a closed pane costs nothing"
        );
    });
}

#[gpui_kit::test]
fn the_toggle_in_the_bar_opens_the_pane_and_closes_it_again(cx: &mut TestAppContext) {
    let (window, view, ids) = window_with(cx, "toggles", &[("Letters", "drafts", "")]);
    open_assistant(cx, window, &ids[0]);

    press_toggle(cx, window);
    cx.read(|cx| {
        assert!(
            view.read(cx).details().is_some(),
            "the toggle in the bar opens the open assistant's own pane"
        );
        assert!(
            view.read(cx).details().is_some(),
            "and it is built when it is opened"
        );
    });

    press_toggle(cx, window);
    cx.read(|cx| {
        assert!(
            !view.read(cx).details().is_some(),
            "the same control closes it, so it is a toggle rather than a latch"
        );
        assert!(
            view.read(cx).details().is_none(),
            "and a closed pane holds no fields, so reopening it reads what is stored"
        );
    });
}

#[gpui_kit::test]
fn the_pane_is_about_the_assistant_that_is_open(cx: &mut TestAppContext) {
    let (window, view, ids) = window_with(
        cx,
        "follows-the-open-assistant",
        &[("Letters", "drafts", ""), ("Recipes", "rewrites", "")],
    );

    open_pane(cx, window, &ids);
    cx.read(|cx| {
        let pane = view.read(cx).details().expect("the pane is open");
        assert_eq!(pane.read(cx).assistant_name(), "Letters");
    });

    // One pane is about one assistant, and a pane left describing the last one
    // would write a rename onto the wrong record. It stays open, because it was
    // asked for.
    open_assistant(cx, window, &ids[1]);
    cx.read(|cx| {
        let pane = view.read(cx).details().expect("the pane is open");
        assert_eq!(
            pane.read(cx).assistant_name(),
            "Recipes",
            "and it follows the assistant when another one is opened"
        );
    });
}

#[gpui_kit::test]
fn editing_an_assistant_saves_what_was_typed(cx: &mut TestAppContext) {
    let (window, view, ids) = window_with(
        cx,
        "saves-an-edit",
        &[("Letters", "drafts my letters", "writes in my voice")],
    );
    open_pane(cx, window, &ids);

    // The description is left alone, so this also covers a field that was not
    // touched keeping what it had.
    cx.update_window(window, |_, window, cx| {
        let pane = view.read(cx).details().expect("the pane is open");
        let (name, title) = (pane.read(cx).name_field(), pane.read(cx).title_field());
        name.update(cx, |input, cx| {
            input.set_value("Correspondence", window, cx);
        });
        title.update(cx, |input, cx| {
            input.set_value("writes letters that sound like me", window, cx);
        });
    })
    .expect("the window is open");
    settle(cx, window);

    // The Save control only appears once something has changed, so its being
    // there is itself part of what the pane is doing.
    cx.update_window(window, |_, window, cx| {
        window.click("save-assistant", cx);
    })
    .expect("the window is open");
    settle(cx, window);

    let stored = stored("saves-an-edit").remove(0);
    assert_eq!(
        (
            stored.name.as_str(),
            stored.title.as_str(),
            stored.description.as_str()
        ),
        (
            "Correspondence",
            "writes letters that sound like me",
            "writes in my voice"
        ),
        "what was typed is what is stored, and a field that was not touched keeps what it had"
    );
}

#[gpui_kit::test]
fn every_field_can_be_changed(cx: &mut TestAppContext) {
    let (window, view, ids) = window_with(cx, "every-field", &[("Letters", "", "was never told")]);
    open_pane(cx, window, &ids);

    cx.update_window(window, |_, window, cx| {
        let pane = view.read(cx).details().expect("the pane is open");
        let name = pane.read(cx).name_field();
        let title = pane.read(cx).title_field();
        let description = pane.read(cx).description_field();
        name.update(cx, |input, cx| {
            input.set_value("Correspondence", window, cx);
        });
        title.update(cx, |input, cx| {
            input.set_value("writes letters", window, cx);
        });
        description.update(cx, |input, cx| {
            input.set_value("writes in my voice, then stops", window, cx);
        });
    })
    .expect("the window is open");
    settle(cx, window);

    cx.update_window(window, |_, window, cx| {
        window.click("save-assistant", cx);
    })
    .expect("the window is open");
    settle(cx, window);

    let stored = stored("every-field").remove(0);
    assert_eq!(
        (
            stored.name.as_str(),
            stored.title.as_str(),
            stored.description.as_str()
        ),
        (
            "Correspondence",
            "writes letters",
            "writes in my voice, then stops"
        ),
        "all three fields are written from what is in them"
    );
}

#[gpui_kit::test]
fn a_name_cannot_be_emptied(cx: &mut TestAppContext) {
    let (window, view, ids) = window_with(cx, "no-empty-name", &[("Letters", "drafts", "")]);
    open_pane(cx, window, &ids);

    cx.update_window(window, |_, window, cx| {
        let pane = view.read(cx).details().expect("the pane is open");
        pane.read(cx).name_field().update(cx, |input, cx| {
            input.set_value("   ", window, cx);
        });
    })
    .expect("the window is open");
    settle(cx, window);

    cx.update_window(window, |_, window, cx| {
        window.click("save-assistant", cx);
    })
    .expect("the window is open");
    settle(cx, window);

    assert_eq!(
        stored("no-empty-name").remove(0).name,
        "Letters",
        "an assistant cannot be left with no name. The list, the bar and the pane all name it, \
         and an unnamed row cannot be told from any other."
    );
}

#[gpui_kit::test]
fn a_pane_that_was_opened_and_not_touched_offers_nothing_to_save(cx: &mut TestAppContext) {
    let (window, view, ids) = window_with(cx, "untouched", &[("Letters", "drafts", "")]);
    open_pane(cx, window, &ids);

    cx.read(|cx| {
        let pane = view.read(cx).details().expect("the pane is open");
        assert!(
            !pane.read(cx).has_changes(cx),
            "an untouched pane has nothing to save, so it offers nothing to press"
        );
    });
}

#[gpui_kit::test]
fn pressing_delete_asks_before_it_removes_anything(cx: &mut TestAppContext) {
    let (window, _view, ids) = window_with(cx, "asks-first", &[("Letters", "drafts", "")]);
    open_pane(cx, window, &ids);

    cx.update_window(window, |_, window, cx| {
        window.click("remove-assistant", cx);
    })
    .expect("the window is open");

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.has_active_dialog(cx),
            "deleting an assistant asks first, because it cannot be undone"
        );
    })
    .expect("the window is open");

    assert_eq!(
        count_stored("asks-first"),
        1,
        "asking the question has not removed anything yet. Answering it is what removes."
    );
}

#[gpui_kit::test]
fn answering_the_question_removes_the_assistant_its_thread_and_its_messages(
    cx: &mut TestAppContext,
) {
    let (window, view, ids) = window_with(cx, "deletes-everything", &[("Letters", "drafts", "")]);
    open_pane(cx, window, &ids);

    answer_the_delete(cx, window, "ok");

    cx.read(|cx| {
        assert!(
            !view.read(cx).details().is_some(),
            "the pane closes with the assistant, because there is nothing left for it"
        );
        assert!(
            !view.read(cx).thread().is_some(),
            "and the window stops showing a thread for something that is not there"
        );
    });

    assert_eq!(
        count_stored("deletes-everything"),
        0,
        "and the assistant is gone from the list on the next run too"
    );
}

#[gpui_kit::test]
fn answering_no_leaves_the_assistant_exactly_where_it_was(cx: &mut TestAppContext) {
    let (window, _view, ids) = window_with(cx, "keeps-it", &[("Letters", "drafts", "")]);
    open_pane(cx, window, &ids);

    answer_the_delete(cx, window, "cancel");

    assert_eq!(
        count_stored("keeps-it"),
        1,
        "answering no removes nothing. A question that can only be answered yes is not a question."
    );
}

#[gpui_kit::test]
fn pressing_settings_puts_it_where_the_conversation_was(cx: &mut TestAppContext) {
    let (window, view, ids) = window_with(
        cx,
        "settings-takes-the-middle",
        &[("Letters", "drafts", "")],
    );
    open_assistant(cx, window, &ids[0]);

    settle(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.click("settings-item", cx);
    })
    .expect("the window is open");
    settle(cx, window);

    cx.update_window(window, |_, window, _| {
        assert!(
            window.try_find("settings-pane").is_some(),
            "the settings are drawn"
        );
        assert!(
            window.try_find("composer-send").is_none(),
            "and the conversation is not. The settings take the place of the middle rather than \
             being a layer over it or a fourth column, so the list and the bar do not move."
        );
        assert!(
            !panes_control_is_drawn(window),
            "and the bar's control for an assistant's own pane goes with it. There is no open \
             assistant, so there is no pane to show and nothing to press."
        );
    })
    .expect("the window is open");

    cx.read(|cx| {
        assert_eq!(
            view.read(cx).list().read(cx).open_id(),
            None,
            "and no assistant is open. A row still marked as open beside a middle that is not \
             its thread says the two are connected, and they are not."
        );
        assert!(
            !view.read(cx).thread().is_some(),
            "so the window is not showing a conversation for anything"
        );
    });
}

#[gpui_kit::test]
fn opening_an_assistant_puts_its_thread_back(cx: &mut TestAppContext) {
    let (window, view, ids) = window_with(
        cx,
        "settings-then-an-assistant",
        &[("Letters", "drafts", "")],
    );

    settle(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.click("settings-item", cx);
    })
    .expect("the window is open");
    settle(cx, window);

    open_assistant(cx, window, &ids[0]);

    cx.update_window(window, |_, window, _| {
        assert!(
            window.try_find("settings-pane").is_none(),
            "opening an assistant takes the settings away. The two are never both in the \
             middle: one is about this machine and one is about one assistant."
        );
        assert!(
            window.try_find("composer-send").is_some(),
            "and that assistant's thread is there"
        );
    })
    .expect("the window is open");

    cx.read(|cx| {
        assert_eq!(
            view.read(cx).list().read(cx).open_id().as_deref(),
            Some(ids[0].as_str()),
            "and its row is marked as open again, because the middle is its thread again"
        );
    });
}

/// The model list is the agent's own, so this needs an agent and starts one. It is
/// not ignored because it does not go near a conversation and needs nobody signed
/// in: it asks a program what it can be set to, which is the one question this pane
/// cannot answer for itself. On a machine with no agent it passes trivially, since
/// there is nothing to show either way.
#[gpui_kit::test]
fn opening_the_settings_asks_the_program_and_shows_what_it_said(cx: &mut TestAppContext) {
    let (window, view, _) = window_with(cx, "settings-asks", &[]);
    let core = cx.read(|cx| view.read(cx).core().expect("the store opened"));

    settle(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.click("settings-item", cx);
    })
    .expect("the window is open");
    settle(cx, window);

    // The window's own reading loop is what collects the answer, so this is what it
    // does: read the store, draw.
    let mut was_asked = false;
    let mut answered: Vec<(String, usize)> = Vec::new();

    for _ in 0..240 {
        settle(cx, window);
        cx.update_window(window, |_, _, cx| {
            core.update(cx, |core, _| core.pump());
        })
        .expect("the window is open");

        let (probing, offered) = cx.read(|cx| {
            let core = core.read(cx);
            (
                core.probing(),
                core.settings()
                    .expect("read")
                    .offered
                    .into_iter()
                    .map(|(command, options)| (command, options.len()))
                    .collect::<Vec<_>>(),
            )
        });
        was_asked |= probing;
        if !offered.is_empty() {
            answered = offered;
            // One more frame, which is what the window's own tick gives the pane
            // after the answer lands.
            settle(cx, window);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    let still_probing = cx.read(|cx| core.read(cx).probing());
    let shown = cx.read(|cx| {
        view.read(cx)
            .settings()
            .expect("the pane was built")
            .read(cx)
            .choices(cx)
    });

    assert!(
        was_asked,
        "opening the settings asks the program what it can be set to, because that list is the \
         program's own and there is no other way to have it"
    );
    assert!(
        !answered.is_empty(),
        "and the answer lands in the store for the pane to draw. It did not, on a machine with \
         {} installed.",
        robokura_acp::agents::first().unwrap_or_default()
    );
    assert!(
        !shown.is_empty(),
        "and the pane draws what the store holds, rather than a copy of the settings from before \
         the answer landed"
    );
    for (name, values, current) in &shown {
        assert!(
            *values > 0,
            "and every row it draws is a chooser with something in it. {name:?} was drawn with no \
             values at all, which on screen is a menu that opens onto nothing. Shown: {shown:?}"
        );
        assert!(
            !current.is_empty(),
            "and every one of them shows the value it is set to. {name:?} was drawn with no \
             value in force. Shown: {shown:?}"
        );
    }
    assert!(
        !still_probing,
        "and the program is let go once it has answered, rather than held open for a pane that \
         has stopped needing it"
    );
}

/// Opens the settings in a window that is dark to start with, so pressing a switch on the
/// appearance group has somewhere to go.
fn open_the_settings_over(cx: &mut TestAppContext, name: &str) -> gpui_kit::AnyWindowHandle {
    let (window, _view, _) = window_with(cx, name, &[]);

    settle(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.click("settings-item", cx);
    })
    .expect("the window is open");
    settle(cx, window);

    window
}

/// The appearance group is the second one on the page, and its item is the only control in
/// it, so the switch is reached through the group's own identity rather than by name.
fn the_dark_mode_switch(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| {
        window.within("group-1").click("check", cx);
    })
    .expect("the window is open");
    settle(cx, window);
}

#[gpui_kit::test]
fn the_dark_mode_switch_changes_the_window_and_is_remembered(cx: &mut TestAppContext) {
    let window = open_the_settings_over(cx, "settings-dark-mode");

    let before = cx.read(|cx| cx.theme().mode);

    the_dark_mode_switch(cx, window);

    let after = cx.read(|cx| cx.theme().mode);
    assert_ne!(
        before, after,
        "pressing the switch repaints the window in the other mode. A switch that moves \
         without repainting is a switch lying about what is on screen."
    );

    let read = Core::open_at(root_of("settings-dark-mode"))
        .expect("the store opened again")
        .settings()
        .expect("read");
    assert_eq!(
        read.mode,
        after.name(),
        "and the mode now on screen is the one written, so the next run opens in the mode \
         that was asked for rather than the machine's answer again"
    );
}

/// The two groups are on one page rather than two, so a person opening the settings is
/// looking at the whole of what this machine has been told rather than at one part of it.
#[gpui_kit::test]
fn the_agent_and_the_appearance_are_both_on_the_general_page(cx: &mut TestAppContext) {
    let window = open_the_settings_over(cx, "settings-one-page");

    // A group is not looked up by its own name: a lookup matches the last element on a path
    // and a group's items are below it. Both groups are found by what is inside them.
    cx.update_window(window, |_, window, _| {
        assert!(
            window.within("group-0").try_find("btn").is_some(),
            "the agent chooser is on the page that is open"
        );
        assert!(
            window.within("group-1").try_find("check").is_some(),
            "and the appearance switch is in a second group on that same page rather than in a \
             page of its own, so the settings are one place rather than two"
        );
    })
    .expect("the window is open");
}

#[gpui_kit::test]
fn a_setting_is_written_the_moment_it_is_changed(cx: &mut TestAppContext) {
    let (window, view, _) = window_with(cx, "settings-write-once", &[]);

    settle(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.click("settings-item", cx);
    })
    .expect("the window is open");
    settle(cx, window);

    let core = cx.read(|cx| view.read(cx).core().expect("the store opened"));

    cx.update_window(window, |_, window, cx| {
        core.update(cx, |core, _| {
            let mut settings = core.settings().expect("read");
            settings.runs_on = "chosen-here".to_string();
            core.save_settings(&settings).expect("written");
        });
        for _ in 0..4 {
            window.render_frame(cx);
        }
    })
    .expect("the window is open");
    settle(cx, window);

    let read = Core::open_at(root_of("settings-write-once"))
        .expect("the store opened again")
        .settings()
        .expect("read");

    assert_eq!(
        read.runs_on, "chosen-here",
        "and it is still there after the window is closed, which is what makes it a setting \
         rather than something the pane was showing"
    );
}
