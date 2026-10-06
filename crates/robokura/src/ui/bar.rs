//! The window's title bar.
//!
//! This is [`TitleBar`] from the component set. It draws the bar, owns dragging and
//! double clicking, and renders the minimise, maximise and close buttons with the
//! platform's own control areas. Only the contents are ours.
//!
//! The bar carries no colour of its own: the panes run the whole height behind it and
//! each paints its own, so a pane can reach the window's edge.

use gpui_kit::{
    App, Entity, FontWeight, Hsla, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, SharedString, Styled as _,
    component::{
        ActiveTheme as _, Icon, IconName, Side, Sizable as _, TitleBar,
        avatar::Avatar,
        button::{Button, ButtonVariants as _},
        h_flex,
        sidebar::SidebarToggleButton,
    },
    div,
    prelude::FluentBuilder as _,
    px,
};

use super::AppView;
use super::layout::{
    BAR_HEIGHT, DETAILS_WIDTH, PANE_CONTROL_GAP, SIDEBAR_INSET_LEFT, SIDEBAR_INSET_RIGHT,
    SIDEBAR_WIDTH, window_buttons,
};
use super::sidebar::AssistantPane;

/// The strip across the top of the window: the one control that makes an assistant, the
/// name of the one that is open, and the one control for that one's own pane.
pub struct Bar {
    /// The list, which owns the control that makes an assistant.
    pub list: Entity<AssistantPane>,
    /// The name of the assistant that is open, and `None` exactly when none is, so one
    /// read answers both what the bar names and whether the pane has a subject.
    pub open_name: Option<SharedString>,
    pub is_details_open: bool,
    /// The window itself, which owns whether that pane is open.
    pub window: Entity<AppView>,
}

impl Bar {
    pub fn render(self, cx: &App) -> impl IntoElement {
        let has_assistant = self.open_name.is_some();

        // The bar is given no fill of its own, and that has to be said here rather than
        // left to the theme: the component mixes `title_bar` into `background` to build a
        // gradient for its own chrome, so a transparent token in the theme still paints a
        // band of 45% black. The panes run the full height behind the bar and each carries
        // its own colour up into the strip, which is the whole reason it has no fill.
        TitleBar::new()
            .h(BAR_HEIGHT)
            .bg(cx.theme().transparent)
            .border_0()
            // The component leaves room on the left for the system's own controls. On
            // Windows there are none, so that space is taken back.
            .pl_0()
            .child(over_the_list(&self.list, cx.theme().secondary_foreground))
            .child(over_the_open_assistant(self.open_name, cx))
            .child(div().flex_1().h_full())
            .child(details_control(
                has_assistant,
                self.is_details_open,
                &self.window,
            ))
    }
}

fn over_the_list(list: &Entity<AssistantPane>, glyph: Hsla) -> impl IntoElement {
    // Over the assistant list, and no colour: the list paints its own.
    h_flex()
        .w(SIDEBAR_WIDTH)
        .h_full()
        .pl(SIDEBAR_INSET_LEFT)
        .pr(SIDEBAR_INSET_RIGHT)
        .items_center()
        .justify_end()
        .child(drag_guard(
            "new-assistant-drag-guard",
            new_assistant_button(list, glyph),
        ))
}

/// The size is set on the component rather than by drawing a circle: `Button` draws its
/// icon at three quarters of the box. The mark is the quietest of the three text tiers,
/// the same one the search field and the settings row draw theirs in, so it reads as a
/// mark rather than as a control competing with the assistant's name.
fn new_assistant_button(list: &Entity<AssistantPane>, glyph: Hsla) -> impl IntoElement {
    Button::new("new-assistant")
        .ghost()
        .with_size(px(24.))
        .icon(Icon::new(IconName::Plus).text_color(glyph))
        .tooltip("New assistant")
        .on_click({
            let list = list.clone();
            move |_, _, cx| list.update(cx, |list, cx| list.create_assistant(cx))
        })
}

/// A press that reaches the bar is handed to the window manager, which drags the window
/// and keeps the release that would have made the control under it a click. The wrapper
/// stops the press here so the control is clickable without giving up the drag.
///
/// It carries its own identity: two elements on one identity make the control beneath
/// it ambiguous to anything looking it up by name.
fn drag_guard(id: &'static str, control: impl IntoElement) -> impl IntoElement {
    h_flex()
        .id(SharedString::from(id))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(control)
}

/// Named for where it sits rather than for what it holds, because that is what decides
/// it: it says which of the panes is in front, so it is here and not only in the list.
fn over_the_open_assistant(open_name: Option<SharedString>, cx: &App) -> impl IntoElement {
    h_flex()
        .h_full()
        .w(SIDEBAR_WIDTH)
        .flex_none()
        .gap_2()
        .items_center()
        .pl(SIDEBAR_INSET_LEFT)
        .when_some(open_name, |this, name| {
            this.child(Avatar::new().name(name.clone()).small()).child(
                div()
                    .text_sm()
                    .font_weight(FontWeight(600.))
                    .text_color(cx.theme().foreground)
                    .child(name),
            )
        })
}

/// The one control, and it is never inside the pane: with the pane open it stands just
/// outside its near edge, and closed it stands just clear of the window's own buttons.
///
/// It lands where it does by the filler that follows it.
fn details_control(
    has_assistant: bool,
    is_details_open: bool,
    window: &Entity<AppView>,
) -> gpui_kit::AnyElement {
    if !has_assistant {
        return div().into_any_element();
    }

    let beyond_the_control = if is_details_open {
        DETAILS_WIDTH - window_buttons() + PANE_CONTROL_GAP
    } else {
        PANE_CONTROL_GAP
    };

    h_flex()
        .h_full()
        .flex_none()
        .items_center()
        .child(details_toggle(is_details_open, window))
        .child(div().w(beyond_the_control).h_full())
        .into_any_element()
}

/// The component's own button, because it already draws the right glyph for a
/// right-hand panel in both states.
fn details_toggle(is_details_open: bool, window: &Entity<AppView>) -> impl IntoElement {
    let view = window.clone();
    drag_guard(
        "details-toggle",
        SidebarToggleButton::new()
            .side(Side::Right)
            .collapsed(!is_details_open)
            .accessibility_label(SharedString::from(if is_details_open {
                "Hide the assistant's own pane"
            } else {
                "Show the assistant's own pane"
            }))
            .on_click(move |_, window, cx| {
                view.update(cx, |view, cx| view.on_toggle_details(window, cx));
            }),
    )
}
