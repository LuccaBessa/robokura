//! The window: the list of assistants on the left, one thread in the middle, the open
//! assistant's own pane on the right.
//!
//! Nothing here is decided while drawing. A pane that needs a window is built where the
//! press was, because a press carries one and a draw does not.

use std::time::Duration;

use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, SharedString,
    Styled as _, Subscription, Window,
    component::{
        ActiveTheme as _,
        empty::{Empty, EmptyDescription, EmptyHeader, EmptyTitle},
        h_flex, v_flex,
    },
    div,
};

use robokura_core::{Assistant, Core};

use crate::theme;

use super::details::DetailsPane;
use super::main_thread::ThreadPane;
use super::settings::SettingsPane;
use super::sidebar::AssistantPane;
use super::{bar, layout};

/// Emitted by [`AssistantPane`] when the person opens an assistant.
pub struct OpenRequest(pub String);

/// Emitted by [`AssistantPane`] when the person asks for the settings. They are about
/// the machine, not about any one assistant, so they belong to no row in the list.
pub struct SettingsRequest;

pub fn open(window: &mut Window, cx: &mut App) -> Entity<AppView> {
    // A store that cannot be opened is said where the conversation would be. The list is
    // told too, so a press there changes nothing and says why.
    let opened = Core::open();
    let complaint = opened.as_ref().err().map(|error| error.to_string());
    let core = opened.ok().map(|core| cx.new(|_| core));

    // A mode the person chose outranks the machine's answer that `main` has already put in
    // force. Applied here rather than there because the store is not open until now, and
    // before the window draws rather than after, so the window never appears in one mode
    // and then repaints into another.
    if let Some(core) = &core {
        let settings = core.read(cx).settings().unwrap_or_default();
        if !settings.mode.trim().is_empty() {
            theme::apply(
                cx,
                theme::mode_in_force(&settings, window.appearance().into()),
            );
        }
    }

    let list = cx.new(|cx| AssistantPane::new(core.clone(), complaint.clone(), window, cx));
    cx.new(|cx| AppView::new(core, complaint, list, window, cx))
}

/// What the middle of the window holds. It holds one thing at a time, so this is one
/// state rather than a switch that could disagree with it.
enum Middle {
    /// The store would not open, and what it said about it.
    Complaint(String),
    /// Nothing has been opened yet.
    Empty,
    Settings(Entity<SettingsPane>),
    Thread(Entity<ThreadPane>),
}

impl Middle {
    fn render(&self) -> gpui_kit::AnyElement {
        match self {
            Middle::Settings(pane) => pane.clone().into_any_element(),
            Middle::Thread(pane) => pane.clone().into_any_element(),
            Middle::Complaint(problem) => empty_state("Robokura could not start", problem.clone()),
            Middle::Empty => empty_state(
                "Make an assistant to begin",
                "Press the + above the list. Its name, what it is for and its own pane are \
                 yours to change."
                    .to_string(),
            ),
        }
    }
}

pub struct AppView {
    core: Option<Entity<Core>>,
    list: Entity<AssistantPane>,
    middle: Middle,
    /// Having a pane is the same as it being open, so there is no switch beside this that
    /// could disagree.
    details: Option<Entity<DetailsPane>>,
    _subscriptions: Vec<Subscription>,
}

impl AppView {
    pub fn new(
        core: Option<Entity<Core>>,
        complaint: Option<String>,
        list: Entity<AssistantPane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let middle = match &complaint {
            Some(problem) => Middle::Complaint(problem.clone()),
            None => Middle::Empty,
        };

        // Two subscriptions rather than one, because the event itself is which of the
        // two things was said. Subscribed in the window, which is what lets a pane be
        // built on the press rather than on the draw after it.
        let opened = cx.subscribe_in(
            &list,
            window,
            |this: &mut AppView, _: &Entity<AssistantPane>, request: &OpenRequest, window, cx| {
                this.on_open(&request.0, window, cx);
            },
        );
        let settings = cx.subscribe_in(
            &list,
            window,
            |this: &mut AppView, _: &Entity<AssistantPane>, _: &SettingsRequest, _window, cx| {
                this.on_settings(cx);
            },
        );

        // The machine changing its appearance while the window is open should not have to
        // mean restarting it to see the change. A mode the person chose is not the
        // machine's to change, so it is put back over the top. Applying a theme refreshes
        // every window on its own.
        let chosen = core.clone();
        let appearance = window.observe_window_appearance(move |window, cx| {
            let machine = window.appearance().into();
            let settings = chosen
                .as_ref()
                .and_then(|core| core.read(cx).settings().ok())
                .unwrap_or_default();
            theme::apply(cx, theme::mode_in_force(&settings, machine));
        });

        // One loop reads every running agent rather than one per open thread: fifty
        // milliseconds while a reply is arriving, a quarter of a second otherwise.
        let ticker = cx.spawn(async move |this, cx| {
            loop {
                let busy = this
                    .update(cx, |this, cx| {
                        this.core
                            .as_ref()
                            .map(|core| core.read(cx).busy())
                            .unwrap_or(false)
                    })
                    .unwrap_or(false);

                cx.background_executor()
                    .timer(if busy {
                        Duration::from_millis(50)
                    } else {
                        Duration::from_millis(250)
                    })
                    .await;

                this.update(cx, |this, cx| {
                    if let Some(core) = &this.core {
                        core.update(cx, |core, cx| {
                            core.pump();
                            cx.notify();
                        });
                    }
                    cx.notify();
                })
                .ok();
            }
        });
        ticker.detach();

        let mut view = Self {
            core,
            list,
            middle,
            details: None,
            _subscriptions: vec![opened, settings, appearance],
        };

        // The list opens onto its first row, so the window opens onto that thread rather
        // than onto nothing. It is the same path a press takes, so there is one place
        // that puts an assistant's thread in the middle. A store that would not open has
        // no rows and so nothing to open on, and keeps its complaint.
        let opening = view.list.read(cx).open_id();
        if let Some(id) = opening {
            view.on_open(&id, window, cx);
        }

        view
    }

