//! What the theme actually resolves to.
//!
//! A theme file is applied whole, and a colour the file leaves out is filled in from the
//! component set's own palette rather than reported as missing. So a file can name half
//! what the product draws with and look perfectly valid, which is what the first check
//! here is for.
//!
//! The two sides of a thread carry opposite text colours, so they do not obey the same
//! rule and a check on one does not cover the other. The agent's side holds markdown,
//! which is always painted in the body colour, so its fill has to be readable against the
//! body colour. The person's side holds plain text, which takes the colour of its bubble,
//! so its fill has to be readable against that.
//!
//! A bubble whose text lands in the same colour as its own fill is an empty box, and an
//! empty box is the only thing visible.

use std::path::{Path, PathBuf};

use gpui_kit::{
    Hsla, Rgba, TestAppContext,
    component::{ActiveTheme as _, Colorize as _, ThemeMode},
};

use robokura::theme;
use robokura_core::Settings;

/// Every colour role the theme file names.
///
/// A role here that the file does not name, or a key in the file that is not here, is a
/// failure. Adding a component that reads a role means adding it to this list, and a role
/// nothing reads means taking it out of the file.
///
/// A few entries here are not drawn by this product, and each is one of these:
///
/// - The six `base.*` colours, which the theme schema lists as required, so a file without
///   them is not a valid theme.
/// - `title_bar.border`. The bar's own fill is the theme's transparent rather than a named
///   colour, because the component mixes that token into `background` rather than using it
///   as a fill.
/// - The semantic families `success`, `warning` and `info`. `danger` is read, as the
///   trouble line and the alert; these three are its siblings in the alert set and are
///   named for the day a success or warning is shown rather than left to fall back to the
///   component set's own palette.
///
/// This list is written by hand against the file, so it cannot notice a role the product
/// stopped reading. It says the file and this list agree, not that either agrees with the
/// code.
const DRAWN: [&str; 63] = [
    "accent.background",
    "accent.foreground",
    "background",
    "base.blue",
    "base.cyan",
    "base.green",
    "base.magenta",
    "base.red",
    "base.yellow",
    "border",
    "button.active.background",
    "button.background",
    "button.danger.active.background",
    "button.danger.background",
    "button.danger.foreground",
    "button.danger.hover.background",
    "button.foreground",
    "button.hover.background",
    "button.primary.active.background",
    "button.primary.background",
    "button.primary.foreground",
    "button.primary.hover.background",
    "caret",
    "danger.background",
    "danger.foreground",
    "foreground",
    "group_box.background",
    "info.background",
    "info.foreground",
    "group_box.foreground",
    "input.border",
    "link",
    "list.active.background",
    "list.background",
    "list.even.background",
    "list.hover.background",
    "muted.background",
    "muted.foreground",
    "overlay",
    "popover.background",
    "popover.foreground",
    "primary.background",
    "primary.foreground",
    "ring",
    "scrollbar.background",
    "scrollbar.thumb.background",
    "scrollbar.thumb.hover.background",
    "secondary.background",
    "secondary.foreground",
    "selection.background",
    "success.background",
    "success.foreground",
    "sidebar.accent.background",
    "sidebar.accent.foreground",
    "sidebar.background",
    "sidebar.border",
    "sidebar.foreground",
    "table.head.background",
    "table.head.foreground",
    "title_bar.border",
    "warning.background",
    "warning.foreground",
    "window.border",
];

/// How far apart two fills have to be before text on one can be read on the other.
const MINIMUM_GAP: f32 = 40.;

/// How light a colour looks once it is laid over `behind`.
///
/// A fill can carry alpha, and a translucent one is only as light as what is under it.
/// Measuring a colour on its own reports white at six per cent as pure white, which
/// would say an agent's reply was light text on a white pill when it is light text on
/// the conversation.
fn lightness_over(colour: Hsla, behind: Hsla) -> f32 {
    let top = colour.to_rgb();
    let under = behind.to_rgb();
    let alpha = colour.a.clamp(0., 1.);
    let mix = |over: f32, under: f32| over * alpha + under * (1. - alpha);
    let red = mix(top.r, under.r);
    let green = mix(top.g, under.g);
    let blue = mix(top.b, under.b);
    (red * 0.2126 + green * 0.7152 + blue * 0.0722) * 100.
}

