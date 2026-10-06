//! The colours and the measurements the product draws with.
//!
//! They live in `themes/robokura.json` rather than here. A theme file is applied whole,
//! so a mode change brings its own colours with it instead of arriving on its own and
//! overwriting whatever was set beside it, and the component set reads the file's values
//! for the parts of a window this crate never draws.
//!
//! Two halves are shipped, one per mode, and which one is in force is the machine's to
//! decide. Nothing here holds a colour: a colour written in two places is a colour that
//! drifts.

use std::rc::Rc;

use gpui_kit::component::{Theme, ThemeConfig, ThemeMode, ThemeSet};

use robokura_core::Settings as Configured;

/// The one file both halves live in, read at compile time. A theme is therefore part of
/// the build rather than something to be found beside the binary at run time.
const THEMES: &str = include_str!("../themes/robokura.json");

/// Puts `mode` in force, having first installed both halves.
///
/// Both are installed before either is chosen, because installing a half registers it
/// under its own mode and a mode change then loads whatever is registered there.
/// Installing only the half in force would leave the other one as the component set's
/// own, which is not a theme this product asked for.
pub fn apply(cx: &mut gpui_kit::App, mode: ThemeMode) {
    for half in halves() {
        Theme::update(cx, |theme| theme.apply_config(&half));
    }

    Theme::change(mode, None, cx);
}

/// The mode in force: what the person chose on the settings page, and the machine's own
/// answer where they have chosen nothing.
///
/// Anything but those two words is treated as no choice at all, because a value this
/// version does not know is a store written by a newer one and refusing to open the
/// window over it would cost a person their settings.
pub fn mode_in_force(settings: &Configured, machine: ThemeMode) -> ThemeMode {
    match settings.mode.trim() {
        "dark" => ThemeMode::Dark,
        "light" => ThemeMode::Light,
        _ => machine,
    }
}

/// One half per mode, and both are named here rather than found in the file by index, so
/// a file that lost one fails instead of quietly leaving a half unregistered.
fn halves() -> Vec<Rc<ThemeConfig>> {
    let set: ThemeSet = serde_json::from_str(THEMES)
        .expect("themes/robokura.json is read at compile time and checked by cargo test");

    [ThemeMode::Light, ThemeMode::Dark]
        .into_iter()
        .map(|mode| {
            set.themes
                .iter()
                .find(|theme| theme.mode == mode)
                .cloned()
                .map(Rc::new)
                .unwrap_or_else(|| panic!("themes/robokura.json has no {} half", mode.name()))
        })
        .collect()
}
