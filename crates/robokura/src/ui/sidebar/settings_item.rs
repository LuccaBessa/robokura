//! The row at the bottom of the list that opens the settings. It is about the machine
//! rather than about any one assistant, so it belongs to no assistant.

use gpui_kit::{
    Context, IntoElement as _, ParentElement as _, SharedString, Styled as _,
    component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, list::ListItem},
};

use crate::ui::layout::item_radius;

use super::assistant_pane::AssistantPane;

pub fn render(cx: &mut Context<AssistantPane>) -> gpui_kit::AnyElement {
    ListItem::new(SharedString::from("settings-item"))
        .rounded(item_radius(cx))
        .on_click(cx.listener(|this, _, _, cx| this.request_settings(cx)))
        .p_2()
        .mb_3()
        .child(
            h_flex()
                .w_full()
                .gap_2()
                .child(
                    Icon::new(IconName::Settings)
                        .xsmall()
                        .text_color(cx.theme().secondary_foreground),
                )
                .child(
                    gpui_kit::div()
                        .text_sm()
                        .text_color(cx.theme().sidebar_foreground)
                        .child("Settings"),
                ),
        )
        .into_any_element()
}