fn gap(first: Hsla, second: Hsla, behind: Hsla) -> f32 {
    (lightness_over(first, behind) - lightness_over(second, behind)).abs()
}

/// Every region of the window has to be a plane rather than the absence of one.
///
/// A fill can be perfectly readable with its own words and still be the same colour as the
/// window behind it, in which case the pane it belongs to is not there: there is no
/// boundary, and a list drawn on the conversation is a list floating in it. Translucent
/// tints are how that happens, because a tint a few per cent over its own backdrop lands
/// back on the backdrop.
///
/// Only the regions are held to this. An overlay is not: a menu above a near white window
/// is white, and is told apart by the ring and the shadow the component set puts round it
/// rather than by a fill of its own.
#[gpui_kit::test]
fn every_region_is_a_plane_rather_than_the_absence_of_one(cx: &mut TestAppContext) {
    /// How far a region may sit from the window behind it before it stops reading as one.
    const MINIMUM: f32 = 2.5;

    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        cx.update(gpui_kit::init);
        cx.update(|cx| theme::apply(cx, mode));

        cx.update(|cx| {
            let t = cx.theme();
            let window = t.background;

            let regions = [
                ("the list", t.sidebar, window),
                ("the composer's field", t.secondary, window),
            ];

            for (name, fill, behind) in regions {
                let apart = gap(fill, behind, behind);
                println!("{mode:?} {name:<24} {apart:>5.1} from the surface behind it");
                assert!(
                    apart >= MINIMUM,
                    "in {mode:?} {name} is {fill:?} on {behind:?}, which is only \
                     {apart:.1} points apart. It cannot be told from what is behind it, \
                     so the region it belongs to is not drawn at all.",
                );
            }
        });
    }
}

/// Every surface a person reads words off, in both halves of the theme.
///
/// The two halves are separate values in a file, so one check per mode is the only way a
/// light palette gets held to the same standard as a dark one.
#[gpui_kit::test]
fn what_a_thread_and_a_composer_are_drawn_in_can_be_read(cx: &mut TestAppContext) {
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        cx.update(gpui_kit::init);
        cx.update(|cx| theme::apply(cx, mode));

        cx.update(|cx| {
            let t = cx.theme();
            let conversation = t.background;
            let sidebar = t.sidebar;

            let surfaces = [
                (
                    "what the person said",
                    t.primary,
                    t.primary_foreground,
                    conversation,
                ),
                ("what an agent said", t.muted, t.foreground, conversation),
                ("the composer", t.secondary, t.foreground, conversation),
                (
                    "the search field",
                    t.input_background(),
                    t.foreground,
                    sidebar,
                ),
                ("a sidebar row", sidebar, t.sidebar_foreground, sidebar),
            ];

            for (name, fill, text, behind) in surfaces {
                println!(
                    "{mode:?} {name:<20} fill {:>5.1}  text {:>5.1}  gap {:>5.1}",
                    lightness_over(fill, behind),
                    lightness_over(text, behind),
                    gap(fill, text, behind),
                );
                assert!(
                    gap(fill, text, behind) > MINIMUM_GAP,
                    "{name} is drawn in {fill:?} over {behind:?} in {mode:?}, which is \
                     only {} points from the {text:?} its text is painted in, so \
                     whatever is written there cannot be read.",
                    gap(fill, text, behind)
                );
            }
        });
    }
}

