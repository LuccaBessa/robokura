//! One entry of the transcript. Each kind of line is drawn here, so the pane itself only
//! decides the order things appear in.

use gpui_kit::{
    App, HighlightStyle, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, StyleRefinement, Styled as _,
    component::{
        ActiveTheme as _, Colorize as _,
        bubble::{Bubble, BubbleVariant},
        message::{Message, MessageAlignment, MessageContent},
        text::{TextView, TextViewStyle},
    },
    div, relative,
};

use super::thread_pane::Line;

/// How much of the pane one message may take.
const MEASURE: f32 = 0.72;

/// How far a block of code is moved from its bubble towards the text written on it. The
/// theme paints code in `muted`, which is this bubble's own fill, so without this a block
/// in a reply has no edge at all.
const CODE_STEP: f32 = 0.09;

pub struct TranscriptItem {
    pub line: Line,
    /// Its place in the thread, which is what its own drawn state hangs off: two rows
    /// on one id would extend one another.
    pub index: usize,
}

impl TranscriptItem {
    pub fn render(self, cx: &App) -> gpui_kit::AnyElement {
        let Self { line, index } = self;
        let id = SharedString::from(format!("line-{index}"));

        match line {
            // `Message` owns the alignment of the row and of the body inside it. This
            // side is plain text, and `Bubble` paints it in `primary_foreground` for it.
            // It cannot be markdown: markdown is painted in the body colour whatever
            // bubble it sits in, and the body colour is unreadable on this pill.
            Line::Said(text) => Message::new()
                .id(id)
                .alignment(MessageAlignment::End)
                .content(
                    MessageContent::new()
                        .bubble(Bubble::new().max_w(relative(MEASURE)).child(text)),
                )
                .into_any_element(),

            // A reply that was cut off says so in the reply itself rather than in a
            // separate line, so what the person reads is what they were sent. The view
            // keeps its own state under this id, so the text it is given each frame
            // extends what it already had.
            Line::Replied { text, cut_off } => Message::new()
                .id(id)
                .alignment(MessageAlignment::Start)
                .content(
                    MessageContent::new().bubble(
                        Bubble::new()
                            // `muted` over `foreground`, both of them this product's own
                            // colours. `Secondary` would paint the text in
                            // `secondary_foreground` instead, which is a role the theme
                            // sets for buttons.
                            .with_variant(BubbleVariant::Muted)
                            .max_w(relative(MEASURE))
                            .child(
                                TextView::markdown(
                                    SharedString::from(format!("replied-{index}")),
                                    if cut_off {
                                        format!("{text}\n\n*The reply was cut off.*")
                                    } else {
                                        text
                                    },
                                )
                                .style(code_stepped_off(cx))
                                .stream_fade(true),
                            ),
                    ),
                )
                .into_any_element(),

            Line::Note(text) => div()
                .id(id)
                .w_full()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(text)
                .into_any_element(),

            Line::Trouble(text) => div()
                .id(id)
                .w_full()
                .text_sm()
                .text_color(cx.theme().danger)
                .child(text)
                .into_any_element(),
        }
    }
}

/// Moves code off the bubble it sits in, by one step towards the text written on it.
///
/// Both the block and the inline span, because a fenced block drawn on a pill with an
/// inline span drawn on the same pill reads as two different surfaces. Everything else is
/// left alone: the component folds this style over the one the theme derived, so a field
/// left at its default keeps the themed value.
fn code_stepped_off(cx: &App) -> TextViewStyle {
    let theme = cx.theme();

    // What the bubble looks like once it is over the conversation, rather than the fill on
    // its own: the fill is nearly transparent, and mixing that with an opaque text colour
    // drags its alpha along too, which would put the block most of the way to the text
    // instead of a step off the bubble.
    let seen = theme.background.blend(theme.muted);
    let fill = seen.mix_oklab(theme.foreground, CODE_STEP);

    TextViewStyle::default()
        .code_block(StyleRefinement::default().bg(fill))
        .inline_code(HighlightStyle {
            background_color: Some(fill),
            ..Default::default()
        })
}
