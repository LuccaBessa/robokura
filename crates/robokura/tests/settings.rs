//! The order the settings page offers its choosers in, and what this application says
//! about each.

use robokura::ui::settings::{about, ordered};
use robokura_core::ConfigOption;

fn selector(id: &str, category: &str) -> ConfigOption {
    ConfigOption {
        id: id.to_string(),
        name: id.to_string(),
        category: category.to_string(),
        values: Vec::new(),
        current: String::new(),
    }
}

fn ids(offered: &[ConfigOption]) -> Vec<&str> {
    offered.iter().map(|s| s.id.as_str()).collect()
}

#[test]
fn a_model_is_offered_before_how_it_behaves() {
    let offered = ordered(vec![selector("mode", "mode"), selector("model", "model")]);
    assert_eq!(
        ids(&offered),
        ["model", "mode"],
        "a person reads what an assistant thinks with before how it behaves"
    );
}

#[test]
fn a_selector_this_application_has_no_word_for_is_still_offered() {
    let offered = ordered(vec![
        selector("custom", "_something"),
        selector("model", "model"),
    ]);
    assert_eq!(
        ids(&offered),
        ["model", "custom"],
        "an agent's own selector goes last rather than being dropped, so a control the agent \
         offered is never silently unavailable"
    );
}

#[test]
fn a_selector_is_described_in_one_line_or_in_nothing() {
    assert_eq!(about(&selector("model", "model")), "What it thinks with.");
    assert_eq!(
        about(&selector("custom", "_something")),
        "",
        "where there is no word for it. The agent has already named it, and a sentence here \
         would be a second answer to a question it answered."
    );
}