/// The colour the component set would have used on its own is not the same as a missing
/// colour, so nothing here can be left out quietly: every role the product draws with has
/// to be named in both halves, and nothing may be named that is not drawn with.
///
/// The file itself is read rather than the theme it produces, because a config read back
/// carries a key for every colour there is whether the file named it or not.
#[gpui_kit::test]
fn the_theme_file_names_every_colour_the_product_draws_with(cx: &mut TestAppContext) {
    let _ = cx;
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("themes/robokura.json");
    let set: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).expect("the theme file is there"))
            .expect("the theme file is JSON");

    let halves = set["themes"]
        .as_array()
        .expect("a theme set carries its halves")
        .clone();

    assert_eq!(
        halves.len(),
        2,
        "the theme file carries {} halves, and the window expects one per mode.",
        halves.len(),
    );

    for half in &halves {
        let name = half["name"].as_str().expect("a half is named");
        let mode = half["mode"].as_str().expect("a half says which mode it is");
        let named: Vec<&str> = half["colors"]
            .as_object()
            .expect("a half carries its colours")
            .keys()
            .map(String::as_str)
            .collect();

        for role in DRAWN {
            assert!(
                named.contains(&role),
                "the {name} half of themes/robokura.json does not name {role:?}, so that \
                 colour comes from the component set's own palette instead of this \
                 product's, and the {mode} window is drawn in two palettes at once.",
            );
        }

        let unused: Vec<&&str> = named.iter().filter(|role| !DRAWN.contains(role)).collect();
        assert!(
            unused.is_empty(),
            "the {name} half of themes/robokura.json names {unused:?}, which nothing in \
             the product draws with. A colour nothing reads is one that will be changed \
             without anything moving.",
        );
    }
}

/// `accent` is the background of an inline code span as well as the fallback fill for a
/// selected row. When one value served both, choosing a different row colour silently
/// repainted code inside a reply.
#[gpui_kit::test]
fn the_row_fill_and_the_code_fill_are_two_colours(cx: &mut TestAppContext) {
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        cx.update(gpui_kit::init);
        cx.update(|cx| theme::apply(cx, mode));

        cx.update(|cx| {
            let t = cx.theme();
            println!(
                "{mode:?} selected row {:?}, inline code {:?}",
                t.list_active, t.accent
            );
            assert_ne!(
                t.list_active, t.accent,
                "in {mode:?} the selected row and an inline code span are both {:?}, so \
                 one of them cannot be changed without changing the other.",
                t.list_active,
            );
        });
    }
}

/// The bar is the one surface in this product with no fill of its own, and the theme's
/// transparent is where that comes from.
///
/// It has to be said at the component rather than in the theme file, because the component
/// does not use `title_bar.background` as a fill: it mixes that token into `background` to
/// build a gradient for its own chrome. A token that is fully transparent therefore still
/// paints a band, and the band is this check's subject — if the bar's fill were left to the
/// theme, this is what would be drawn over the panes.
#[gpui_kit::test]
fn the_bar_takes_the_theme_s_transparent_rather_than_a_colour(cx: &mut TestAppContext) {
    /// The component's own weighting of the bar token against the background.
    const TITLE_BAR_WEIGHT: f32 = 0.55;

    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        cx.update(gpui_kit::init);
        cx.update(|cx| theme::apply(cx, mode));

        cx.update(|cx| {
            let t = cx.theme();

            assert_eq!(
                t.transparent.a, 0.,
                "the bar is filled with the theme's transparent, and in {mode:?} that is \
                 {:?}, which would paint the bar rather than leave the pane showing.",
                t.transparent,
            );

            // What the component would have painted, had the theme been left to decide.
            // This is the component's own arithmetic, which mixes in RGB rather than in the
            // hue, lightness and saturation a theme is written in.
            let token = Hsla::transparent_black().to_rgb();
            let behind = t.background.to_rgb();
            let mixed = Hsla::from(Rgba {
                r: token.r * TITLE_BAR_WEIGHT + behind.r * (1. - TITLE_BAR_WEIGHT),
                g: token.g * TITLE_BAR_WEIGHT + behind.g * (1. - TITLE_BAR_WEIGHT),
                b: token.b * TITLE_BAR_WEIGHT + behind.b * (1. - TITLE_BAR_WEIGHT),
                a: token.a * TITLE_BAR_WEIGHT + behind.a * (1. - TITLE_BAR_WEIGHT),
            });

            println!(
                "{mode:?} over {:?} the bar would fade from {mixed:?} to nothing",
                t.background,
            );
            assert!(
                mixed.a > 0.2,
                "a fully transparent bar token mixed into the background comes out at \
                 {:?} in {mode:?}, which is not transparent. Naming a transparent \
                 `title_bar.background` would therefore put a band over the panes rather \
                 than leave the bar invisible.",
                mixed,
            );
        });
    }
}

