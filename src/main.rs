// main.rs - pmux GUI application using gpui
use std::path::PathBuf;

use gpui::{actions, point, px, size, AssetSource, TitlebarOptions, WindowBounds, WindowOptions, *};
use pmux::ui::app_root::AppRoot;
use pmux::window_state::PersistentAppState;

/// Set macOS notification bundle identifier before any notification.
/// Avoids get_bundle_identifier_or_default() triggering "Where is use_default?" dialog and freeze.
#[cfg(target_os = "macos")]
fn init_macos_notifications() {
    let _ = notify_rust::set_application("cn.mx5.pmux");
}

struct Assets {
    base: PathBuf,
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        std::fs::read(self.base.join(path))
            .map(|data| Some(std::borrow::Cow::Owned(data)))
            .map_err(Into::into)
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        std::fs::read_dir(self.base.join(path))
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok().and_then(|e| e.file_name().into_string().ok()))
                    .map(SharedString::from)
                    .collect()
            })
            .map_err(Into::into)
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    init_macos_notifications();

    let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
    gpui_platform::application()
        .with_assets(Assets { base: resources })
        .run(|cx: &mut App| {
        // Register menu actions
        cx.on_action(open_settings);
        cx.on_action(select_workspace_from_menu);
        cx.on_action(toggle_sidebar_from_menu);
        cx.on_action(open_help);

        // Set up macOS-style application menus
        cx.set_menus(vec![
            Menu {
                name: "Settings".into(),
                items: vec![MenuItem::action("Preferences…", OpenSettings)],
            },
            Menu {
                name: "File".into(),
                items: vec![MenuItem::action("Select Workspace…", SelectWorkspaceFromMenu)],
            },
            Menu {
                name: "Edit".into(),
                items: vec![],
            },
            Menu {
                name: "View".into(),
                items: vec![MenuItem::action("Toggle Sidebar", ToggleSidebarFromMenu)],
            },
            Menu {
                name: "Help".into(),
                items: vec![MenuItem::action("pmux Help", OpenHelp)],
            },
        ]);

        let bounds = {
            let state = PersistentAppState::load().unwrap_or_default();
            let ws = &state.window_state;
            let (w, h) = ws.size;
            let (x, y) = ws.position;
            if w > 0 && h > 0 {
                Bounds::new(
                    point(px(x as f32), px(y as f32)),
                    size(px(w as f32), px(h as f32)),
                )
            } else {
                Bounds::centered(None, size(px(900.), px(600.)), cx)
            }
        };
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: None,
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(12.), px(12.))),
                }),
                ..Default::default()
            },
            |window, cx| {
                // Register window close handler to save state and quit
                window.on_window_should_close(cx, |window, app| {
                    let mut state = PersistentAppState::load().unwrap_or_default();
                    let bounds = window.window_bounds().get_bounds();
                    state.window_state.size = (
                        (f32::from(bounds.size.width)).round().max(0.) as u32,
                        (f32::from(bounds.size.height)).round().max(0.) as u32,
                    );
                    state.window_state.position = (
                        (f32::from(bounds.origin.x)).round() as i32,
                        (f32::from(bounds.origin.y)).round() as i32,
                    );
                    if let Some(Some(root)) = window.root::<AppRoot>() {
                        root.update(app, |app_root, _| {
                            state.sidebar_width = app_root.sidebar_width();
                        });
                    }
                    let _ = state.save();
                    app.quit();
                    true // Allow the window to close
                });

                cx.new(|cx| {
                    let mut app_root = AppRoot::new();
                    app_root.init_workspace_restoration(cx);
                    app_root
                })
            },
        )
        .unwrap();

        cx.activate(true);
    });
}

// Application-wide menu actions (currently placeholders)
actions!(pmux_menus, [
    OpenSettings,
    SelectWorkspaceFromMenu,
    ToggleSidebarFromMenu,
    OpenHelp,
]);

fn open_settings(_: &OpenSettings, _cx: &mut App) {
    println!("Settings menu clicked - TODO: open settings UI");
}

fn select_workspace_from_menu(_: &SelectWorkspaceFromMenu, _cx: &mut App) {
    println!("File > Select Workspace… clicked - TODO: trigger workspace picker");
}

fn toggle_sidebar_from_menu(_: &ToggleSidebarFromMenu, _cx: &mut App) {
    println!("View > Toggle Sidebar clicked - TODO: toggle sidebar visibility");
}

fn open_help(_: &OpenHelp, _cx: &mut App) {
    println!("Help > pmux Help clicked - TODO: open documentation");
}
