//! The control that removes an assistant, and what it says before it does. There is no
//! undo and nothing is kept, so the confirmation is not a formality: it names the
//! assistant and counts what goes with it.

use gpui_kit::{
    Context, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window,
    component::{
        IconName, WindowExt as _,
        button::{Button, ButtonVariant, ButtonVariants as _},
        h_flex,
    },
};

use super::details_pane::DetailsPane;

/// Emitted once the assistant is gone.
pub struct Removed(pub String);

/// Full width and the default size: this is the only control here that cannot be taken
/// back, and a button sized to its own label reads as a footnote beside three fields.
pub fn render(cx: &mut Context<DetailsPane>) -> gpui_kit::AnyElement {
    h_flex()
        .flex_none()
        .py_4()
        .child(
            Button::new("remove-assistant")
                .danger()
                .w_full()
                .label("Delete assistant")
                .icon(IconName::Delete)
                .on_click(cx.listener(|this, _, window, cx| this.ask_to_remove(window, cx))),
        )
        .into_any_element()
}

impl DetailsPane {
    /// Counts what is at stake now rather than keeping it, so it cannot drift.
    pub(crate) fn ask_to_remove(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(core) = self.core.clone() else {
            return;
        };

        let name = self.assistant.name.clone();
        let id = self.assistant.id.clone();
        let messages = core
            .read(cx)
            .messages(&id)
            .map(|messages| messages.len())
            .unwrap_or_default();

        let what = match messages {
            0 => "Its thread goes with it.".to_string(),
            1 => "Its thread and the one message in it go with it.".to_string(),
            n => format!("Its thread and all {n} messages in it go with it."),
        };

        // The pane is captured rather than the context, because the answer arrives
        // later and outside the frame that asked. The build closure is called more than
        // once, so each call gets its own handle.
        let pane = cx.entity();

        window.open_alert_dialog(cx, move |alert, _window, _cx| {
            let pane = pane.clone();
            alert
                .icon(IconName::TriangleAlert)
                .title(SharedString::from(format!("Delete {name}?")))
                .description(SharedString::from(format!(
                    "{what} There is no undo and nothing is kept."
                )))
                .show_cancel(true)
                .ok_text(SharedString::from("Delete"))
                .ok_variant(ButtonVariant::Danger)
                .cancel_text(SharedString::from("Keep"))
                .on_ok(move |_, _window, cx| {
                    pane.update(cx, |this, cx| this.remove(cx));
                    true
                })
        });
    }

    /// The pane stays open on a failure, because an assistant that is still there still
    /// has fields to edit.
    pub(crate) fn remove(&mut self, cx: &mut Context<Self>) {
        let Some(core) = self.core.clone() else {
            return;
        };
        let id = self.assistant.id.clone();

        if let Err(error) = core.update(cx, |core, _| core.delete_assistant(&id)) {
            self.problem = Some(error.to_string());
            cx.notify();
            return;
        }
        cx.emit(Removed(id));
    }
}