/// Code in a reply is painted in `muted`, which is that reply's own fill. The transcript
/// moves it one step off, and this is the number that step is worth.
///
/// The step is taken from what the bubble looks like once it is over the conversation,
/// not from the fill on its own: `muted` is nearly transparent, and mixing that with an
/// opaque text colour drags its alpha along, which puts the block most of the way to the
/// text rather than a step off the bubble.
#[gpui_kit::test]
fn code_in_a_reply_is_not_the_same_colour_as_the_reply(cx: &mut TestAppContext) {
    const STEP: f32 = 0.09;
    /// A large flat area reads at a fraction of what a line of prose needs, so this is far
    /// below the gap the text pairs are held to.
    const MINIMUM: f32 = 1.5;

    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        cx.update(gpui_kit::init);
        cx.update(|cx| theme::apply(cx, mode));

        cx.update(|cx| {
            let t = cx.theme();
            let pane = t.background;
            let seen = pane.blend(t.muted);
            let block = seen.mix_oklab(t.foreground, STEP);
            let apart = gap(seen, block, pane);

            println!(
                "{mode:?} the reply is {seen:?} over the pane, code in it is {block:?}, \
                 {apart:.1} points apart"
            );
            assert!(
                block.a > 0.99,
                "in {mode:?} code in a reply is painted {block:?}, which is not opaque, so \
                 what is under it still shows through it.",
            );
            assert!(
                apart > MINIMUM,
                "in {mode:?} code in a reply is painted {block:?} on a bubble that reads as \
                 {seen:?}, only {apart:.1} points apart, so a block of code has no edge.",
            );
        });
    }
}

/// The theme carries each colour twice: once as a plain field a component reads, and
/// once as a token that may hold a gradient. Writing only the plain field can leave
/// the two disagreeing, and a component that reads the token then paints something the
/// product never asked for.
#[gpui_kit::test]
fn writing_a_colour_leaves_the_token_saying_the_same_thing(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| theme::apply(cx, ThemeMode::Dark));

    cx.update(|cx| {
        let theme = cx.theme();

        let pairs: [(&str, Hsla, Hsla); 10] = [
            (
                "background",
                theme.background,
                theme.tokens.background.color,
            ),
            (
                "foreground",
                theme.foreground,
                theme.tokens.foreground.color,
            ),
            ("primary", theme.primary, theme.tokens.primary.color),
            (
                "primary_foreground",
                theme.primary_foreground,
                theme.tokens.primary_foreground.color,
            ),
            ("secondary", theme.secondary, theme.tokens.secondary.color),
            (
                "secondary_foreground",
                theme.secondary_foreground,
                theme.tokens.secondary_foreground.color,
            ),
            ("muted", theme.muted, theme.tokens.muted.color),
            ("accent", theme.accent, theme.tokens.accent.color),
            (
                "button_primary",
                theme.button_primary,
                theme.tokens.button_primary.color,
            ),
            ("input", theme.input, theme.tokens.input.color),
        ];

        for (name, field, token) in pairs {
            // Compared on their own values: a field and the token that carries it
            // describe one colour, so a translucent one is the same colour in both and
            // there is nothing behind either of them here.
            let difference = (field.to_rgb().r - token.to_rgb().r).abs()
                + (field.to_rgb().g - token.to_rgb().g).abs()
                + (field.to_rgb().b - token.to_rgb().b).abs()
                + (field.a - token.a).abs();
            println!(
                "{name:<20} field {:>5.1}  token {:>5.1}  alpha {:.2}/{:.2}",
                lightness_over(field, Hsla::black()),
                lightness_over(token, Hsla::black()),
                field.a,
                token.a
            );
            assert!(
                difference < 0.001,
                "{name} is {field:?} as a field but {token:?} as a token, so a \
                 component reading the token paints a colour the product did not set."
            );
        }
    });
}

