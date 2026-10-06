//! Robokura: personal assistants that run on your own computer.

use gpui_kit::component::{ThemeMode, TitleBar};
use gpui_kit::{WindowBounds, WindowOptions, application, px, size};

use robokura::{theme, ui};

fn main() {
    // Diagnostics go to stderr and are off unless asked for. Nothing is collected
    // about the person.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("ROBOKURA_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            // The machine's own answer first, so the window has a theme of this product's
            // from the moment it exists. A mode the person has chosen is put over the top
            // once the store is open, in `ui::open`.
            theme::apply(cx, ThemeMode::from(cx.window_appearance()));

            // The bar's options set up everything the component needs, including
            // letting it own dragging and double clicking rather than the system.
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::centered(size(px(1100.), px(760.)), cx)),
                ..TitleBar::window_options()
            };

            gpui_kit::open_window(options, cx, ui::open).expect("failed to open window");
        });
}
