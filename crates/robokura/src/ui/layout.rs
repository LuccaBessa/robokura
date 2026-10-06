//! The measurements more than one part of the window has to agree on, and the two
//! shapes every pane takes. A number two files each hold a copy of is a number that
//! will disagree.

use gpui_kit::{
    App, Div, Hsla, Pixels, Styled as _,
    component::{ActiveTheme as _, TITLE_BAR_HEIGHT, v_flex},
    px, rems,
};

/// Taller than the component's own default of 34, because the bar also carries the
/// open assistant's name and an avatar.
pub const BAR_HEIGHT: Pixels = px(44.);

/// How much of the bar's right-hand end the window's own buttons hold. Worked out from
/// the component's own published height rather than written down, so it cannot drift.
pub fn window_buttons() -> Pixels {
    TITLE_BAR_HEIGHT * 3.
}

/// A thing drawn hard against its neighbour reads as part of it.
pub const PANE_CONTROL_GAP: Pixels = px(8.);

/// The bar's section over the list is this wide too, so the two meet along one edge.
pub const SIDEBAR_WIDTH: Pixels = px(280.);

/// A hairline rather than a whole pixel. Every rule in this product is half a pixel wide,
/// which is what keeps a boundary visible without it becoming an edge.
pub const HAIRLINE: Pixels = px(0.5);

/// Narrower than the list: this pane holds three fields and a delete control rather
/// than every assistant.
pub const DETAILS_WIDTH: Pixels = px(288.);

/// The two panes are read as one row of content, so their fields sit on the same two
/// vertical lines.
pub const DETAILS_INSET_LEFT: Pixels = SIDEBAR_INSET_LEFT;
pub const DETAILS_INSET_RIGHT: Pixels = SIDEBAR_INSET_RIGHT;

pub const SIDEBAR_INSET_LEFT: Pixels = px(16.);
pub const SIDEBAR_INSET_RIGHT: Pixels = px(10.);

/// The corner shape of a row, the settings row beside them and the search field above
/// them: all three are on the same pane and read as one set. A tier of the theme's rather
/// than a number of its own, so the shape follows whichever half of the theme is in force
/// and a message bubble above them lands on a tier of its own.
pub fn item_radius(cx: &App) -> Pixels {
    cx.theme().radius_tokens().lg
}

/// As tall as the tallest item in the list: a name line, the gap under it, a message
/// line, and the padding a row carries above and below. Every item is held to it, so an
/// assistant with nothing in its thread takes the same room as one with a message and the
/// list stops moving as rows come and go.
///
/// In rems, because the two lines it is worked out from are in rems too, and a person
/// with larger text would otherwise get uneven rows back.
pub fn item_height(rem_size: Pixels) -> Pixels {
    rems(3.375).to_pixels(rem_size)
}

/// The conversation and the settings, which take each other's place in the middle. The
/// bar is laid over the panes rather than above them, so the top is held clear of it.
pub fn middle_pane(colour: Hsla) -> Div {
    v_flex()
        .flex_1()
        .h_full()
        .min_w_0()
        .bg(colour)
        .pt(BAR_HEIGHT)
}

/// One of the two panes at the edge of the window, on the list's insets so the two
/// sides read as a pair.
pub fn side_pane(colour: Hsla, width: Pixels) -> Div {
    v_flex()
        .w(width)
        .h_full()
        .flex_none()
        .bg(colour)
        .pl(SIDEBAR_INSET_LEFT)
        .pr(SIDEBAR_INSET_RIGHT)
}
