//! Standalone runner — opens the Diagram Editor in a bare window without the
//! Pulsar engine. Useful for UI development and iteration.
//!
//! Run with:
//!   cargo run --example standalone

use gpui::*;
use plugin_diagrams::DiagramsEditorPanel;
use ui::{Assets, Root, Theme, ThemeMode};

fn main() {
    Application::new().with_assets(Assets).run(|cx: &mut App| {
        // Initialise UI component registry (buttons, icons, themes, …)
        ui::init(cx);
        ui::themes::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point { x: px(80.0),    y: px(80.0)    },
                    size:   Size  { width: px(1440.0), height: px(900.0) },
                })),
                titlebar: Some(TitlebarOptions {
                    title: Some("Diagram Editor".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |window, cx| {
                let panel = cx.new(|cx| DiagramsEditorPanel::new(cx));
                cx.new(|cx| Root::new(panel.into(), window, cx))
            },
        )
        .expect("Failed to open window");
    });
}
