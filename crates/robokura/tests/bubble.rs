//! What a bubble can hold, and what it does with it.
//!
//! The two sides of a thread need opposite text colours, and markdown takes its colour
//! from the theme whatever bubble it sits on, so the side that needs the other colour
//! has to be drawn some other way. That side is plain text, and whether that is
//! acceptable turns on whether a bubble keeps the line breaks a person pasted in.
//!
//! It is the component set that decides this rather than this product: the style handed
//! to a text view covers spacing, headings and code surfaces, and has nothing for the
//! colour the words themselves are painted in. A text view inside a bubble whose text
//! colour is not the body colour would need one, and reporting that is more use than
//! working around it.

use gpui_kit::{
    AppContext as _, Bounds, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, Styled as _, TestAppContext, Window, WindowBounds, WindowOptions,
    component::{ThemeMode, bubble::Bubble},
    px, relative, size,
    test::{TestSupportExt as _, TestWindowExt},
};

/// One bubble, on a row of its own that can be measured.
struct OneBubble {
    body: String,
}

impl gpui_kit::Render for OneBubble {
    fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        let body = SharedString::from(self.body.clone());

        gpui_kit::div().id("row").test_support().p_6().child(
            Bubble::new()
                .max_w(relative(0.72))
                .child(body.into_any_element()),
        )
    }
}

fn measure(cx: &mut TestAppContext, body: &str) -> f32 {
    let (window, _) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Default::default(),
                    size: size(px(900.), px(600.)),
                })),
                ..Default::default()
            },
            cx,
            move |_window, cx| {
                let body = body.to_string();
                cx.new(|_cx| OneBubble { body })
            },
        )
        .expect("the window opened")
    });
    cx.update_window(window, |_, window, cx| {
        for _ in 0..4 {
            window.render_frame(cx);
        }
        window.find("row").bounds().size.height
    })
    .expect("the window is open")
    .into()
}

#[gpui_kit::test]
fn plain_text_in_a_bubble_keeps_the_line_breaks_it_was_given(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| robokura::theme::apply(cx, ThemeMode::Dark));

    let one = measure(cx, "one line of text");
    let two = measure(cx, "one line of text\nand a second line");
    println!("plain: one line {one:.1}, two lines {two:.1}");

    assert!(
        two > one + 10.,
        "two lines of plain text in a bubble are {one:.1} and {two:.1} points tall, \
         so the line break is being dropped rather than shown. The side of the thread \
         that needs the other text colour is drawn as plain text, and a multi-line \
         paste would arrive as one run-on line without this."
    );
}
