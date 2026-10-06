//! The settings pane: what the person has configured for this machine. It takes the
//! place of the conversation rather than sitting over it, so the bars and the panes
//! either side of it do not move.
//!
//! There is one page and it is the general one: the agent is a group in it, beside the
//! group that says how this window is drawn. Every setting is written the moment it is
//! changed: there is no Save control, because a setting that takes effect only when
//! someone remembers to press a button is one that is not in force.

use gpui_kit::{
    App, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StyleRefinement, Styled as _, TestSupportExt as _, Window,
    component::{
        ActiveTheme as _, ThemeMode,
        group_box::GroupBoxVariant,
        setting::{SettingField, SettingGroup, SettingItem, SettingPage, Settings},
    },
    div, px,
};

use robokura_core::{ConfigOption, Core, Settings as Configured};

use crate::theme;
use crate::ui::layout::middle_pane;

use super::chooser::{self, ordered};

/// Locked rather than resizable: there is nothing on either side of the handle to
/// give up space to.
const PAGE_SIDEBAR: f32 = 168.;

pub struct SettingsPane {
    core: Option<Entity<Core>>,
}

impl SettingsPane {
    pub fn new(core: Option<Entity<Core>>, cx: &mut Context<Self>) -> Self {
        let pane = Self { core };
        // Asked once here rather than once per draw: a draw may happen several times a
        // second, and starting a program is not something to decide on a frame.
        pane.ask(cx);
        pane
    }

    /// Read on every redraw rather than held, so what the pane draws and what is stored
    /// cannot drift apart.
    fn stored(&self, cx: &App) -> Configured {
        self.core
            .as_ref()
            .and_then(|core| core.read(cx).settings().ok())
            .unwrap_or_default()
    }

    /// So the pane can say an agent is being asked rather than looking broken.
    fn is_probing(&self, cx: &App) -> bool {
        self.core
            .as_ref()
            .map(|core| core.read(cx).probing())
            .unwrap_or(false)
    }

    /// The same answer a new assistant gets, so what is offered here is what it will
    /// actually be started with.
    fn chosen_agent(settings: &Configured) -> String {
        if settings.runs_on.trim().is_empty() {
            robokura_acp::agents::first().unwrap_or_default()
        } else {
            settings.runs_on.clone()
        }
    }

    fn offered(settings: &Configured, agent: &str) -> Vec<ConfigOption> {
        settings.offered.get(agent).cloned().unwrap_or_default()
    }

    /// Started rather than waited for: the window's own reading loop collects the
    /// answer, so nothing here blocks.
    pub(crate) fn ask(&self, cx: &mut Context<Self>) {
        let Some(core) = self.core.clone() else {
            return;
        };
        let agent = Self::chosen_agent(&self.stored(cx));
        core.update(cx, |core, _| core.probe(&agent));
    }

    pub(crate) fn write(&mut self, mutate: impl FnOnce(&mut Configured), cx: &mut Context<Self>) {
        let mut settings = self.stored(cx);
        mutate(&mut settings);

        let Some(core) = self.core.clone() else {
            return;
        };

        match core.update(cx, |core, _| core.save_settings(&settings)) {
            // The change is not held, because a field showing a value that was not
            // stored is a field lying about what this application will do.
            Ok(()) => {}
            Err(error) => tracing::warn!("a setting could not be written: {error}"),
        }
        cx.notify();
    }

    /// Each selector's name, how many values it offers, and the value it is set to.
    pub fn choices(&self, cx: &App) -> Vec<(String, usize, String)> {
        let settings = self.stored(cx);
        ordered(Self::offered(&settings, &Self::chosen_agent(&settings)))
            .into_iter()
            .map(|selector| (selector.name, selector.values.len(), selector.current))
            .collect()
    }

    fn agent_group(
        &self,
        settings: &Configured,
        agent: &str,
        cx: &mut Context<Self>,
    ) -> SettingGroup {
        // No card behind the items: the page already sits on the conversation's own
        // colour, and a second surface inside it would be a third thing in a window
        // meant to be two panes and a bar. Without this the group takes whatever card the
        // component set defaults to.
        let mut group = SettingGroup::new()
            .title("Agent")
            .variant(GroupBoxVariant::Normal);
        let pane = cx.entity();

        match chooser::found_agents(settings) {
            Some(options) => {
                group = group.item(chooser::agent_chooser(options, agent, pane.clone()))
            }
            None => {
                group = group.item(chooser::no_chooser(
                    "No agent was found on this computer.".to_string(),
                ))
            }
        }

        let selectors = ordered(Self::offered(settings, agent));
        if selectors.is_empty() {
            let words = chooser::nothing_offered(agent, self.is_probing(cx));
            return group.item(chooser::no_chooser(words));
        }

        for selector in &selectors {
            group = group.item(chooser::agent_setting(selector, settings, pane.clone()));
        }
        group
    }

    /// Which half of the theme this window is drawn in. Read from the theme rather than
    /// from the store, so the switch says what is on screen and not what was once asked
    /// for: a machine that has not been chosen for still answers the question.
    fn appearance_group(&self, pane: Entity<Self>) -> SettingGroup {
        SettingGroup::new()
            .title("Appearance")
            .variant(GroupBoxVariant::Normal)
            .item(
                SettingItem::new(
                    "Dark Mode",
                    SettingField::switch(
                        |cx: &App| cx.theme().mode.is_dark(),
                        move |dark: bool, cx| {
                            pane.update(cx, |this, cx| this.use_mode(dark, cx));
                        },
                    ),
                )
                .description("Whether this window is drawn light or dark."),
            )
    }

    /// Written and applied together: a switch that has moved but not repainted the window
    /// is a switch lying about what is on screen.
    fn use_mode(&mut self, dark: bool, cx: &mut Context<Self>) {
        let mode = if dark { "dark" } else { "light" };
        self.write(|settings| settings.mode = mode.to_string(), cx);
        theme::apply(
            cx,
            if dark {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            },
        );
    }
}

impl Render for SettingsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A store that would not open is not a reason to draw an empty pane: the window
        // says so in the middle instead.
        let content: gpui_kit::AnyElement = match self.core.clone() {
            Some(_) => {
                let settings = self.stored(cx);
                let agent = Self::chosen_agent(&settings);
                // The list of pages is drawn in the list's own colour, which is darker
                // than the pane it stands in.
                let sidebar = StyleRefinement::default().bg(cx.theme().background);

                Settings::new("settings")
                    .sidebar_width(px(PAGE_SIDEBAR))
                    .sidebar_size_range(px(PAGE_SIDEBAR)..px(PAGE_SIDEBAR))
                    .sidebar_style(&sidebar)
                    .page(
                        // Nothing to reset to: everything here is either chosen or
                        // absent, and absent already means the shipped answer.
                        SettingPage::new("General")
                            .resettable(false)
                            .default_open(true)
                            .groups(vec![
                                self.agent_group(&settings, &agent, cx),
                                self.appearance_group(cx.entity()),
                            ]),
                    )
                    .into_any_element()
            }
            None => div().into_any_element(),
        };

        // The pane runs the height of the window and the bar is drawn over it.
        middle_pane(cx.theme().background)
            .id("settings-pane")
            .child(content)
            .test_support()
    }
}
