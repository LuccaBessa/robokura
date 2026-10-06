//! One assistant's thread. The rows in the transcript and the composer are each built
//! in a file of their own beside this one.
//!
//! Nothing is held here that the records already hold. Every redraw reads the thread
//! again, so what is on screen and what is stored cannot drift apart, and a reply
//! arriving in pieces is the same code path as a finished one.

use gpui_kit::{
    App, AppContext as _, Context, Entity, Hsla, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Window,
    component::{
        ActiveTheme as _,
        input::InputState,
        message_scroller::{MessageScroller, MessageScrollerState},
        v_flex,
    },
    div,
};

use robokura_core::{Assistant, Core, Message};

use crate::ui::layout::{HAIRLINE, middle_pane};

use super::composer::Composer;
use super::transcript_item::TranscriptItem;

/// One line of the transcript: what was said, and by whom or by what.
#[derive(Clone)]
pub enum Line {
    Said(String),
    Replied { text: String, cut_off: bool },
    Note(String),
    Trouble(String),
}

/// One place decides this, so a message that was cut off reads the same way whether it
/// arrived a moment ago or was found on reopening the application.
fn line_of(message: &Message) -> Option<Line> {
    // A reply line exists before the assistant has said anything, so an empty one is
    // not a message.
    if message.body.is_empty() {
        return None;
    }
    Some(match (message.kind, message.is_person()) {
        (robokura_core::Kind::Note, _) => Line::Note(message.body.clone()),
        (_, true) => Line::Said(message.body.clone()),
        (_, false) => Line::Replied {
            text: message.body.clone(),
            cut_off: !message.complete,
        },
    })
}

pub struct ThreadPane {
    core: Option<Entity<Core>>,
    assistant: Assistant,
    draft: Entity<InputState>,
    /// Follows the newest message, which is what a live conversation needs.
    pub scroller: Entity<MessageScrollerState>,
}

impl ThreadPane {
    pub fn new(
        core: Option<Entity<Core>>,
        assistant: Assistant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let draft = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(SharedString::from(format!("Message {}", assistant.name)))
        });
        let scroller = cx.new(|cx| MessageScrollerState::new(0, cx));
        cx.observe(&scroller, |_, _, cx| cx.notify()).detach();

        Self {
            core,
            assistant,
            draft,
            scroller,
        }
    }

    fn lines(&self, cx: &App) -> Vec<Line> {
        let Some(core) = &self.core else {
            return Vec::new();
        };
        let guard = core.read(cx);
        let mut lines: Vec<Line> = guard
            .messages(&self.assistant.id)
            .unwrap_or_default()
            .iter()
            .filter_map(line_of)
            .collect();

        // A reply that has said nothing yet is left out rather than drawn as an empty
        // bubble. The line standing in its place is not stored, because it is only
        // true while the turn is going.
        if guard.thinking(&self.assistant.id) {
            lines.push(Line::Note("Working".to_string()));
        }

        if let Some(problem) = guard.problem(&self.assistant.id) {
            lines.push(Line::Trouble(problem.to_string()));
        }
        lines
    }

    fn is_working(&self, cx: &App) -> bool {
        self.core
            .as_ref()
            .map(|core| core.read(cx).working(&self.assistant.id))
            .unwrap_or(false)
    }

    /// Lines live inside the scroller so a reply arriving in pieces keeps the newest text
    /// in view. A scroller needs a height, so the empty case is drawn beside it rather
    /// than inside it.
    fn transcript(&self, lines: Vec<Line>, background: Hsla) -> gpui_kit::AnyElement {
        if lines.is_empty() {
            return v_flex()
                .flex_1()
                .min_h_0()
                .px_6()
                .py_6()
                .gap_3()
                .items_center()
                .justify_end()
                .into_any_element();
        }

        MessageScroller::new(
            "thread",
            self.scroller.clone(),
            move |index, _, cx| match lines.get(index) {
                Some(line) => TranscriptItem {
                    line: line.clone(),
                    index,
                }
                .render(cx),
                None => div().into_any_element(),
            },
        )
        .flex_1()
        .min_h_0()
        .px_5()
        .pt_2()
        .pb_5()
        .with_bottom_fade(background)
        .into_any_element()
    }

    pub(crate) fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(core) = self.core.clone() else {
            return;
        };
        let text = self.draft.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }

        self.draft
            .update(cx, |draft, cx| draft.set_value("", window, cx));

        let assistant_id = self.assistant.id.clone();
        if let Err(error) = core.update(cx, |core, _| core.send(&assistant_id, &text)) {
            tracing::warn!("the message could not be sent: {error}");
        }
        cx.notify();
    }

    pub(crate) fn stop(&mut self, cx: &mut Context<Self>) {
        let Some(core) = self.core.clone() else {
            return;
        };
        let assistant_id = self.assistant.id.clone();
        core.update(cx, |core, _| core.stop(&assistant_id));
        cx.notify();
    }
}

impl Render for ThreadPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_working = self.is_working(cx);
        let lines = self.lines(cx);
        let count = lines.len();
        let background = cx.theme().background;
        let rule = cx.theme().border;

        let transcript = self.transcript(lines, background);

        // The scroller decides how many rows to draw from its own count, not from the
        // list passed to it, so it is told the count here. The guard makes it settle
        // after one extra frame instead of resetting for ever.
        self.scroller.update(cx, |state, cx| {
            if state.item_count() != count {
                state.reset(count, cx);
            }
        });

        let composer = Composer {
            draft: self.draft.clone(),
            is_working,
        }
        .render(cx);

        // The bar is drawn over the pane, so the rule that separates the two belongs on
        // the top of the conversation rather than on the top of the window.
        middle_pane(background).child(
            v_flex()
                .flex_1()
                .min_h_0()
                .border_t(HAIRLINE)
                .border_color(rule)
                .child(transcript)
                .child(composer),
        )
    }
}
