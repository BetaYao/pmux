//! app_root_render.rs — Render helper methods for AppRoot.
//!
//! Extracted from app_root.rs to reduce file size.
//! These are `impl AppRoot` methods that build UI sub-components.

use crate::agent_status::AgentStatus;
use crate::config::Config;
use crate::input::{key_to_xterm_escape, KeyModifiers};
use crate::remotes::secrets::Secrets;
use crate::runtime::backends::{kill_tmux_window, resolve_backend, session_name_for_workspace};
use crate::split_tree::SplitNode;
use crate::ui::app_root::{build_paste_text_from_clipboard, AppRoot};
use crate::ui::close_tab_dialog_ui::CloseTabDialogUi;
use crate::ui::delete_worktree_dialog_ui::DeleteWorktreeDialogUi;
use crate::ui::models::NotificationPanelModel;
use crate::ui::sidebar::Sidebar;
use crate::ui::split_pane_container::SplitPaneContainer;
use crate::ui::status_bar::StatusBar;
use crate::ui::terminal_area_entity::TerminalAreaEntity;
use crate::ui::terminal_view::TerminalBuffer;
use crate::ui::topbar_entity::TopBarEntity;
use crate::ui::workspace_tabbar::WorkspaceTabBar;
use crate::deps;
use gpui::prelude::FluentBuilder;
use gpui::prelude::*;
use gpui::{AnyElement, App, ClickEvent, ClipboardItem, Div, Entity, FocusHandle, FontWeight, KeyDownEvent, SharedString, Stateful, StyleRefinement, Window, div, px, rgb, rgba, svg, uniform_list, ScrollStrategy, UniformListScrollHandle, font};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;

