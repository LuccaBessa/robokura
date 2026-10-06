//! The settings page's contents: the agent, and whatever selectors that agent has. A
//! pane holds the store and decides what to write; everything that turns a setting into
//! a row on the page is here.

use gpui_kit::{
    Entity, IntoElement as _, ParentElement as _, SharedString, Styled as _,
    component::{
        ActiveTheme as _,
        setting::{SettingField, SettingItem},
    },
    div,
};

use robokura_core::{ConfigOption, Settings as Configured};

use super::settings_pane::SettingsPane;

/// A model is what an assistant thinks with and a mode is how it is expected to
/// behave, and that is the order a person reads them in.
const SELECTOR_ORDER: [&str; 2] = ["model", "mode"];

/// Every agent on this machine, and the chosen one even when it is not there anymore: a
/// machine where the chosen agent is gone is exactly when a person needs to see which
/// one it was. `None` when the machine has none.
pub fn found_agents(settings: &Configured) -> Option<Vec<(SharedString, SharedString)>> {
    let mut options: Vec<(SharedString, SharedString)> = robokura_acp::agents::detect()
        .iter()
        .map(|found| {
            (
                SharedString::from(found.command.clone()),
                SharedString::from(found.name.to_string()),
            )
        })
        .collect();

    let chosen = settings.runs_on.trim().to_string();
    if !chosen.is_empty()
        && !options
            .iter()
            .any(|(command, _)| command.as_ref() == chosen.as_str())
    {
        options.push((
            SharedString::from(chosen),
            SharedString::from("Not on this computer"),
        ));
    }

    (!options.is_empty()).then_some(options)
}

/// The first agent found is the one in force until a choice is written, because nothing
/// chosen is the first agent, which is the one an assistant runs on.
pub fn agent_chooser(
    options: Vec<(SharedString, SharedString)>,
    current: &str,
    pane: Entity<SettingsPane>,
) -> SettingItem {
    let current = SharedString::from(current.to_string());

    SettingItem::new(
        "Agent",
        SettingField::dropdown(
            options,
            move |_| current.clone(),
            move |value, cx| {
                pane.update(cx, |this, cx| {
                    this.write(|settings| settings.runs_on = value.to_string(), cx);
                    // The list belonged to the agent just put down, and the one picked
                    // in its place has not been asked yet.
                    this.ask(cx);
                })
            },
        ),
    )
    .description("What a new assistant runs on.")
}

pub fn agent_setting(
    selector: &ConfigOption,
    settings: &Configured,
    pane: Entity<SettingsPane>,
) -> SettingItem {
    let id = selector.id.clone();
    let options: Vec<(SharedString, SharedString)> = selector
        .values
        .iter()
        .map(|(value, name)| {
            (
                SharedString::from(value.clone()),
                SharedString::from(name.clone()),
            )
        })
        .collect();

    // An unset model is not "no model", it is the one the agent picked.
    let current = settings
        .chosen
        .get(&selector.id)
        .cloned()
        .unwrap_or_else(|| selector.current.clone());

    let mut item = SettingItem::new(
        selector.name.clone(),
        SettingField::scrollable_dropdown(
            options,
            move |_| SharedString::from(current.clone()),
            move |value, cx| {
                pane.update(cx, |this, cx| {
                    this.write(
                        |settings| {
                            settings.chosen.insert(id.clone(), value.to_string());
                        },
                        cx,
                    )
                })
            },
        ),
    );
    let words = about(selector);
    if !words.is_empty() {
        item = item.description(words);
    }
    item
}

/// What to say where a chooser would be, when there is nothing to choose yet.
pub fn nothing_offered(agent: &str, probing: bool) -> String {
    if agent.trim().is_empty() {
        return "No agent was found on this computer.".to_string();
    }
    let name = robokura_acp::agents::display_name(agent);
    if probing {
        format!("Asking {name} what it can be set to.")
    } else {
        format!("{name} has not said what it can be set to.")
    }
}

/// A line of words standing in for a chooser, saying why there is none.
pub fn no_chooser(text: String) -> SettingItem {
    SettingItem::render(move |_, _, cx| {
        div()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(text.clone())
            .into_any_element()
    })
}

/// Anything an agent named that has no place here goes last rather than being dropped:
/// an agent is free to add a selector of its own.
pub fn ordered(selectors: Vec<ConfigOption>) -> Vec<ConfigOption> {
    let rank = |selector: &ConfigOption| {
        SELECTOR_ORDER
            .iter()
            .position(|kind| *kind == selector.category)
            .unwrap_or(SELECTOR_ORDER.len())
    };
    let mut selectors = selectors;
    selectors.sort_by_key(rank);
    selectors
}

/// A sentence of four words under a name the agent chose, and nothing where this
/// application has no word. A longer one would be a second thing to read beside the chooser.
pub fn about(selector: &ConfigOption) -> &'static str {
    match selector.category.as_str() {
        "model" => "What it thinks with.",
        "mode" => "How it behaves.",
        _ => "",
    }
}