/// A person who has chosen a mode keeps it, and one who has not gets the machine's answer.
/// The word is read from the store rather than from what the window happens to be in, so
/// the answer does not depend on whether anything has been drawn yet.
#[test]
fn the_mode_a_person_chose_outranks_the_machine_and_nothing_chosen_does_not() {
    let machine = ThemeMode::Dark;

    for (stored, wanted) in [
        ("dark", ThemeMode::Dark),
        ("light", ThemeMode::Light),
        ("", machine),
        ("dark ", ThemeMode::Dark),
        // A store written by a newer build, which this one must not refuse to open.
        ("solarized", machine),
    ] {
        let settings = Settings {
            mode: stored.to_string(),
            ..Settings::default()
        };
        assert_eq!(
            theme::mode_in_force(&settings, machine),
            wanted,
            "with the store saying {stored:?}, the window is drawn {wanted:?}. A word this \
             version does not know is the machine's answer rather than an error."
        );
    }
}

/// A colour at a call site cannot follow the theme, so there is one way to write one and
/// it is not in the interface. The theme file is the only place a hex belongs.
///
/// This reads the interface's own source, so it also says how much it read: a scan that
/// found nothing because it read nothing would otherwise pass on the strength of nothing.
#[gpui_kit::test]
fn no_colour_is_written_at_a_call_site(cx: &mut TestAppContext) {
    let _ = cx;
    let interface = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    let mut scan = Scan::default();
    scan.walk(&interface);

    for (path, line, text) in &scan.found {
        println!("{}:{line}: {text}", path.display());
    }

    assert!(
        scan.read > 10,
        "only {} source files were read from {}, so this says nothing. The interface has \
         more than that in it.",
        scan.read,
        interface.display(),
    );
    assert!(
        scan.found.is_empty(),
        "{} colour(s) written in the interface rather than named in \
         themes/robokura.json. A colour set on one element and not another is a colour \
         that drifts, and one written here cannot follow the theme.",
        scan.found.len(),
    );
}

#[derive(Default)]
struct Scan {
    /// How many source files were read, so a scan that read nothing cannot pass.
    read: usize,
    /// Where a colour was written: the file, the line, and the line itself.
    found: Vec<(PathBuf, usize, String)>,
}

impl Scan {
    fn walk(&mut self, directory: &Path) {
        let entries = std::fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("{} could not be read: {error}", directory.display()));

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                self.walk(&path);
                continue;
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                continue;
            }

            self.read += 1;
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("{} could not be read: {error}", path.display()));

            for (index, line) in source.lines().enumerate() {
                // A comment may say what a colour is; only code may hold one.
                let code = line.split_once("//").map_or(line, |(code, _)| code);
                if CALLS.iter().any(|call| writes_a_colour(code, call)) {
                    let line = index + 1;
                    self.found
                        .push((path.clone(), line, line_text(&source, line)));
                }
            }
        }
    }
}

fn line_text(source: &str, line: usize) -> String {
    source
        .lines()
        .nth(line - 1)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// The constructors that take a colour as a number rather than reading one from the theme.
const CALLS: [&str; 4] = ["rgb(", "rgba(", "hsla(", "Rgba("];

/// Whether `code` calls one of them. The character in front has to be checked, because
/// `to_rgb()` ends in the same letters and writes no colour at all.
fn writes_a_colour(code: &str, call: &str) -> bool {
    code.match_indices(call).any(|(at, _)| {
        code[..at]
            .chars()
            .next_back()
            .is_none_or(|before| !before.is_alphanumeric() && before != '_')
    })
}
