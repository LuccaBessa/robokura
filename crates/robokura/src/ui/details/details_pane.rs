//! The assistant's own pane on the right: what it is called, what it is for, and the
//! control that removes it. The form and the delete control are built in files of their
//! own beside this one.
//!
//! The pane takes no width when it is closed. A rail with nothing on it reads as a
//! control that is broken, and the control that opens this pane is in the bar.

use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, TestSupportExt as _, Window,
    component::{
        ActiveTheme as _, Sizable as _, avatar::Avatar, h_flex, input::InputState, v_flex,
    },
};

use robokura_core::{Assistant, Core};

use crate::ui::layout::{BAR_HEIGHT, DETAILS_WIDTH, side_pane};

use super::assistant_form::AssistantForm;
use super::remove_assistant::Removed;

pub struct DetailsPane {
    pub(crate) core: Option<Entity<Core>>,
    pub(crate) assistant: Assistant,
    /// What the person has typed, which is not what is stored until they keep it.
    name: Entity<InputState>,
    title: Entity<InputState>,
    description: Entity<InputState>,
    pub(crate) problem: Option<String>,
}

impl EventEmitter<Removed> for DetailsPane {}

impl DetailsPane {
    pub fn new(
        core: Option<Entity<Core>>,
        assistant: Assistant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // `InputState` has no value to build one with, so each field is written after
        // it is made.
        let mut field = |value: String, placeholder: &str| {
            let entity =
                cx.new(|cx| InputState::new(window, cx).placeholder(placeholder.to_string()));
            entity.update(cx, |input, cx| input.set_value(value, window, cx));
            entity
        };

        Self {
            core,
            name: field(assistant.name.clone(), "Name"),
            title: field(assistant.title.clone(), "Title"),
            description: field(
                assistant.description.clone(),
                "Description, in a sentence or two",
            ),
            assistant,
            problem: None,
        }
    }

    fn wanted(&self, cx: &App) -> (String, String, String) {
        (
            trimmed(self.name.read(cx).value()),
            trimmed(self.title.read(cx).value()),
            trimmed(self.description.read(cx).value()),
        )
    }

    /// What is in the fields differs from what is stored, which is the only thing that
    /// makes saving worth offering.
    pub fn has_changes(&self, cx: &App) -> bool {
        let (name, title, description) = self.wanted(cx);
        name != self.assistant.name.trim()
            || title != self.assistant.title.trim()
            || description != self.assistant.description.trim()
    }

    pub(crate) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(core) = self.core.clone() else {
            return;
        };
        let (name, title, description) = self.wanted(cx);

        if name.is_empty() {
            self.problem = Some("An assistant needs a name.".to_string());
            cx.notify();
            return;
        }

        let id = self.assistant.id.clone();
        match core.update(cx, |core, _| {
            core.update_assistant(&id, name, title, description)
        }) {
            Ok(assistant) => {
                self.assistant = assistant;
                self.problem = None;
                self.fill(window, cx);
            }
            Err(error) => self.problem = Some(error.to_string()),
        }
        cx.notify();
    }

    pub(crate) fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.problem = None;
        self.fill(window, cx);
        cx.notify();
    }

    fn fill(&self, window: &mut Window, cx: &mut Context<Self>) {
        for (field, value) in [
            (&self.name, self.assistant.name.clone()),
            (&self.title, self.assistant.title.clone()),
            (&self.description, self.assistant.description.clone()),
        ] {
            field.update(cx, |input, cx| input.set_value(value, window, cx));
        }
    }

    /// Asked rather than assumed, so a pane left over from a different assistant is
    /// rebuilt rather than shown.
    pub(crate) fn assistant_id(&self) -> &str {
        &self.assistant.id
    }

    pub fn assistant_name(&self) -> String {
        self.assistant.name.clone()
    }

    pub fn name_field(&self) -> Entity<InputState> {
        self.name.clone()
    }

    pub fn title_field(&self) -> Entity<InputState> {
        self.title.clone()
    }

    pub fn description_field(&self) -> Entity<InputState> {
        self.description.clone()
    }
}

impl Render for DetailsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = AssistantForm {
            problem: self.problem.clone(),
            has_changes: self.has_changes(cx),
            name: self.name.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
        }
        .render(cx);
        let remove = super::remove_assistant::render(cx);

        let portrait = h_flex()
            .flex_none()
            .justify_center()
            .py_4()
            .child(Avatar::new().name(self.assistant.name.clone()).large());

        // The title's middle is the middle of everything else in the pane: the insets are
        // not equal, so centring on the colour would leave it a few pixels right of the rest.
        let title = h_flex()
            .flex_none()
            .h(BAR_HEIGHT)
            .items_center()
            .justify_center()
            .child(
                gpui_kit::div()
                    .text_sm()
                    .font_weight(gpui_kit::FontWeight(600.))
                    .text_color(cx.theme().foreground)
                    .child("Settings")
                    .id("details-title")
                    .test_support(),
            );

        // The content uses the pane's whole width: the window's buttons are drawn over
        // its far corner, but they are in the bar's strip and this starts below it.
        side_pane(cx.theme().sidebar, DETAILS_WIDTH)
            .child(title)
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .child(portrait)
                    .child(body)
                    .child(remove),
            )
    }
}

fn trimmed(value: impl AsRef<str>) -> String {
    let value = value.as_ref().trim();
    if value.is_empty() {
        String::new()
    } else {
        value.to_string()
    }
}
