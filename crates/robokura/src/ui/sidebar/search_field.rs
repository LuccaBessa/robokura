//! The field that filters the list. There is nothing to filter yet: an assistant list
//! on one machine is short enough to read. It is a real field rather than a drawn one, so
//! it behaves like one from the first minute.

use gpui_kit::{
    App, Entity, IntoElement as _, ParentElement as _, Styled as _,
    component::{
        ActiveTheme as _, Icon, IconName, Sizable as _,
        input::{Input, InputState},
    },
    div,
};

use crate::ui::layout::item_radius;

pub fn render(search: &Entity<InputState>, cx: &App) -> gpui_kit::AnyElement {
    // Bottom spacing only: the insets come from the pane.
    div()
        .flex_none()
        .pb_3()
        .child(
            Input::new(search)
                .w_full()
                // The same corner shape as a row: it is the same pane.
                .rounded(item_radius(cx))
                .prefix(
                    Icon::new(IconName::Search)
                        .xsmall()
                        .text_color(cx.theme().secondary_foreground),
                ),
        )
        .into_any_element()
}
