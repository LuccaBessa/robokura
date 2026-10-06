//! The field a message is typed into, and the one control beside it.

use gpui_kit::{
    Context, Entity, IntoElement as _, ParentElement as _, Styled as _,
    component::{
        ActiveTheme as _, IconName, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        input::{Input, InputState},
    },
};

use super::thread_pane::ThreadPane;
use crate::ui::layout::HAIRLINE;

pub struct Composer {
    pub draft: Entity<InputState>,
    pub is_working: bool,
}

impl Composer {
    pub fn render(self, cx: &mut Context<ThreadPane>) -> gpui_kit::AnyElement {
        // A surface raised off the conversation and the rule around it, both from the
        // theme: the field's own text and placeholder reach it through the same colours
        // as everything else, so the pill needs nothing of its own.
        let fill = cx.theme().secondary;
        let rule = cx.theme().border;
        let is_working = self.is_working;

        // One control, and it means one thing at a time. Only the icon name changes
        // between the two states, because `Button` sizes the icon from its own size.
        let action = Button::new(if is_working {
            "composer-stop"
        } else {
            "composer-send"
        })
        .primary()
        .small()
        .rounded_full()
        .icon(if is_working {
            IconName::Pause
        } else {
            IconName::ArrowUp
        })
        .tooltip(if is_working { "Stop" } else { "Send" })
        .on_click(cx.listener(move |this, _, window, cx| {
            if is_working {
                this.stop(cx);
            } else {
                this.send(window, cx);
            }
        }));

        // A pill, so the corners stay round as the field grows taller.
        // `appearance(false)` takes the input's own background and border away; left on,
        // it paints a second fill inside this one. The side inset is the transcript's, so
        // the field and the messages above it sit on one spine.
        h_flex()
            .mx_5()
            .mb_5()
            .px_4()
            .py_2()
            .min_h_12()
            .gap_2()
            .items_center()
            .rounded_full()
            .bg(fill)
            .border(HAIRLINE)
            .border_color(rule)
            // `min_w_0` is what lets the field shrink. A flex item defaults to a minimum
            // width of its own content, so the placeholder held the input open and the
            // button was pushed past the right edge of the pill.
            .child(Input::new(&self.draft).flex_1().min_w_0().appearance(false))
            .child(action)
            .into_any_element()
    }
}
