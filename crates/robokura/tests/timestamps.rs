//! The words an item in the list uses for when something was said.
//!
//! The rest of an item is checked by where its parts landed, in `list`, because
//! that needs a window.

use chrono::{DateTime, Duration, Local, TimeZone as _};

use robokura::ui::sidebar::assistant_item::when;

fn at(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Local> {
    Local
        .with_ymd_and_hms(year, month, day, hour, minute, 0)
        .earliest()
        .expect("a local moment that exists")
}

fn said_at(stamped: DateTime<Local>, now: DateTime<Local>) -> String {
    when(stamped.timestamp(), now).to_string()
}

#[test]
fn a_message_from_today_is_said_by_the_clock() {
    let now = at(2026, 3, 12, 14, 30);
    assert_eq!(said_at(now - Duration::minutes(3), now), "14:27");
    assert_eq!(
        said_at(now - Duration::minutes(2) - Duration::seconds(59), now),
        "14:27",
        "and from either side of the minute it names"
    );
    assert_eq!(
        said_at(now - Duration::minutes(2) - Duration::seconds(1), now),
        "14:27",
        "so an item does not change its time while someone is looking at it"
    );
}

#[test]
fn the_day_before_is_named_rather_than_dated() {
    let now = at(2026, 3, 12, 9, 0);
    assert_eq!(said_at(at(2026, 3, 11, 23, 50), now), "Yesterday");
    assert_eq!(
        said_at(at(2026, 3, 11, 0, 5), now),
        "Yesterday",
        "however early in the day it was"
    );
}

#[test]
fn an_older_day_is_a_date_and_a_year_is_only_carried_when_it_differs() {
    let now = at(2026, 3, 12, 9, 0);
    assert_eq!(said_at(at(2026, 1, 3, 16, 0), now), "3 Jan");
    assert_eq!(said_at(at(2025, 12, 31, 16, 0), now), "31/12/2025");
}

#[test]
fn a_stamp_ahead_of_the_clock_is_still_treated_as_today() {
    let now = at(2026, 3, 12, 14, 30);
    assert_eq!(
        said_at(now + Duration::seconds(20), now),
        "14:30",
        "two clocks that disagree by seconds must not turn a message from a moment ago into a \
         date in the future"
    );
}

#[test]
fn a_stamp_no_calendar_could_hold_says_nothing_rather_than_something_wrong() {
    assert_eq!(
        when(i64::MAX, at(2026, 3, 12, 14, 30)).to_string(),
        "",
        "a wrong time would be a claim about when something happened, which is worse than \
         saying nothing"
    );
}
