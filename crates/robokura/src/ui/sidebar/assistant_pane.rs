//! The list of assistants, and the one control that adds one. Each part is built in a
//! file of its own beside this one.

use std::collections::HashMap;

use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, SharedString, Styled as _, TestSupportExt as _, Window,
    component::{
        ActiveTheme as _, Sizable as _, alert::Alert, input::InputState,
        scroll::ScrollableElement as _, v_flex,
    },
    prelude::FluentBuilder as _,
    px,
};

use robokura_core::{Assistant, Core, Preview};

use crate::ui::app_view::{OpenRequest, SettingsRequest};
use crate::ui::layout::{BAR_HEIGHT, SIDEBAR_WIDTH, side_pane};

use super::assistant_item::AssistantItem;
use super::{search_field, settings_item};

pub struct AssistantPane {
    core: Option<Entity<Core>>,
    /// Why nothing can be listed, if the store would not open.
    problem: Option<String>,
    /// Which assistant is open. The list is the one place that knows, because it is the
    /// one place the person picks one, and every other part asks it here.
    open: Option<String>,
    search: Entity<InputState>,
}

impl EventEmitter<OpenRequest> for AssistantPane {}
impl EventEmitter<SettingsRequest> for AssistantPane {}

impl AssistantPane {
    pub fn new(
        core: Option<Entity<Core>>,
        problem: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));

        // The window opens onto the first row, which is the assistant the thread was
        // last spoken in. A window with somebody's work in it that opens onto an empty
        // middle is asking them to go and find what they left off on.
        let open = match &core {
            Some(core) => core
                .read(cx)
                .assistants()
                .unwrap_or_default()
                .into_iter()
                .next()
                .map(|assistant| assistant.id),
            None => None,
        };

        Self {
            core,
            problem,
            open,
            search,
        }
    }

    fn assistants(&self, cx: &App) -> Vec<Assistant> {
        match &self.core {
            Some(core) => core.read(cx).assistants().unwrap_or_default(),
            None => Vec::new(),
        }
    }

    /// One read for the whole list. A row is drawn from the thread behind it, and the
    /// window redraws several times a second whether or not anything changed.
    fn previews(&self, cx: &App) -> HashMap<String, Preview> {
        match &self.core {
            Some(core) => core
                .read(cx)
                .previews()
                .unwrap_or_default()
                .into_iter()
                .map(|preview| (preview.assistant_id.clone(), preview))
                .collect(),
            None => HashMap::new(),
        }
    }

    fn query(&self, cx: &App) -> String {
        self.search.read(cx).value().trim().to_lowercase()
    }

    /// Matches names only, and does not move which assistant is open or empty the
    /// thread: a search narrows what the list draws and nothing else.
    fn matching<'a>(every: &'a [Assistant], query: &str) -> Vec<&'a Assistant> {
        every
            .iter()
            .filter(|assistant| query.is_empty() || assistant.name.to_lowercase().contains(query))
            .collect()
    }

    pub fn open_id(&self) -> Option<String> {
        self.open.clone()
    }

    pub fn open_name(&self, cx: &App) -> Option<SharedString> {
        let id = self.open.as_deref()?;
        let core = self.core.as_ref()?;
        let assistant = core.read(cx).assistant(id).unwrap_or(None)?;
        Some(SharedString::from(assistant.name))
    }

    pub fn forget(&mut self, id: &str) {
        if self.open.as_deref() == Some(id) {
            self.open = None;
        }
    }

    /// Nothing is marked open once the middle is showing the settings rather than a
    /// conversation.
    pub fn clear_open(&mut self, cx: &mut Context<Self>) {
        if self.open.take().is_some() {
            cx.notify();
        }
    }

    /// No form stands in front of the press, because the three things a form would ask
    /// for are all changed on the assistant's own pane.
    pub fn create_assistant(&mut self, cx: &mut Context<Self>) {
        let Some(core) = self.core.clone() else {
            return;
        };

        match core.update(cx, |core, _| core.create_default_assistant()) {
            Ok(assistant) => {
                self.problem = None;
                self.open = Some(assistant.id.clone());
                cx.emit(OpenRequest(assistant.id));
            }
            Err(error) => self.problem = Some(error.to_string()),
        }
        cx.notify();
    }

    pub(crate) fn open(&mut self, id: &str, cx: &mut Context<Self>) {
        self.open = Some(id.to_string());
        cx.emit(OpenRequest(id.to_string()));
    }

    pub(crate) fn request_settings(&mut self, cx: &mut Context<Self>) {
        cx.emit(SettingsRequest);
    }

    /// Types into the search field the way a person does.
    pub fn search_for(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |input, cx| {
            input.set_value(query.to_string(), window, cx)
        });
        cx.notify();
    }

    /// One row per assistant the search leaves standing.
    fn visible_items(&self, rem_size: Pixels, cx: &mut Context<Self>) -> Vec<gpui_kit::AnyElement> {
        let previews = self.previews(cx);
        let open = self.open.clone();
        Self::matching(&self.assistants(cx), &self.query(cx))
            .into_iter()
            .map(|assistant| {
                AssistantItem {
                    assistant,
                    preview: previews.get(assistant.id.as_str()),
                    is_open: open.as_deref() == Some(assistant.id.as_str()),
                }
                .render(rem_size, cx)
            })
            .collect()
    }

    /// A list with no rows says nothing about why. The blank area still takes the space
    /// the rows would have, so the field above and the row below stay where they are.
    /// `list-blank` is what tells a check the two cases apart.
    fn items_area(&self, rem_size: Pixels, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let rows = self.visible_items(rem_size, cx);
        if rows.is_empty() {
            return gpui_kit::div()
                .flex_1()
                .id("list-blank")
                .test_support()
                .into_any_element();
        }

        v_flex()
            .flex_1()
            .min_h_0()
            .gap(px(2.))
            .overflow_y_scrollbar()
            .children(rows)
            .into_any_element()
    }

    /// A failure is shown here rather than nowhere: there is no form left to carry it,
    /// and a press that changed nothing without saying why is a control a person cannot
    /// tell from a broken one.
    fn failure(&self) -> Option<gpui_kit::AnyElement> {
        // Only once the store has opened, because the middle says it otherwise.
        self.core.as_ref()?;
        let text = self.problem.clone()?;

        Some(
            Alert::error("new-assistant-problem", text)
                .small()
                .flex_none()
                .into_any_element(),
        )
    }
}

impl Render for AssistantPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let failure = self.failure();
        // A row's height is worked out in rems, so it has to be asked of the window: it
        // is the only place the size of an em is known.
        let rows = self.items_area(window.rem_size(), cx);

        // The insets are padding on the pane, once, so nothing inside carries its own.
        // The first BAR_HEIGHT is held clear because the bar is drawn over this pane and
        // nothing a person can press may sit under its drag region.
        side_pane(cx.theme().sidebar, SIDEBAR_WIDTH)
            .pt(BAR_HEIGHT)
            .child(search_field::render(&self.search, cx))
            .when_some(failure, |this, failure| this.child(failure))
            .child(rows)
            .child(settings_item::render(cx))
    }
}