impl AppRoot {
    pub(crate) fn handle_search_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        match event.keystroke.key.as_str() {
            "escape" => {
                self.search_active = false;
                self.search_query.clear();
                if let Some(ref e) = self.terminal_area_entity {
                    let _ = cx.update_entity(e, |ent: &mut TerminalAreaEntity, cx| {
                        ent.set_search(None, 0);
                        cx.notify();
                    });
                }
                cx.notify();
                true
            }
            "enter" | "g" if event.keystroke.modifiers.platform || event.keystroke.key == "enter" => {
                if let Ok(buffers) = self.terminal_buffers.lock() {
                    if let Some(target) = self.active_pane_target.as_ref() {
                        if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                            let matches = terminal.search(&self.search_query);
                            if !matches.is_empty() {
                                self.search_current_match =
                                    (self.search_current_match + 1) % matches.len();
                                if let Some(ref e) = self.terminal_area_entity {
                                    let _ = cx.update_entity(e, |ent: &mut TerminalAreaEntity, cx| {
                                        ent.set_search(
                                            Some(self.search_query.clone()),
                                            self.search_current_match,
                                        );
                                        cx.notify();
                                    });
                                }
                            }
                        }
                    }
                }
                cx.notify();
                true
            }
            "backspace" => {
                self.search_query.pop();
                if let Some(ref e) = self.terminal_area_entity {
                    let query = self.search_query.clone();
                    let _ = cx.update_entity(e, |ent: &mut TerminalAreaEntity, cx| {
                        ent.set_search(
                            if query.is_empty() { None } else { Some(query) },
                            self.search_current_match,
                        );
                        cx.notify();
                    });
                }
                cx.notify();
                true
            }
            _ => {
                if event.keystroke.key.len() == 1 {
                    let ch = event.keystroke.key.chars().next().unwrap();
                    if ch.is_ascii_graphic() || ch == ' ' {
                        self.search_query.push(ch);
                        if let Some(ref e) = self.terminal_area_entity {
                            let query = self.search_query.clone();
                            let _ = cx.update_entity(e, |ent: &mut TerminalAreaEntity, cx| {
                                ent.set_search(Some(query), self.search_current_match);
                                cx.notify();
                            });
                        }
                        cx.notify();
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Handle Cmd+key application shortcuts.
    pub(crate) fn handle_shortcut(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "b" => {
                self.sidebar_visible = !self.sidebar_visible;
                let visible = self.sidebar_visible;
                if let Some(ref e) = self.topbar_entity {
                    let _ = cx.update_entity(e, |t: &mut TopBarEntity, cx| {
                        t.set_sidebar_visible(visible);
                        cx.notify();
                    });
                }
                cx.notify();
            }
            "f" => {
                self.search_active = true;
                self.search_query.clear();
                self.search_current_match = 0;
                if let Some(ref e) = self.terminal_area_entity {
                    let _ = cx.update_entity(e, |ent: &mut TerminalAreaEntity, cx| {
                        ent.set_search(Some(String::new()), 0);
                        cx.notify();
                    });
                }
                cx.notify();
            }
            "i" => {
                if let Some(ref model) = self.notification_panel_model {
                    let _ = cx.update_entity(model, |m, cx| {
                        m.toggle_panel();
                        cx.notify();
                    });
                }
            }
            "d" => {
                if event.keystroke.modifiers.shift {
                    self.handle_split_pane(false, cx);
                } else {
                    self.handle_split_pane(true, cx);
                }
            }
            "r" => {
                self.open_diff_view(cx);
            }
            "w" => {
                if self.diff_view_entity.is_some() {
                    self.diff_view_entity = None;
                    cx.notify();
                } else {
                    self.handle_close_pane(cx);
                }
            }
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" => {
                if let Ok(idx) = event.keystroke.key.parse::<usize>() {
                    let idx = idx - 1;
                    if idx < self.workspace_manager.tab_count() {
                        self.handle_workspace_tab_switch(idx, cx);
                        let counts = self.compute_per_tab_active_counts();
                        if let Some(ref e) = self.topbar_entity {
                            let topbar = e.clone();
                            let wm = self.workspace_manager.clone();
                            let _ = cx.update_entity(&topbar, |t: &mut TopBarEntity, cx| {
                                t.set_workspace_manager(wm);
                                t.set_per_tab_active_counts(counts);
                                cx.notify();
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Forward a key event to the terminal runtime (xterm escape sequences, IME handling).
    pub(crate) fn forward_key_to_terminal(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key_name = event.keystroke.key.clone();
        let modifiers = KeyModifiers {
            platform: event.keystroke.modifiers.platform,
            shift: event.keystroke.modifiers.shift,
            alt: event.keystroke.modifiers.alt,
            ctrl: event.keystroke.modifiers.control,
        };

        match (&self.runtime, self.active_pane_target.as_ref()) {
            (Some(runtime), Some(target)) => {
                // IME: defer Enter so replace_text_in_range can send committed text first
                if (key_name == "enter" || key_name == "return" || key_name == "kp_enter")
                    && !modifiers.shift
                    && !modifiers.platform
                    && !modifiers.alt
                {
                    self.ime_pending_enter.store(true, Ordering::SeqCst);
                    let runtime = runtime.clone();
                    let target = target.clone();
                    let pending = self.ime_pending_enter.clone();
                    cx.spawn(async move |_entity, cx| {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(50))
                            .await;
                        if pending.swap(false, Ordering::SeqCst) {
                            let _ = runtime.send_input(&target, b"\r");
                        }
                    })
                    .detach();
                    return;
                }

                let bytes_opt = if let Ok(buffers) = self.terminal_buffers.lock() {
                    if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                        crate::terminal::key_to_bytes(event, terminal.mode())
                    } else {
                        None
                    }
                } else {
                    None
                };

                let has_text_char = event.keystroke.key_char.as_ref().is_some_and(|c| !c.is_empty());
                let bytes_opt = if has_text_char {
                    bytes_opt
                } else {
                    bytes_opt.or_else(|| key_to_xterm_escape(&key_name, modifiers))
                };

                if let Some(bytes) = bytes_opt {
                    if let Ok(buffers) = self.terminal_buffers.lock() {
                        if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                            if terminal.display_offset() > 0 {
                                terminal.scroll_to_bottom();
                            }
                        }
                    }
                    let send_result = runtime.send_input(target, &bytes);
                    if let Err(e) = send_result {
                        eprintln!("pmux: send_input failed: {}", e);
                    }
                }
            }
            _ => {
                if !modifiers.platform {
                    eprintln!(
                        "pmux: key '{}' not forwarded (runtime={} target={})",
                        key_name,
                        self.runtime.is_some(),
                        self.active_pane_target.as_deref().unwrap_or("none")
                    );
                }
            }
        }
    }


    pub(crate) fn render_dependency_check_page(
        &self,
        deps: &deps::DependencyCheckResult,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let missing: Vec<String> = deps.missing.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(24.))
            .bg(rgb(0x1e1e1e))
            .child(
                div()
                    .text_size(px(24.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff))
                    .child("Dependency Check")
            )
            .child(
                div()
                    .text_size(px(14.))
                    .text_color(rgb(0x999999))
                    .child("pmux requires the following dependencies. Please install any missing items:")
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .max_w(px(480.))
                    .children(missing.into_iter().map(|cmd| {
                        let install = deps::installation_instructions(&cmd);
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .px(px(16.))
                            .py(px(12.))
                            .rounded(px(6.))
                            .bg(rgb(0x2a2a2a))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .text_color(rgb(0xff6666))
                                            .child("✗ ")
                                    )
                                    .child(
                                        div()
                                            .text_color(rgb(0xffffff))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(cmd.clone())
                                    )
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0xaaaaaa))
                                    .font_family("ui-monospace")
                                    .child(install)
                            )
                    }))
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0x888888))
                    .child("After installing, click the button below to recheck")
            )
            .child(
                div()
                    .id("recheck-deps-btn")
                    .px(px(24.))
                    .py(px(12.))
                    .rounded(px(6.))
                    .bg(rgb(0x0066cc))
                    .text_color(rgb(0xffffff))
                    .text_size(px(15.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .hover(|style: StyleRefinement| style.bg(rgb(0x0077dd)))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        let result = deps::check_dependencies_detailed();
                        if result.is_ok() {
                            this.dependency_check = None;
                        } else {
                            this.dependency_check = Some(result);
                        }
                        cx.notify();
                    }))
                    .child("Recheck")
            )
    }

    pub(crate) fn render_startup_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let has_error = self.state.error_message.is_some();
        let error_msg = self.state.error_message.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(20.))
            .bg(rgb(0x1e1e1e))
            .child(
                div()
                    .text_size(px(28.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff))
                    .child("Welcome to pmux")
            )
            .child(
                div()
                    .text_size(px(14.))
                    .text_color(rgb(0x999999))
                    .child("Select a Git repository to manage your AI agents")
            )
            .child(
                div()
                    .id("select-workspace-btn")
                    .px(px(24.))
                    .py(px(12.))
                    .rounded(px(6.))
                    .bg(rgb(0x0066cc))
                    .text_color(rgb(0xffffff))
                    .text_size(px(15.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .hover(|style: StyleRefinement| style.bg(rgb(0x0077dd)))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.handle_add_workspace(cx);
                    }))
                    .child("Select Workspace")
            )
            .when(has_error, |el: Div| {
                if let Some(msg) = error_msg {
                    el.child(
                        div()
                            .px(px(16.))
                            .py(px(8.))
                            .rounded(px(4.))
                            .bg(rgb(0x3a1111))
                            .text_color(rgb(0xff4444))
                            .text_size(px(13.))
                            .max_w(px(400.))
                            .child(SharedString::from(msg))
                    )
                } else {
                    el
                }
            })
    }

    /// Render the update available/downloading banner (if any).
    pub(crate) fn render_update_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.update_available.is_some() && !self.update_downloading {
            let version = self.update_available.as_ref().map(|i| i.latest_version.display()).unwrap_or_default();
            Some(
                div()
                    .id("update-banner")
                    .w_full()
                    .h(px(28.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .gap(px(12.))
                    .bg(rgb(0x1a3a2a))
                    .border_t_1()
                    .border_color(rgb(0x2d5f3f))
                    .text_size(px(12.))
                    .text_color(rgb(0x4ec9b0))
                    .child(format!("pmux {} is available", version))
                    .child(
                        div()
                            .id("update-now-btn")
                            .px(px(12.))
                            .py(px(2.))
                            .rounded(px(3.))
                            .bg(rgb(0x0e7a0d))
                            .text_color(rgb(0xffffff))
                            .text_size(px(11.))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x12991e)))
                            .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                                this.trigger_update(cx);
                            }))
                            .child("Update Now"),
                    )
                    .child(
                        div()
                            .id("update-later-btn")
                            .px(px(8.))
                            .py(px(2.))
                            .cursor_pointer()
                            .text_color(rgb(0x888888))
                            .text_size(px(11.))
                            .hover(|s| s.text_color(rgb(0xcccccc)))
                            .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                                this.update_available = None;
                                cx.notify();
                            }))
                            .child("Later"),
                    )
                    .child(
                        div()
                            .id("update-skip-btn")
                            .px(px(8.))
                            .py(px(2.))
                            .cursor_pointer()
                            .text_color(rgb(0x666666))
                            .text_size(px(11.))
                            .hover(|s| s.text_color(rgb(0xaaaaaa)))
                            .on_click(cx.listener(|this, _event: &ClickEvent, _window, cx| {
                                this.skip_update_version();
                                cx.notify();
                            }))
                            .child("Skip"),
                    )
                    .into_any_element()
            )
        } else if self.update_downloading {
            Some(
                div()
                    .id("update-progress-banner")
                    .w_full()
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(0x1a2a3a))
                    .border_t_1()
                    .border_color(rgb(0x2d4f6f))
                    .text_size(px(12.))
                    .text_color(rgb(0x6cb6ff))
                    .child("Downloading update...")
                    .into_any_element()
            )
        } else {
            None
        }
    }

    /// Build the terminal content area (loading state, terminal entity, or split pane container).
    pub(crate) fn build_terminal_content_area(
        &self,
        cx: &mut Context<Self>,
        terminal_focus: &FocusHandle,
        repo_name: &str,
        split_tree: SplitNode,
        terminal_buffers: Arc<Mutex<HashMap<String, TerminalBuffer>>>,
        focused_pane_index: usize,
        split_divider_drag: Option<(Vec<bool>, f32, f32, bool)>,
        _worktree_switch_loading: Option<usize>,
        cursor_blink_visible: bool,
    ) -> Div {
        let app_root_entity = cx.entity();
        let app_root_entity_for_ratio = app_root_entity.clone();
        let app_root_entity_for_drag = app_root_entity.clone();
        let app_root_entity_for_drag_end = app_root_entity.clone();
        let app_root_entity_for_pane_click = app_root_entity.clone();
        let terminal_focus_for_pane = terminal_focus.clone();
        div()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .cursor(gpui::CursorStyle::IBeam)
            .relative()
            .child(
                if let Some(ref term_entity) = self.terminal_area_entity {
                    div().size_full().child(term_entity.clone()).into_any_element()
                } else {
                    SplitPaneContainer::new(
                        split_tree,
                        terminal_buffers,
                        focused_pane_index,
                        repo_name,
                    )
                    .with_cursor_blink_visible(cursor_blink_visible)
                    .with_drag_state(split_divider_drag)
                    .with_search(
                        if self.search_active { Some(self.search_query.clone()) } else { None },
                        self.search_current_match,
                    )
                    .on_ratio_change(move |path, ratio, _window, cx| {
                        let _ = cx.update_entity(&app_root_entity_for_ratio, |this: &mut AppRoot, cx| {
                            this.split_tree.update_ratio(&path, ratio);
                            cx.notify();
                        });
                    })
                    .on_divider_drag_start(move |path, pos, ratio, is_vertical, _window, cx| {
                        let _ = cx.update_entity(&app_root_entity_for_drag, |this: &mut AppRoot, cx| {
                            this.split_divider_drag = Some((path, pos, ratio, is_vertical));
                            cx.notify();
                        });
                    })
                    .on_divider_drag_end(move |_window, cx| {
                        let _ = cx.update_entity(&app_root_entity_for_drag_end, |this: &mut AppRoot, cx| {
                            this.split_divider_drag = None;
                            cx.notify();
                        });
                    })
                    .on_pane_click(move |pane_idx, window, cx| {
                        let _ = cx.update_entity(&app_root_entity_for_pane_click, |this: &mut AppRoot, cx| {
                            this.focused_pane_index = pane_idx;
                            if let Some(target) = this.split_tree.focus_index_to_pane_target(pane_idx) {
                                if let Some(rt) = &this.runtime {
                                    let _ = rt.focus_pane(&target);
                                }
                                this.active_pane_target = Some(target.clone());
                                if let Ok(mut guard) = this.active_pane_target_shared.lock() {
                                    *guard = target.clone();
                                }
                                this.terminal_needs_focus = false;
                                if let Ok(buffers) = this.terminal_buffers.lock() {
                                    if let Some(TerminalBuffer::Terminal { focus_handle, .. }) = buffers.get(&target) {
                                        window.focus(focus_handle, cx);
                                    } else {
                                        drop(buffers);
                                        window.focus(&terminal_focus_for_pane, cx);
                                    }
                                } else {
                                    window.focus(&terminal_focus_for_pane, cx);
                                }
                            } else {
                                this.terminal_needs_focus = true;
                            }
                            cx.notify();
                        });
                    })
                    .into_any_element()
                }
            )
            .when(self.search_active, |el| {
                el.child(
                    div()
                        .absolute()
                        .top(px(2.0))
                        .right(px(12.0))
                        .bg(rgb(0x2e343e))
                        .border_1()
                        .border_color(rgb(0x5c6370))
                        .rounded(px(4.0))
                        .px(px(8.0))
                        .py(px(4.0))
                        .child(format!("🔍 {}_", self.search_query))
                )
            })
    }

    /// Build the sidebar right-click context menu (View Diff, Delete).
    pub(crate) fn build_sidebar_context_menu(
        &self,
        cx: &mut Context<Self>,
        idx: usize,
        repo_path: &std::path::Path,
        cached_worktrees: &[crate::worktree::WorktreeInfo],
    ) -> impl IntoElement {
        let app_root_entity = cx.entity();
        let on_view_diff: Option<Arc<dyn Fn(usize, &mut Window, &mut App) + Send + Sync>> = {
            let entity = app_root_entity.clone();
            let repo_path = repo_path.to_path_buf();
            Some(Arc::new(move |idx, _window, cx| {
                let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx| {
                    this.sidebar_context_menu = None;
                    cx.notify();
                });
                let entity2 = entity.clone();
                let repo_path2 = repo_path.clone();
                cx.spawn(async move |cx| {
                    let result = blocking::unblock(move || {
                        crate::worktree::discover_worktrees(&repo_path2).ok().map(|wt| (wt, repo_path2))
                    }).await;
                    let _ = cx.update_entity(&entity2, |this: &mut AppRoot, cx: &mut _| {
                        if let Some((wt, rp)) = result {
                            this.cached_worktrees = wt;
                            this.cached_worktrees_repo = Some(rp);
                        }
                        this.open_diff_view_for_worktree_with_cache(idx, cx);
                    });
                }).detach();
            }))
        };
        let on_delete: Option<Arc<dyn Fn(usize, &mut Window, &mut App) + Send + Sync>> = {
            let entity = app_root_entity.clone();
            let repo_path = repo_path.to_path_buf();
            Some(Arc::new(move |idx, _window, cx| {
                let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx| {
                    this.sidebar_context_menu = None;
                    cx.notify();
                });
                let entity2 = entity.clone();
                let repo_path2 = repo_path.clone();
                cx.spawn(async move |cx| {
                    let result = blocking::unblock(move || {
                        let worktrees = crate::worktree::discover_worktrees(&repo_path2).ok()?;
                        let worktree = worktrees.get(idx).cloned()?;
                        let has_uncommitted = crate::worktree::has_uncommitted_changes(&worktree.path);
                        Some((worktrees, worktree, has_uncommitted, repo_path2))
                    }).await;
                    if let Some((worktrees, worktree, has_uncommitted, rp)) = result {
                        let _ = cx.update_entity(&entity2, |this: &mut AppRoot, cx: &mut _| {
                            this.cached_worktrees = worktrees;
                            this.cached_worktrees_repo = Some(rp);
                            this.delete_worktree_dialog.open(worktree, has_uncommitted);
                            cx.notify();
                        });
                    }
                }).detach();
            }))
        };
        Sidebar::render_context_menu(idx, on_view_diff, on_delete, cached_worktrees)
    }

    /// Build the terminal right-click context menu (Copy, Paste, Select All, Clear).
    pub(crate) fn build_terminal_context_menu(&self, cx: &mut Context<Self>, has_selection: bool) -> Stateful<Div> {
        let app_root_entity = cx.entity();
        let app_root_for_copy = app_root_entity.clone();
        let app_root_for_paste = app_root_entity.clone();
        let app_root_for_select_all = app_root_entity.clone();
        let app_root_for_clear = app_root_entity.clone();

        let mut menu = div()
            .id("terminal-context-menu")
            .min_w(px(180.))
            .py(px(4.))
            .rounded(px(6.))
            .bg(rgb(0x282828))
            .border_1().border_color(rgb(0x404040))
            .shadow_lg()
            .occlude()
            .on_click(|_event, _window, cx| { cx.stop_propagation(); })
            .flex().flex_col();

        // Copy
        if has_selection {
            menu = menu.child(
                div()
                    .id("term-ctx-copy")
                    .mx(px(4.)).px(px(8.)).py(px(6.))
                    .rounded(px(4.))
                    .flex().flex_row().items_center().gap(px(8.))
                    .text_size(px(13.)).text_color(rgb(0xdddddd))
                    .hover(|s: StyleRefinement| s.bg(rgb(0x3a3a3a)).text_color(rgb(0xffffff)))
                    .cursor_pointer()
                    .on_click(move |_event, _window, cx| {
                        let _ = cx.update_entity(&app_root_for_copy, |this: &mut AppRoot, cx| {
                            if let Some(ref target) = this.active_pane_target {
                                if let Ok(buffers) = this.terminal_buffers.lock() {
                                    if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                                        if let Some(text) = terminal.selection_text() {
                                            if !text.is_empty() {
                                                cx.write_to_clipboard(ClipboardItem::new_string(text));
                                            }
                                        }
                                    }
                                }
                            }
                            this.terminal_context_menu = None;
                            cx.notify();
                        });
                    })
                    .child(svg().path("icons/copy.svg").size(px(15.)).flex_shrink_0().text_color(rgb(0xaaaaaa)))
                    .child(div().flex_1().child("Copy"))
                    .child(div().text_size(px(11.)).text_color(rgb(0x888888)).child("⌘C"))
            );
        } else {
            menu = menu.child(
                div()
                    .id("term-ctx-copy")
                    .mx(px(4.)).px(px(8.)).py(px(6.))
                    .rounded(px(4.))
                    .flex().flex_row().items_center().gap(px(8.))
                    .text_size(px(13.)).text_color(rgb(0x666666))
                    .child(svg().path("icons/copy.svg").size(px(15.)).flex_shrink_0().text_color(rgb(0x555555)))
                    .child(div().flex_1().child("Copy"))
                    .child(div().text_size(px(11.)).text_color(rgb(0x555555)).child("⌘C"))
            );
        }

        // Paste
        menu = menu.child(
            div()
                .id("term-ctx-paste")
                .mx(px(4.)).px(px(8.)).py(px(6.))
                .rounded(px(4.))
                .flex().flex_row().items_center().gap(px(8.))
                .text_size(px(13.)).text_color(rgb(0xdddddd))
                .hover(|s: StyleRefinement| s.bg(rgb(0x3a3a3a)).text_color(rgb(0xffffff)))
                .cursor_pointer()
                .on_click(move |_event, _window, cx| {
                    let _ = cx.update_entity(&app_root_for_paste, |this: &mut AppRoot, cx| {
                        if let Some(clipboard) = cx.read_from_clipboard() {
                            let text = build_paste_text_from_clipboard(&clipboard);
                            if !text.is_empty() {
                                if let (Some(runtime), Some(target)) = (&this.runtime, this.active_pane_target.as_ref()) {
                                    let bracketed = if let Ok(buffers) = this.terminal_buffers.lock() {
                                        if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                                            if terminal.display_offset() > 0 {
                                                terminal.scroll_to_bottom();
                                            }
                                            terminal.mode().contains(alacritty_terminal::term::TermMode::BRACKETED_PASTE)
                                        } else { false }
                                    } else { false };
                                    let mut bytes = Vec::with_capacity(text.len() + 12);
                                    if bracketed { bytes.extend_from_slice(b"\x1b[200~"); }
                                    bytes.extend_from_slice(text.replace('\n', "\r").as_bytes());
                                    if bracketed { bytes.extend_from_slice(b"\x1b[201~"); }
                                    let _ = runtime.send_input(target, &bytes);
                                }
                            }
                        }
                        this.terminal_context_menu = None;
                        cx.notify();
                    });
                })
                .child(svg().path("icons/paste.svg").size(px(15.)).flex_shrink_0().text_color(rgb(0xaaaaaa)))
                .child(div().flex_1().child("Paste"))
                .child(div().text_size(px(11.)).text_color(rgb(0x888888)).child("⌘V"))
        );

        // Separator
        menu = menu.child(div().mx(px(4.)).my(px(2.)).h(px(1.)).bg(rgb(0x3a3a3a)));

        // Select All
        menu = menu.child(
            div()
                .id("term-ctx-select-all")
                .mx(px(4.)).px(px(8.)).py(px(6.))
                .rounded(px(4.))
                .flex().flex_row().items_center().gap(px(8.))
                .text_size(px(13.)).text_color(rgb(0xdddddd))
                .hover(|s: StyleRefinement| s.bg(rgb(0x3a3a3a)).text_color(rgb(0xffffff)))
                .cursor_pointer()
                .on_click(move |_event, _window, cx| {
                    let _ = cx.update_entity(&app_root_for_select_all, |this: &mut AppRoot, cx| {
                        if let Some(ref target) = this.active_pane_target {
                            if let Ok(buffers) = this.terminal_buffers.lock() {
                                if let Some(TerminalBuffer::Terminal { terminal, .. }) = buffers.get(target) {
                                    terminal.select_all();
                                }
                            }
                        }
                        this.terminal_context_menu = None;
                        cx.notify();
                    });
                })
                .child(svg().path("icons/select-all.svg").size(px(15.)).flex_shrink_0().text_color(rgb(0xaaaaaa)))
                .child(div().flex_1().child("Select All"))
                .child(div().text_size(px(11.)).text_color(rgb(0x888888)).child("⌘A"))
        );

        // Clear
        menu = menu.child(
            div()
                .id("term-ctx-clear")
                .mx(px(4.)).px(px(8.)).py(px(6.))
                .rounded(px(4.))
                .flex().flex_row().items_center().gap(px(8.))
                .text_size(px(13.)).text_color(rgb(0xdddddd))
                .hover(|s: StyleRefinement| s.bg(rgb(0x3a3a3a)).text_color(rgb(0xffffff)))
                .cursor_pointer()
                .on_click(move |_event, _window, cx| {
                    let _ = cx.update_entity(&app_root_for_clear, |this: &mut AppRoot, cx| {
                        if let (Some(runtime), Some(target)) = (&this.runtime, this.active_pane_target.as_ref()) {
                            let _ = runtime.send_input(target, b"\x0c");
                        }
                        this.terminal_context_menu = None;
                        cx.notify();
                    });
                })
                .child(svg().path("icons/clear.svg").size(px(15.)).flex_shrink_0().text_color(rgb(0xaaaaaa)))
                .child(div().flex_1().child("Clear"))
                .child(div().text_size(px(11.)).text_color(rgb(0x888888)).child("⌘K"))
        );

        menu
    }

    /// Build the delete worktree confirmation dialog with callbacks.
    pub(crate) fn build_delete_dialog(&self, cx: &mut Context<Self>) -> DeleteWorktreeDialogUi {
        let app_root_entity = cx.entity();
        let app_root_entity_for_confirm = app_root_entity.clone();
        let app_root_entity_for_cancel = app_root_entity.clone();
        let mut dialog = DeleteWorktreeDialogUi::new()
            .on_confirm(move |wt, _window, cx| {
                let _ = cx.update_entity(&app_root_entity_for_confirm, |this: &mut AppRoot, cx| {
                    this.confirm_delete_worktree(wt, cx);
                });
            })
            .on_cancel(move |_window, cx| {
                let _ = cx.update_entity(&app_root_entity_for_cancel, |this: &mut AppRoot, cx| {
                    this.close_delete_dialog(cx);
                });
            });
        if self.delete_worktree_dialog.is_open() {
            if let Some(wt) = self.delete_worktree_dialog.worktree() {
                dialog.open(wt.clone(), self.delete_worktree_dialog.has_uncommitted());
            }
        }
        if let Some(err) = self.delete_worktree_dialog.error_message() {
            dialog.set_error(err);
        }
        dialog
    }

    /// Build the close tab confirmation dialog with callbacks.
    pub(crate) fn build_close_tab_dialog(&self, cx: &mut Context<Self>) -> CloseTabDialogUi {
        let app_root_entity = cx.entity();
        let app_root_entity_for_confirm = app_root_entity.clone();
        let app_root_entity_for_cancel = app_root_entity.clone();
        let app_root_entity_for_toggle = app_root_entity.clone();
        let mut dialog = CloseTabDialogUi::new()
            .on_confirm(move |tab_index, kill_tmux, _window, cx| {
                let _ = cx.update_entity(&app_root_entity_for_confirm, |this: &mut AppRoot, cx| {
                    this.confirm_close_tab(tab_index, kill_tmux, cx);
                });
            })
            .on_cancel(move |_window, cx| {
                let _ = cx.update_entity(&app_root_entity_for_cancel, |this: &mut AppRoot, cx| {
                    this.close_close_tab_dialog(cx);
                });
            })
            .on_toggle_kill_tmux(move |_window, cx| {
                let _ = cx.update_entity(&app_root_entity_for_toggle, |this: &mut AppRoot, cx| {
                    this.toggle_close_tab_kill_tmux(cx);
                });
            });
        if self.close_tab_dialog.is_open() {
            if let (Some(idx), Some(path), Some(name)) = (
                self.close_tab_dialog.tab_index(),
                self.close_tab_dialog.workspace_path().cloned(),
                self.close_tab_dialog.workspace_name().map(|s| s.to_string()),
            ) {
                dialog.open(idx, path, name);
                if !self.close_tab_dialog.kill_tmux() {
                    dialog.toggle_kill_tmux();
                }
            }
        }
        dialog
    }

    /// Build the sidebar component with all callbacks wired.
    pub(crate) fn build_sidebar(
        &self,
        cx: &mut Context<Self>,
        repo_name: &str,
        repo_path: &std::path::Path,
        terminal_focus: &gpui::FocusHandle,
    ) -> Sidebar {
        let app_root_entity = cx.entity();
        let pane_statuses = self.pane_statuses.clone();
        let pane_summaries_data = self.pane_summary_model.as_ref()
            .map(|m| m.read(cx).summaries().clone())
            .unwrap_or_default();
        let running_frame = self.running_animation_frame;
        let notification_unread = self
            .notification_panel_model
            .as_ref()
            .map(|m| m.read(cx).unread_count)
            .unwrap_or_else(|| self.notification_manager.lock().map(|m| m.unread_count()).unwrap_or(0));
        let scheduled_tasks = self.scheduler_manager.as_ref()
            .map(|m| m.read(cx).tasks().to_vec())
            .unwrap_or_default();
        let notification_panel_model_for_toggle = self.notification_panel_model.clone();
        let app_root_entity_for_toggle = app_root_entity.clone();
        let app_root_entity_for_add_ws = app_root_entity.clone();

        let mut sidebar = Sidebar::new(repo_name, repo_path.to_path_buf())
            .with_statuses(pane_statuses.clone())
            .with_pane_summaries(pane_summaries_data)
            .with_running_frame(running_frame)
            .with_context_menu(self.sidebar_context_menu)
            .with_tasks_expanded(self.tasks_expanded)
            .on_toggle_sidebar(move |_window, cx| {
                let _ = cx.update_entity(&app_root_entity_for_toggle, |this: &mut AppRoot, cx| {
                    this.sidebar_visible = !this.sidebar_visible;
                    let visible = this.sidebar_visible;
                    if let Some(ref e) = this.topbar_entity {
                        let _ = cx.update_entity(e, |t: &mut TopBarEntity, cx| {
                            t.set_sidebar_visible(visible);
                            cx.notify();
                        });
                    }
                    cx.notify();
                });
            })
            .on_toggle_notifications(move |_window, cx| {
                if let Some(ref model) = notification_panel_model_for_toggle {
                    let _ = cx.update_entity(model, |m, cx| {
                        m.toggle_panel();
                        cx.notify();
                    });
                }
            })
            .on_add_workspace(move |_window, cx| {
                let _ = cx.update_entity(&app_root_entity_for_add_ws, |this: &mut AppRoot, cx| {
                    this.handle_add_workspace(cx);
                });
            })
            .with_notification_count(notification_unread);

        // Use cached worktrees (never call git in render)
        let worktrees = self.worktrees_for_render(&repo_path).to_vec();
        if !worktrees.is_empty() {
            sidebar.set_worktrees(worktrees);
            if let Some(idx) = self.active_worktree_index {
                if idx < sidebar.worktree_count() {
                    sidebar.select(idx);
                }
            } else {
                sidebar.select(0);
            }
        }
        let orphan_windows = self.orphan_tmux_windows_for_repo(&repo_path);
        sidebar.set_orphan_windows(orphan_windows);
        sidebar.set_scheduled_tasks(scheduled_tasks);

        // Set up select callback
        let app_root_entity_for_sidebar = app_root_entity.clone();
        let terminal_focus_for_select = terminal_focus.clone();
        sidebar.on_select(move |idx: usize, window: &mut Window, cx: &mut App| {
            let _ = cx.update_entity(&app_root_entity_for_sidebar, |this: &mut AppRoot, cx| {
                this.pending_worktree_selection = Some(idx);
                this.process_pending_worktree_selection(cx);
                cx.notify();
            });
            // Clicking the sidebar may defocus the terminal. Restore focus immediately
            // so keyboard input works without waiting for the async switch to complete.
            let focus = terminal_focus_for_select.clone();
            window.on_next_frame(move |window, cx| {
                window.focus(&focus, cx);
            });
        });

        // Set up New Branch callback - opens the dialog
        let app_root_entity_for_new_branch = app_root_entity.clone();
        let dialog_focus = self.dialog_input_focus.clone();
        sidebar.on_new_branch(move |window, cx| {
            let _ = cx.update_entity(&app_root_entity_for_new_branch, |this: &mut AppRoot, cx| {
                this.open_new_branch_dialog(cx);
            });
            // Double on_next_frame so dialog DOM (and focusable input) is fully mounted before focus
            if let Some(ref focus) = dialog_focus {
                let focus = focus.clone();
                window.on_next_frame(move |window, _cx| {
                    let focus = focus.clone();
                    window.on_next_frame(move |window, cx| {
                        window.focus(&focus, cx);
                    });
                });
            }
        });

        // Set up Refresh callback - refreshes worktree list
        let app_root_entity_for_refresh = app_root_entity.clone();
        sidebar.on_refresh(move |_window, cx| {
            let _ = cx.update_entity(&app_root_entity_for_refresh, |this: &mut AppRoot, cx| {
                if let Some(repo_path) = this.workspace_manager.active_tab().map(|t| t.path.clone()) {
                    this.refresh_worktrees_for_repo(&repo_path);
                }
                cx.notify();
            });
        });

        // Set up Settings callback - opens the settings modal
        let app_root_entity_for_settings = app_root_entity.clone();
        let settings_focus_for_cb = self.settings_focus.clone().expect("settings_focus created in ensure_entities");
        sidebar.on_settings(move |window, cx| {
            let _ = cx.update_entity(&app_root_entity_for_settings, |this: &mut AppRoot, cx| {
                this.show_settings = true;
                this.settings_draft = Config::load().ok();
                this.settings_secrets_draft = Secrets::load().ok();
                // Sync to DialogManager
                if let Some(ref dm) = this.dialog_mgr {
                    let config = Config::load().unwrap_or_default();
                    let secrets = Secrets::load().unwrap_or_default();
                    dm.update(cx, |dm, cx| dm.open_settings(config, secrets, cx));
                }
                cx.notify();
            });
            // Focus settings overlay on next frame (after DOM is mounted)
            let focus = settings_focus_for_cb.clone();
            window.on_next_frame(move |window, cx| {
                window.focus(&focus, cx);
            });
        });

        let app_root_entity_for_delete = app_root_entity.clone();
        let app_root_entity_for_view_diff = app_root_entity.clone();
        let app_root_entity_for_right_click = app_root_entity.clone();
        let app_root_entity_for_close_orphan = app_root_entity.clone();
        let repo_path = repo_path.to_path_buf();
        let repo_path_for_delete = repo_path.clone();
        let repo_path_for_close_orphan = repo_path.clone();
        let repo_path_for_view_diff = repo_path.clone();
        // Extra clones for the root-level context menu overlay
        let app_root_entity_for_menu_delete = app_root_entity.clone();
        let app_root_entity_for_menu_diff = app_root_entity.clone();
        let repo_path_for_menu_delete = repo_path.clone();
        let repo_path_for_menu_diff = repo_path.clone();
        sidebar.on_delete(move |idx, _window, cx| {
            let _ = cx.update_entity(&app_root_entity_for_delete, |this: &mut AppRoot, cx| {
                this.sidebar_context_menu = None;
                cx.notify();
            });
            let repo_path = repo_path_for_delete.clone();
            let entity = app_root_entity_for_delete.clone();
            cx.spawn(async move |cx| {
                let result = blocking::unblock(move || {
                    let worktrees = crate::worktree::discover_worktrees(&repo_path).ok()?;
                    let worktree = worktrees.get(idx).cloned()?;
                    let has_uncommitted = crate::worktree::has_uncommitted_changes(&worktree.path);
                    Some((worktrees, worktree, has_uncommitted, repo_path))
                }).await;
                if let Some((worktrees, worktree, has_uncommitted, repo_path)) = result {
                    let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx: &mut _| {
                        this.cached_worktrees = worktrees;
                        this.cached_worktrees_repo = Some(repo_path);
                        this.delete_worktree_dialog.open(worktree, has_uncommitted);
                        cx.notify();
                    });
                }
            }).detach();
        });
        sidebar.on_close_orphan(move |window_name, _window, cx: &mut App| {
            let repo_path = repo_path_for_close_orphan.clone();
            let entity = app_root_entity_for_close_orphan.clone();
            let window_name = window_name.to_string();
            cx.spawn(async move |cx| {
                let _ = blocking::unblock(move || kill_tmux_window(&repo_path, &window_name)).await;
                let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx: &mut _| {
                    this.cached_tmux_windows = None;
                    cx.notify();
                });
            }).detach();
        });
        sidebar.on_view_diff(move |idx, _window, cx| {
            let _ = cx.update_entity(&app_root_entity_for_view_diff, |this: &mut AppRoot, cx| {
                this.sidebar_context_menu = None;
                cx.notify();
            });
            let entity = app_root_entity_for_view_diff.clone();
            let repo_path = repo_path_for_view_diff.clone();
            cx.spawn(async move |cx| {
                let result = blocking::unblock(move || {
                    crate::worktree::discover_worktrees(&repo_path).ok().map(|wt| (wt, repo_path))
                }).await;
                let _ = cx.update_entity(&entity, |this: &mut AppRoot, cx: &mut _| {
                    if let Some((wt, repo_path)) = result {
                        this.cached_worktrees = wt;
                        this.cached_worktrees_repo = Some(repo_path);
                    }
                    this.open_diff_view_for_worktree_with_cache(idx, cx);
                });
            }).detach();
        });
        sidebar.on_right_click(move |idx, x, y, _window, cx| {
            let _ = cx.update_entity(&app_root_entity_for_right_click, |this: &mut AppRoot, cx| {
                this.sidebar_context_menu = Some((idx, x, y));
                cx.notify();
            });
        });

        // Task callbacks
        let app_root_entity_for_toggle_task = app_root_entity.clone();
        let app_root_entity_for_run_task = app_root_entity.clone();
        let app_root_entity_for_add_task = app_root_entity.clone();
        sidebar = sidebar
            .on_toggle_task(move |id, _window, cx| {
                let _ = cx.update_entity(&app_root_entity_for_toggle_task, |_this: &mut AppRoot, _cx| {
                    // TODO: Toggle task enabled state
                });
            })
            .on_run_task(move |_id, _window, _cx| {
                // TODO: Run task immediately
            })
            .on_add_task(move |_window, _cx| {
                // TODO: Open add task dialog
            });

        sidebar
    }

}