    pub fn core(&self) -> Option<Entity<Core>> {
        self.core.clone()
    }

    pub fn list(&self) -> &Entity<AssistantPane> {
        &self.list
    }

    pub fn thread(&self) -> Option<Entity<ThreadPane>> {
        match &self.middle {
            Middle::Thread(thread) => Some(thread.clone()),
            _ => None,
        }
    }

    pub fn settings(&self) -> Option<Entity<SettingsPane>> {
        match &self.middle {
            Middle::Settings(settings) => Some(settings.clone()),
            _ => None,
        }
    }

    pub fn details(&self) -> Option<Entity<DetailsPane>> {
        self.details.clone()
    }

    /// A row was pressed. The list has already marked it open, so this is only about
    /// what the middle shows now.
    fn on_open(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(core) = self.core.clone() else {
            return;
        };
        let Some(assistant) = core.read(cx).assistant(id).unwrap_or(None) else {
            return;
        };

        let thread = cx.new(|cx| ThreadPane::new(Some(core), assistant, window, cx));
        self.middle = Middle::Thread(thread);
        self.follow_open_assistant(window, cx);
        cx.notify();
    }

    /// The settings take the place of the conversation, and the row that was open stops
    /// being open because the middle is no longer its thread.
    fn on_settings(&mut self, cx: &mut Context<Self>) {
        let Some(core) = self.core.clone() else {
            // Nothing to put in the middle: the store's complaint is already in it.
            return;
        };

        self.middle = Middle::Settings(cx.new(|cx| SettingsPane::new(Some(core), cx)));
        self.close_details();
        self.list.update(cx, |list, cx| list.clear_open(cx));
        cx.notify();
    }

    fn open_assistant(&self, cx: &App) -> Option<Assistant> {
        let id = self.list.read(cx).open_id()?;
        let core = self.core.as_ref()?;
        core.read(cx).assistant(&id).unwrap_or(None)
    }

    /// The bar's control is the only way in and the only way out.
    pub(crate) fn on_toggle_details(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.details.is_some() {
            self.close_details();
        } else {
            self.details = self.build_details(window, cx);
        }
        cx.notify();
    }

    fn close_details(&mut self) {
        self.details = None;
    }

    /// The pane follows the assistant that is open, so a rename cannot land on the wrong
    /// record. A pane nobody asked for is not built on the way past.
    fn follow_open_assistant(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.details.is_none() {
            return;
        }
        let wanted = self.open_assistant(cx).map(|assistant| assistant.id);
        let showing = self
            .details
            .as_ref()
            .map(|pane| pane.read(cx).assistant_id().to_string());
        if wanted != showing {
            self.details = self.build_details(window, cx);
        }
    }

    fn build_details(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<DetailsPane>> {
        let assistant = self.open_assistant(cx)?;
        let core = self.core.clone()?;

        let pane = cx.new(|cx| DetailsPane::new(Some(core), assistant, window, cx));
        let removed = cx.subscribe(
            &pane,
            |this: &mut AppView,
             _: Entity<DetailsPane>,
             gone: &super::details::remove_assistant::Removed,
             cx| {
                this.on_assistant_deleted(&gone.0, cx);
            },
        );

        self._subscriptions.push(removed);
        Some(pane)
    }

    /// The thread goes only if it was this one's.
    fn on_assistant_deleted(&mut self, id: &str, cx: &mut Context<Self>) {
        let was_open = self.list.read(cx).open_id().as_deref() == Some(id);
        self.list.update(cx, |list, _cx| list.forget(id));
        if was_open {
            self.middle = Middle::Empty;
        }
        self.close_details();
        cx.notify();
    }
}

impl Render for AppView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let list = self.list.clone();
        let middle = self.middle.render();
        let details = self.details.as_ref().map_or_else(
            // A closed pane takes no width at all, rather than leaving a rail of its own.
            || div().into_any_element(),
            |pane| pane.clone().into_any_element(),
        );

        let top = bar::Bar {
            list: self.list.clone(),
            open_name: self.list.read(cx).open_name(cx),
            is_details_open: self.details.is_some(),
            window: cx.entity(),
        }
        .render(cx);

        // The panes run the whole height with the bar laid over them, each painting its
        // own colour to the top of the window so it can reach the window's own edge.
        div()
            .relative()
            .size_full()
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .child(list)
                    .child(middle)
                    .child(details),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(layout::BAR_HEIGHT)
                    .child(top),
            )
    }
}

fn empty_state(title: &'static str, description: String) -> gpui_kit::AnyElement {
    v_flex()
        .flex_1()
        .items_center()
        .justify_center()
        .child(
            Empty::new().header(
                EmptyHeader::new()
                    .title(EmptyTitle::new().child(title))
                    .description(EmptyDescription::new().child(SharedString::from(description))),
            ),
        )
        .into_any_element()
}
