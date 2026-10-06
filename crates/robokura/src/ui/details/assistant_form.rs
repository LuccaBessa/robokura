//! The form that says what an assistant is for, and the controls that keep what is in
//! it or put it back.
//!
//! Each entry is a name above a field rather than a placeholder inside one, so every
//! name stays on screen while it is being typed into.

use gpui_kit::{
    App, Context, Entity, IntoElement as _, ParentElement as _, Styled as _,
    component::{
        ActiveTheme as _, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        input::{InputGroup, InputGroupInput, InputState},
        label::Label,
        scroll::ScrollableElement as _,
        v_flex,
    },
    prelude::FluentBuilder as _,
};

use super::details_pane::DetailsPane;

pub struct AssistantForm {
    pub problem: Option<String>,
    /// Whether anything has changed, which is what decides whether there is anything to
    /// keep.
    pub has_changes: bool,
    pub name: Entity<InputState>,
    pub title: Entity<InputState>,
    pub description: Entity<InputState>,
}

impl AssistantForm {
    pub fn render(self, cx: &mut Context<DetailsPane>) -> gpui_kit::AnyElement {
        v_flex()
            .flex_1()
            .min_h_0()
            .gap_4()
            .py_3()
            .overflow_y_scrollbar()
            .child(labelled(cx, "name-field", "Name", &self.name))
            .child(labelled(cx, "title-field", "Title (optional)", &self.title))
            .child(labelled(
                cx,
                "description-field",
                "Description",
                &self.description,
            ))
            .when_some(self.problem, |this, text| {
                this.child(
                    gpui_kit::div()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(text),
                )
            })
            // Keeping and cancelling appear only once something has changed.
            .when(self.has_changes, |this| {
                this.child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("save-assistant")
                                .primary()
                                .small()
                                .label("Save")
                                .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                        )
                        .child(
                            Button::new("cancel-assistant")
                                .ghost()
                                .small()
                                .label("Cancel")
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.cancel(window, cx)),
                                ),
                        ),
                )
            })
            .into_any_element()
    }
}

/// The name is above the field rather than a placeholder in it, so it is still there once
/// something has been typed. The group draws the frame around the input.
fn labelled(
    cx: &App,
    id: &'static str,
    name: &str,
    input: &Entity<InputState>,
) -> gpui_kit::AnyElement {
    v_flex()
        .gap_1()
        .child(
            Label::new(name.to_string())
                .text_xs()
                .text_color(cx.theme().muted_foreground),
        )
        .child(
            InputGroup::new(id)
                .small()
                .input(InputGroupInput::new(input).aria_label(name)),
        )
        .into_any_element()
}
