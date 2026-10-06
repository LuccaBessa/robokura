//! One assistant in the list: what it is called, what its thread last said, and when
//! it last said it. The title is not drawn: it is what the assistant is for rather than
//! a second name, and the pane holds it with the description.

use chrono::{DateTime, Datelike as _, Local};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement as _, ParentElement as _, Pixels, SharedString,
    Styled as _, TestSupportExt as _,
    component::{ActiveTheme as _, avatar::Avatar, h_flex, list::ListItem, v_flex},
    div,
    prelude::FluentBuilder as _,
};

use robokura_core::{Assistant, Preview};

use super::assistant_pane::AssistantPane;
use crate::ui::layout::{item_height, item_radius};

pub(crate) struct AssistantItem<'a> {
    pub assistant: &'a Assistant,
    pub preview: Option<&'a Preview>,
    pub is_open: bool,
}

impl AssistantItem<'_> {
    pub fn render(self, rem_size: Pixels, cx: &mut Context<AssistantPane>) -> gpui_kit::AnyElement {
        let Self {
            assistant,
            preview,
            is_open,
        } = self;
        let id = assistant.id.clone();
        let name = assistant.name.clone();
        let stem = format!("assistant-{}", assistant.id);

        // A thread with nothing in it draws only the name, and centres what it has: a
        // name above a space that says nothing reads as an assistant that has gone
        // quiet. The row around it is held to one height, so centring is within the row
        // rather than the row itself growing to suit.
        let heading = h_flex()
            .w_full()
            .gap_2()
            .items_baseline()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .text_ellipsis()
                    .text_color(cx.theme().sidebar_foreground)
                    .id(format!("{stem}-name"))
                    .child(name.clone())
                    .test_support(),
            )
            .when_some(preview, |this, preview| {
                this.child(
                    div()
                        .flex_none()
                        .text_xs()
                        // The quietest of the three tiers, one below the message under it.
                        // On the same colour as that message the two read as one line, which
                        // is what the gap below is there to prevent.
                        .text_color(cx.theme().secondary_foreground)
                        .id(format!("{stem}-when"))
                        .child(when(preview.at, Local::now()))
                        .test_support(),
                )
            });

        // The gap is between the three lines rather than the two of them, so the time
        // reads as a line of its own and not as a label stuck to the message under it.
        let middle = v_flex()
            .flex_1()
            .min_w_0()
            .gap_1()
            .child(heading)
            .when_some(preview, |this, preview| {
                this.child(
                    div()
                        .text_xs()
                        .text_ellipsis()
                        .text_color(cx.theme().muted_foreground)
                        .id(format!("{stem}-last"))
                        .child(preview.body.clone())
                        .test_support(),
                )
            });

        ListItem::new(SharedString::from(stem.clone()))
            .rounded(item_radius(cx))
            .min_h(item_height(rem_size))
            .selected(is_open)
            .on_click(cx.listener(move |this, _, _, cx| this.open(&id, cx)))
            .child(
                h_flex()
                    .flex_1()
                    .w_full()
                    .gap_2()
                    .child(
                        div()
                            .id(format!("{stem}-initials"))
                            .child(Avatar::new().name(name).size_8())
                            .test_support(),
                    )
                    .child(middle),
            )
            .into_any_element()
    }
}

/// The clock for today, "Yesterday" for the day before, a date without a year for any
/// other day this year, and its own numbers for a day in another year.
///
/// `now` is a parameter so the words each day gets can be checked without waiting.
pub fn when(at: i64, now: DateTime<Local>) -> SharedString {
    let Some(said) = DateTime::from_timestamp(at, 0) else {
        return SharedString::default();
    };
    let said = said.with_timezone(&Local);
    let days = now
        .date_naive()
        .signed_duration_since(said.date_naive())
        .num_days();

    // A stamp ahead of the clock is still today: two machines whose time runs slightly
    // fast should not turn a message from a minute ago into a date.
    let text = match days {
        d if d <= 0 => said.format("%H:%M").to_string(),
        1 => "Yesterday".to_string(),
        _ if said.year() == now.year() => said.format("%-d %b").to_string(),
        _ => said.format("%d/%m/%Y").to_string(),
    };
    SharedString::from(text)
}
