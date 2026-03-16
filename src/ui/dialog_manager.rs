//! DialogManager - manages all modal dialogs (new branch, delete worktree, close tab, settings)
//!
//! Extracted from AppRoot Phase 1 to reduce God Object complexity.
//! Communication: receives workspace info via setter methods, callbacks to AppRoot via closures.

use crate::config::Config;
use crate::remotes::secrets::Secrets;
use crate::ui::models::NewBranchDialogModel;
use crate::ui::new_branch_dialog_entity::NewBranchDialogEntity;
use crate::ui::close_tab_dialog_ui::CloseTabDialogUi;
use crate::ui::delete_worktree_dialog_ui::DeleteWorktreeDialogUi;
use gpui::prelude::FluentBuilder;
use gpui::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct DialogManager {
    // New Branch Dialog
    pub new_branch_dialog_model: Option<Entity<NewBranchDialogModel>>,
    pub new_branch_dialog_entity: Option<Entity<NewBranchDialogEntity>>,
    pub dialog_input_focus: Option<FocusHandle>,

    // Delete Worktree Dialog
    pub delete_worktree_dialog: DeleteWorktreeDialogUi,

    // Close Tab Dialog
    pub close_tab_dialog: CloseTabDialogUi,

    // Settings Modal
    pub show_settings: bool,
    pub settings_draft: Option<Config>,
    pub settings_secrets_draft: Option<Secrets>,
    pub settings_configuring_channel: Option<String>,
    pub settings_editing_agent: Option<usize>,
    pub settings_tab: String,
    pub settings_focus: Option<FocusHandle>,
    pub settings_focused_field: Option<String>,

    // Shared flag: when any modal is open, terminal output loop skips notifying
    pub modal_overlay_open: Arc<AtomicBool>,
}

impl DialogManager {
    pub fn new(modal_overlay_open: Arc<AtomicBool>) -> Self {
        Self {
            new_branch_dialog_model: None,
            new_branch_dialog_entity: None,
            dialog_input_focus: None,
            delete_worktree_dialog: DeleteWorktreeDialogUi::new(),
            close_tab_dialog: CloseTabDialogUi::new(),
            show_settings: false,
            settings_draft: None,
            settings_secrets_draft: None,
            settings_configuring_channel: None,
            settings_editing_agent: None,
            settings_tab: "channels".to_string(),
            settings_focus: None,
            settings_focused_field: None,
            modal_overlay_open,
        }
    }

    /// Returns true if any modal dialog is currently open.
    pub fn is_any_open(&self, cx: &App) -> bool {
        let new_branch_open = self.new_branch_dialog_model
            .as_ref()
            .map_or(false, |m| m.read(cx).is_open);
        self.show_settings || new_branch_open
            || self.delete_worktree_dialog.is_open()
            || self.close_tab_dialog.is_open()
    }

    /// Sync modal_overlay_open flag.
    pub fn sync_modal_flag(&self, cx: &App) {
        let new_branch_open = self.new_branch_dialog_model
            .as_ref()
            .map_or(false, |m| m.read(cx).is_open);
        let any_open = self.show_settings || new_branch_open;
        self.modal_overlay_open.store(any_open, Ordering::Relaxed);
    }

    pub fn is_settings_open(&self) -> bool {
        self.show_settings
    }

    pub fn is_new_branch_open(&self, cx: &App) -> bool {
        self.new_branch_dialog_model
            .as_ref()
            .map_or(false, |m| m.read(cx).is_open)
    }

    pub fn open_settings(&mut self, config: Config, secrets: Secrets, cx: &mut Context<Self>) {
        self.show_settings = true;
        self.settings_draft = Some(config);
        self.settings_secrets_draft = Some(secrets);
        self.modal_overlay_open.store(true, Ordering::Relaxed);
        cx.notify();
    }

    pub fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.show_settings = false;
        self.settings_draft = None;
        self.settings_secrets_draft = None;
        self.settings_configuring_channel = None;
        self.settings_editing_agent = None;
        self.settings_focused_field = None;
        self.modal_overlay_open.store(false, Ordering::Relaxed);
        cx.notify();
    }

    pub fn open_new_branch_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(ref model) = self.new_branch_dialog_model {
            let _ = cx.update_entity(model, |m, cx| {
                m.open();
                cx.notify();
            });
        }
        self.modal_overlay_open.store(true, Ordering::Relaxed);
        cx.notify();
    }

    pub fn close_new_branch_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(ref model) = self.new_branch_dialog_model {
            let _ = cx.update_entity(model, |m, cx| {
                m.close();
                cx.notify();
            });
        }
        self.modal_overlay_open.store(false, Ordering::Relaxed);
        cx.notify();
    }

    pub fn ensure_focus_handles(&mut self, cx: &mut Context<Self>) {
        if self.dialog_input_focus.is_none() {
            self.dialog_input_focus = Some(cx.focus_handle());
        }
    }

    // ========================================================================
    // Settings render methods (moved from AppRoot)
    // ========================================================================

    pub fn settings_channel_card_el<F>(
        name: &str,
        channel_key: &str,
        status: &str,
        enabled: bool,
        entity: Entity<Self>,
        on_toggle: F,
    ) -> impl IntoElement
    where
        F: Fn(&mut Config) + Send + 'static,
    {
        let name_owned = name.to_string();
        let status_owned = status.to_string();
        let name_ss = SharedString::from(name_owned.clone());
        let status_ss = SharedString::from(status_owned.clone());
        let entity_for_toggle = entity.clone();
        let entity_for_config = entity.clone();
        let toggle = div()
            .id(format!("settings-toggle-{}", name_owned))
            .w(px(40.))
            .h(px(22.))
            .rounded(px(11.))
            .flex()
            .items_center()
            .px(px(2.))
            .cursor_pointer()
            .bg(if enabled { rgb(0x0066cc) } else { rgb(0x4a4a4a) })
            .on_click(move |_event, _window, cx| {
                let _ = cx.update_entity(&entity_for_toggle, |this: &mut DialogManager, cx| {
                    if let Some(ref mut draft) = this.settings_draft {
                        on_toggle(draft);
                    }
                    cx.notify();
                });
            })
            .child(
                div()
                    .w(px(18.))
                    .h(px(18.))
                    .rounded(px(9.))
                    .bg(rgb(0xffffff))
                    .ml(if enabled { px(18.) } else { px(0.) })
            );
        let channel_key_owned = channel_key.to_string();
        let config_btn = div()
            .id(format!("settings-config-{}", name_owned))
            .px(px(12.))
            .py(px(6.))
            .rounded(px(4.))
            .bg(rgb(0x3d3d3d))
            .text_color(rgb(0xcccccc))
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(|style: StyleRefinement| style.bg(rgb(0x4d4d4d)))
            .on_click(move |_event, _window, cx| {
                let _ = cx.update_entity(&entity_for_config, |this: &mut DialogManager, cx| {
                    this.settings_configuring_channel = Some(channel_key_owned.clone());
                    cx.notify();
                });
            })
            .child("配置");
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .p(px(12.))
            .rounded(px(6.))
            .bg(rgb(0x1e1e1e))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0xffffff))
                            .child(name_ss)
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(0x888888))
                            .child(status_ss)
                    )
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .child(toggle)
                    .child(config_btn)
            )
    }

    pub fn render_settings_modal(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let dm_entity = cx.entity();
        let dm_entity_for_close = dm_entity.clone();
        let settings_focus = self.settings_focus.get_or_insert_with(|| cx.focus_handle()).clone();
        let active_tab = self.settings_tab.clone();

        // ── Tab bar ──
        let dm_entity_tab_channels = dm_entity.clone();
        let dm_entity_tab_agent = dm_entity.clone();
        let is_channels = active_tab == "channels";
        let tab_channels = div()
            .id("settings-tab-channels")
            .px(px(16.))
            .py(px(6.))
            .rounded_t(px(6.))
            .cursor_pointer()
            .text_size(px(13.))
            .font_weight(if is_channels { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
            .text_color(if is_channels { rgb(0xffffff) } else { rgb(0x999999) })
            .bg(if is_channels { rgb(0x3d3d3d) } else { rgb(0x2d2d2d) })
            .hover(|style: StyleRefinement| style.bg(rgb(0x454545)))
            .on_click(move |_event, _window, cx| {
                let _ = cx.update_entity(&dm_entity_tab_channels, |this: &mut DialogManager, cx| {
                    this.settings_tab = "channels".to_string();
                    cx.notify();
                });
            })
            .child("Channels");
        let tab_agent = div()
            .id("settings-tab-agent-detect")
            .px(px(16.))
            .py(px(6.))
            .rounded_t(px(6.))
            .cursor_pointer()
            .text_size(px(13.))
            .font_weight(if !is_channels { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
            .text_color(if !is_channels { rgb(0xffffff) } else { rgb(0x999999) })
            .bg(if !is_channels { rgb(0x3d3d3d) } else { rgb(0x2d2d2d) })
            .hover(|style: StyleRefinement| style.bg(rgb(0x454545)))
            .on_click(move |_event, _window, cx| {
                let _ = cx.update_entity(&dm_entity_tab_agent, |this: &mut DialogManager, cx| {
                    this.settings_tab = "agent_detect".to_string();
                    cx.notify();
                });
            })
            .child("Agent Detect");
        let tab_bar = div()
            .flex()
            .flex_row()
            .gap(px(2.))
            .pb(px(8.))
            .mb(px(4.))
            .border_b_1()
            .border_color(rgb(0x3d3d3d))
            .child(tab_channels)
            .child(tab_agent);

        // ── Tab body ──
        let tab_body = if is_channels {
            self.render_settings_channels_tab(cx)
        } else {
            self.render_settings_agent_detect_tab(cx)
        };

        // ── Layout ──
        let header_row = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(px(18.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff))
                    .child("Settings")
            )
            .child(
                div()
                    .id("settings-close-btn")
                    .px(px(12.))
                    .py(px(6.))
                    .rounded(px(4.))
                    .bg(rgb(0x3d3d3d))
                    .text_color(rgb(0xcccccc))
                    .text_size(px(14.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .hover(|style: StyleRefinement| style.bg(rgb(0x4d4d4d)))
                    .on_click(move |_event, _window, cx| {
                        let _ = cx.update_entity(&dm_entity_for_close, |this: &mut DialogManager, cx| {
                            this.show_settings = false;
                            this.settings_draft = None;
                            this.settings_secrets_draft = None;
                            this.settings_configuring_channel = None;
                            this.settings_editing_agent = None;
                            cx.notify();
                        });
                    })
                    .child("×")
            );

        let scrollable_body = div()
            .id("settings-content-scroll")
            .flex_grow()
            .overflow_y_scroll()
            .child(tab_body);

        let settings_content = div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .max_h(px(600.))
            .child(header_row)
            .child(tab_bar)
            .child(scrollable_body);
        let dm_entity_card = dm_entity.clone();
        let settings_card = div()
            .id("settings-dialog-card")
            .max_w(px(560.))
            .w_full()
            .flex()
            .flex_col()
            .gap(px(20.))
            .px(px(24.))
            .py(px(24.))
            .rounded(px(8.))
            .bg(rgb(0x2d2d2d))
            .shadow_lg()
            .on_mouse_down(gpui::MouseButton::Left, {
                move |_event, _window, cx| {
                    let _ = cx.update_entity(&dm_entity_card, |this: &mut DialogManager, cx| {
                        if this.settings_focused_field.is_some() {
                            this.settings_focused_field = None;
                            cx.notify();
                        }
                    });
                    cx.stop_propagation();
                }
            })
            .child(settings_content);
        let dm_entity_for_esc = dm_entity.clone();
        let dm_entity_for_esc2 = dm_entity.clone();
        div()
            .id("settings-modal-overlay")
            .absolute()
            .inset(px(0.))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000099u32))
            .cursor_pointer()
            .focusable()
            .track_focus(&settings_focus)
            .on_key_down(move |event: &KeyDownEvent, _window, cx| {
                if event.keystroke.key.as_str() == "escape" {
                    let _ = cx.update_entity(&dm_entity_for_esc, |this: &mut DialogManager, cx| {
                        if this.settings_focused_field.is_some() {
                            this.settings_focused_field = None;
                        } else {
                            this.show_settings = false;
                            this.settings_draft = None;
                            this.settings_secrets_draft = None;
                            this.settings_configuring_channel = None;
                            this.settings_editing_agent = None;
                            this.settings_focused_field = None;
                        }
                        cx.notify();
                    });
                } else {
                    let clipboard_text = if event.keystroke.modifiers.platform && event.keystroke.key.as_str() == "v" {
                        cx.read_from_clipboard().and_then(|c| {
                            let t = c.text().unwrap_or_default();
                            if t.is_empty() { None } else { Some(t) }
                        })
                    } else {
                        None
                    };
                    let is_select_all = event.keystroke.modifiers.platform && event.keystroke.key.as_str() == "a";
                    let _ = cx.update_entity(&dm_entity_for_esc, |this: &mut DialogManager, cx| {
                        if let Some(ref field_id) = this.settings_focused_field.clone() {
                            if event.keystroke.modifiers.platform && clipboard_text.is_none() && !is_select_all {
                                return;
                            }
                            let key = event.keystroke.key.as_str();
                            let draft = this.settings_draft.get_or_insert_with(|| Config::load().unwrap_or_default());
                            if field_id.starts_with("agent-name-") {
                                if let Ok(idx) = field_id.strip_prefix("agent-name-").unwrap().parse::<usize>() {
                                    if idx < draft.agent_detect.agents.len() {
                                        let name = &mut draft.agent_detect.agents[idx].name;
                                        if let Some(ref paste) = clipboard_text {
                                            let filtered: String = paste.chars().filter(|c| !c.is_control()).collect();
                                            name.push_str(&filtered);
                                        } else if is_select_all {
                                            // no-op
                                        } else {
                                            match key {
                                                "backspace" => { name.pop(); }
                                                "space" => { name.push(' '); }
                                                "tab" | "enter" => { this.settings_focused_field = None; }
                                                _ => {
                                                    let ch_text = event.keystroke.key_char.as_deref()
                                                        .or_else(|| {
                                                            let k = event.keystroke.key.as_str();
                                                            if k.chars().count() == 1 { Some(k) } else { None }
                                                        });
                                                    if let Some(ch) = ch_text {
                                                        let filtered: String = ch.chars()
                                                            .filter(|c| !c.is_control())
                                                            .collect();
                                                        name.push_str(&filtered);
                                                    }
                                                }
                                            }
                                        }
                                        cx.notify();
                                    }
                                }
                            } else if field_id.starts_with("rule-patterns-") {
                                let parts: Vec<&str> = field_id.strip_prefix("rule-patterns-").unwrap().split('-').collect();
                                if parts.len() == 2 {
                                    if let (Ok(ai), Ok(ri)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                                        if ai < draft.agent_detect.agents.len() && ri < draft.agent_detect.agents[ai].rules.len() {
                                            let patterns = &mut draft.agent_detect.agents[ai].rules[ri].patterns;
                                            let mut text = patterns.join(", ");
                                            if let Some(ref paste) = clipboard_text {
                                                let filtered: String = paste.chars().filter(|c| !c.is_control()).collect();
                                                text.push_str(&filtered);
                                            } else if is_select_all {
                                                // no-op
                                            } else {
                                                match key {
                                                    "backspace" => { text.pop(); }
                                                    "space" => { text.push(' '); }
                                                    "tab" | "enter" => { this.settings_focused_field = None; }
                                                    _ => {
                                                        let ch_text = event.keystroke.key_char.as_deref()
                                                            .or_else(|| {
                                                                let k = event.keystroke.key.as_str();
                                                                if k.chars().count() == 1 { Some(k) } else { None }
                                                            });
                                                        if let Some(ch) = ch_text {
                                                            let filtered: String = ch.chars()
                                                                .filter(|c| !c.is_control())
                                                                .collect();
                                                            text.push_str(&filtered);
                                                        }
                                                    }
                                                }
                                            }
                                            *patterns = text.split(',').map(|s| s.trim_start().to_string()).filter(|s| !s.is_empty()).collect();
                                            cx.notify();
                                        }
                                    }
                                }
                            } else if field_id.starts_with("agent-skip-patterns-") {
                                if let Ok(idx) = field_id.strip_prefix("agent-skip-patterns-").unwrap().parse::<usize>() {
                                    if idx < draft.agent_detect.agents.len() {
                                        let patterns = &mut draft.agent_detect.agents[idx].message_skip_patterns;
                                        let mut text = patterns.join(", ");
                                        if let Some(ref paste) = clipboard_text {
                                            let filtered: String = paste.chars().filter(|c| !c.is_control()).collect();
                                            text.push_str(&filtered);
                                        } else if is_select_all {
                                            // no-op
                                        } else {
                                            match key {
                                                "backspace" => { text.pop(); }
                                                "space" => { text.push(' '); }
                                                "tab" | "enter" => { this.settings_focused_field = None; }
                                                _ => {
                                                    let ch_text = event.keystroke.key_char.as_deref()
                                                        .or_else(|| {
                                                            let k = event.keystroke.key.as_str();
                                                            if k.chars().count() == 1 { Some(k) } else { None }
                                                        });
                                                    if let Some(ch) = ch_text {
                                                        let filtered: String = ch.chars()
                                                            .filter(|c| !c.is_control())
                                                            .collect();
                                                        text.push_str(&filtered);
                                                    }
                                                }
                                            }
                                        }
                                        *patterns = text.split(',').map(|s| s.trim_start().to_string()).filter(|s| !s.is_empty()).collect();
                                        cx.notify();
                                    }
                                }
                            }
                        }
                    });
                }
                cx.stop_propagation();
            })
            .on_scroll_wheel(|_event, _window, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(gpui::MouseButton::Left, move |_event, _window, cx| {
                let _ = cx.update_entity(&dm_entity_for_esc2, |this: &mut DialogManager, cx| {
                    this.show_settings = false;
                    this.settings_draft = None;
                    this.settings_secrets_draft = None;
                    this.settings_configuring_channel = None;
                    this.settings_editing_agent = None;
                    cx.notify();
                });
            })
            .child(settings_card)
    }

    fn render_settings_channels_tab(&mut self, cx: &mut Context<Self>) -> Div {
        let dm_entity = cx.entity();
        let config = self.settings_draft.clone().unwrap_or_else(|| Config::load().unwrap_or_default());
        let secrets = self.settings_secrets_draft.clone().unwrap_or_else(|| Secrets::load().unwrap_or_default());
        let discord_configured = config.remote_channels.discord.channel_id.as_ref().map_or(false, |s: &String| !s.is_empty())
            && secrets.remote_channels.discord.bot_token.as_ref().map_or(false, |s: &String| !s.is_empty());
        let kook_configured = config.remote_channels.kook.channel_id.as_ref().map_or(false, |s: &String| !s.is_empty())
            && secrets.remote_channels.kook.bot_token.as_ref().map_or(false, |s: &String| !s.is_empty());
        let feishu_configured = config.remote_channels.feishu.chat_id.as_ref().map_or(false, |s: &String| !s.is_empty())
            && secrets.remote_channels.feishu.app_id.as_ref().map_or(false, |s: &String| !s.is_empty())
            && secrets.remote_channels.feishu.app_secret.as_ref().map_or(false, |s: &String| !s.is_empty());
        let discord_enabled = config.remote_channels.discord.enabled;
        let kook_enabled = config.remote_channels.kook.enabled;
        let feishu_enabled = config.remote_channels.feishu.enabled;
        let dm_entity_discord = dm_entity.clone();
        let dm_entity_kook = dm_entity.clone();
        let dm_entity_feishu = dm_entity.clone();
        let discord_status = if discord_configured { "已配置" } else { "未配置" };
        let kook_status = if kook_configured { "已配置" } else { "未配置" };
        let feishu_status = if feishu_configured { "已配置" } else { "未配置" };
        let channel_cards = div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(Self::settings_channel_card_el(
                "Discord", "discord", discord_status, discord_enabled, dm_entity_discord,
                |draft| { draft.remote_channels.discord.enabled = !draft.remote_channels.discord.enabled; },
            ))
            .child(Self::settings_channel_card_el(
                "KOOK", "kook", kook_status, kook_enabled, dm_entity_kook,
                |draft| { draft.remote_channels.kook.enabled = !draft.remote_channels.kook.enabled; },
            ))
            .child(Self::settings_channel_card_el(
                "飞书", "feishu", feishu_status, feishu_enabled, dm_entity_feishu,
                |draft| { draft.remote_channels.feishu.enabled = !draft.remote_channels.feishu.enabled; },
            ));
        let config_guide = self.render_settings_config_guide(&dm_entity);
        let mut body = div().flex().flex_col().gap(px(16.)).child(channel_cards);
        if let Some(guide) = config_guide {
            body = body.child(guide);
        }
        body
    }

    fn render_settings_agent_detect_tab(&self, cx: &mut Context<Self>) -> Div {
        self.render_agent_detect_section(cx)
    }

    fn render_agent_detect_section(&self, cx: &mut Context<Self>) -> Div {
        let dm_entity = cx.entity();
        let config = self.settings_draft.clone().unwrap_or_else(|| Config::load().unwrap_or_default());
        let agents = config.agent_detect.agents.clone();
        let editing_idx = self.settings_editing_agent;

        let dm_entity_add = dm_entity.clone();
        let add_button = div()
            .id("agent-detect-add-btn")
            .px(px(10.))
            .py(px(4.))
            .rounded(px(4.))
            .bg(rgb(0x3d3d3d))
            .text_color(rgb(0xcccccc))
            .text_size(px(12.))
            .cursor_pointer()
            .hover(|style: StyleRefinement| style.bg(rgb(0x4d4d4d)))
            .on_click(move |_event, _window, cx| {
                let _ = cx.update_entity(&dm_entity_add, |this: &mut DialogManager, cx| {
                    let draft = this.settings_draft.get_or_insert_with(|| Config::load().unwrap_or_default());
                    draft.agent_detect.agents.insert(0, crate::config::AgentDef {
                        name: String::new(),
                        rules: vec![],
                        default_status: "Idle".to_string(),
                        message_skip_patterns: vec![],
                    });
                    if let Some(ref mut idx) = this.settings_editing_agent {
                        *idx += 1;
                    }
                    this.settings_editing_agent = Some(0);
                    cx.notify();
                });
            })
            .child("+ 添加");

        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xdddddd))
                    .child("Agent Detect"),
            )
            .child(add_button);

        let mut agent_cards = div().flex().flex_col().gap(px(8.));
        for (i, agent) in agents.iter().enumerate() {
            let is_editing = editing_idx == Some(i);
            if is_editing {
                agent_cards = agent_cards.child(self.render_agent_edit_card(i, agent, cx));
            } else {
                agent_cards = agent_cards.child(self.render_agent_summary_card(i, agent, cx));
            }
        }

        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(header)
            .child(agent_cards)
    }

    fn render_agent_summary_card(
        &self,
        index: usize,
        agent: &crate::config::AgentDef,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let dm_entity = cx.entity();
        let dm_entity_del = dm_entity.clone();
        let name = agent.name.clone();
        let default_status = agent.default_status.clone();

        let mut rules_els: Vec<Div> = Vec::new();
        for rule in &agent.rules {
            let patterns_str = rule.patterns.iter().map(|p| format!("\"{}\"", p)).collect::<Vec<_>>().join(", ");
            rules_els.push(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0x999999))
                    .child(format!("{}: {}", rule.status, patterns_str)),
            );
        }
        rules_els.push(
            div()
                .text_size(px(12.))
                .text_color(rgb(0x777777))
                .child(format!("默认: {}", default_status)),
        );

        let edit_btn = div()
            .id(SharedString::from(format!("agent-edit-{}", index)))
            .px(px(8.))
            .py(px(2.))
            .rounded(px(4.))
            .bg(rgb(0x3d3d3d))
            .text_color(rgb(0xcccccc))
            .text_size(px(11.))
            .cursor_pointer()
            .hover(|style: StyleRefinement| style.bg(rgb(0x4d4d4d)))
            .on_click(move |_event, _window, cx| {
                let _ = cx.update_entity(&dm_entity, |this: &mut DialogManager, cx| {
                    this.settings_editing_agent = Some(index);
                    cx.notify();
                });
            })
            .child("编辑");

        let del_btn = div()
            .id(SharedString::from(format!("agent-del-{}", index)))
            .px(px(8.))
            .py(px(2.))
            .rounded(px(4.))
            .bg(rgb(0x3d3d3d))
            .text_color(rgb(0xcc6666))
            .text_size(px(11.))
            .cursor_pointer()
            .hover(|style: StyleRefinement| style.bg(rgb(0x4d4d4d)))
            .on_click(move |_event, _window, cx| {
                let _ = cx.update_entity(&dm_entity_del, |this: &mut DialogManager, cx| {
                    let draft = this.settings_draft.get_or_insert_with(|| Config::load().unwrap_or_default());
                    if index < draft.agent_detect.agents.len() {
                        draft.agent_detect.agents.remove(index);
                    }
                    this.settings_editing_agent = None;
                    cx.notify();
                });
            })
            .child("删除");

        let top_row = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(0xffffff))
                    .child(if name.is_empty() { "(unnamed)".to_string() } else { name }),
            )
            .child(
                div().flex().flex_row().gap(px(6.)).child(edit_btn).child(del_btn),
            );

        let mut card = div()
            .p(px(12.))
            .rounded(px(6.))
            .bg(rgb(0x353535))
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(top_row);
        for el in rules_els {
            card = card.child(el);
        }
        card
    }

    fn render_agent_edit_card(
        &self,
        index: usize,
        agent: &crate::config::AgentDef,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let dm_entity = cx.entity();
        let agent_name = agent.name.clone();
        let agent_default = agent.default_status.clone();

        // Name input
        let name_field_id = format!("agent-name-{}", index);
        let name_is_focused = self.settings_focused_field.as_deref() == Some(&name_field_id);
        let dm_entity_name = dm_entity.clone();
        let name_field_id_for_click = name_field_id.clone();
        let settings_focus_for_name = self.settings_focus.clone();
        let name_display = if agent_name.is_empty() && !name_is_focused {
            div().text_color(rgb(0x666666)).text_size(px(13.)).child("点击输入名称")
        } else {
            let mut row = div().flex().flex_row().items_center();
            if !agent_name.is_empty() {
                row = row.child(div().text_size(px(13.)).text_color(rgb(0xeeeeee)).child(SharedString::from(agent_name.clone())));
            }
            if name_is_focused {
                row = row.child(div().w(px(1.5)).h(px(15.)).bg(rgb(0xffffff)).flex_shrink_0());
            }
            row
        };
        let name_input = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0x999999))
                    .w(px(60.))
                    .child("名称:"),
            )
            .child(
                div()
                    .id(SharedString::from(format!("agent-name-input-{}", index)))
                    .flex_1()
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(4.))
                    .bg(rgb(0x2a2a2a))
                    .when(name_is_focused, |el| el.border_1().border_color(rgb(0x0066cc)))
                    .cursor(gpui::CursorStyle::IBeam)
                    .on_click(move |_event, window, cx| {
                        let _ = cx.update_entity(&dm_entity_name, |this: &mut DialogManager, cx| {
                            this.settings_focused_field = Some(name_field_id_for_click.clone());
                            cx.notify();
                        });
                        if let Some(ref focus) = settings_focus_for_name {
                            window.focus(focus, cx);
                        }
                        cx.stop_propagation();
                    })
                    .child(name_display),
            );

        // Default status selector
        let dm_entity_default = dm_entity.clone();
        let default_selector = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0x999999))
                    .w(px(60.))
                    .child("默认:"),
            )
            .child(
                div()
                    .id(SharedString::from(format!("agent-default-{}", index)))
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(4.))
                    .bg(rgb(0x2a2a2a))
                    .text_size(px(13.))
                    .text_color(rgb(0xeeeeee))
                    .cursor_pointer()
                    .child(agent_default.clone())
                    .on_click(move |_event, _window, cx| {
                        let _ = cx.update_entity(&dm_entity_default, |this: &mut DialogManager, cx| {
                            let draft = this.settings_draft.get_or_insert_with(|| Config::load().unwrap_or_default());
                            if index < draft.agent_detect.agents.len() {
                                let current = &draft.agent_detect.agents[index].default_status;
                                let options = ["Idle", "Running", "Waiting", "Error"];
                                let next = options.iter()
                                    .position(|&o| o == current.as_str())
                                    .map(|i| (i + 1) % options.len())
                                    .unwrap_or(0);
                                draft.agent_detect.agents[index].default_status = options[next].to_string();
                            }
                            cx.notify();
                        });
                    }),
            );

        // Rules list
        let mut rules_container = div().flex().flex_col().gap(px(4.));
        for (ri, rule) in agent.rules.iter().enumerate() {
            let rule_status = rule.status.clone();
            let patterns_str = rule.patterns.join(", ");
            let dm_entity_status = dm_entity.clone();
            let dm_entity_del_rule = dm_entity.clone();

            let status_btn = div()
                .id(SharedString::from(format!("rule-status-{}-{}", index, ri)))
                .px(px(6.))
                .py(px(2.))
                .rounded(px(3.))
                .bg(rgb(0x2a2a2a))
                .text_size(px(12.))
                .text_color(rgb(0xeeeeee))
                .w(px(70.))
                .cursor_pointer()
                .child(rule_status.clone())
                .on_click(move |_event, _window, cx| {
                    let _ = cx.update_entity(&dm_entity_status, |this: &mut DialogManager, cx| {
                        let draft = this.settings_draft.get_or_insert_with(|| Config::load().unwrap_or_default());
                        if index < draft.agent_detect.agents.len() && ri < draft.agent_detect.agents[index].rules.len() {
                            let current = &draft.agent_detect.agents[index].rules[ri].status;
                            let options = ["Running", "Waiting", "Error", "Idle"];
                            let next = options.iter()
                                .position(|&o| o == current.as_str())
                                .map(|i| (i + 1) % options.len())
                                .unwrap_or(0);
                            draft.agent_detect.agents[index].rules[ri].status = options[next].to_string();
                        }
                        cx.notify();
                    });
                });

            let pat_field_id = format!("rule-patterns-{}-{}", index, ri);
            let pat_is_focused = self.settings_focused_field.as_deref() == Some(&pat_field_id);
            let dm_entity_pat = dm_entity.clone();
            let pat_field_id_for_click = pat_field_id.clone();
            let settings_focus_for_pat = self.settings_focus.clone();
            let pat_inner = if patterns_str.is_empty() && !pat_is_focused {
                div().text_color(rgb(0x666666)).text_size(px(12.)).child("(no patterns)")
            } else {
                let mut row = div().flex().flex_row().items_center();
                if !patterns_str.is_empty() {
                    row = row.child(div().text_size(px(12.)).text_color(rgb(0xbbbbbb)).child(SharedString::from(patterns_str)));
                }
                if pat_is_focused {
                    row = row.child(div().w(px(1.5)).h(px(13.)).bg(rgb(0xffffff)).flex_shrink_0());
                }
                row
            };
            let patterns_display = div()
                .id(SharedString::from(format!("rule-pat-input-{}-{}", index, ri)))
                .flex_1()
                .px(px(6.))
                .py(px(2.))
                .rounded(px(3.))
                .bg(rgb(0x2a2a2a))
                .when(pat_is_focused, |el| el.border_1().border_color(rgb(0x0066cc)))
                .cursor(gpui::CursorStyle::IBeam)
                .on_click(move |_event, window, cx| {
                    let _ = cx.update_entity(&dm_entity_pat, |this: &mut DialogManager, cx| {
                        this.settings_focused_field = Some(pat_field_id_for_click.clone());
                        cx.notify();
                    });
                    if let Some(ref focus) = settings_focus_for_pat {
                        window.focus(focus, cx);
                    }
                    cx.stop_propagation();
                })
                .child(pat_inner);

            let del_rule_btn = div()
                .id(SharedString::from(format!("rule-del-{}-{}", index, ri)))
                .px(px(6.))
                .py(px(2.))
                .rounded(px(3.))
                .text_size(px(12.))
                .text_color(rgb(0xcc6666))
                .cursor_pointer()
                .hover(|style: StyleRefinement| style.bg(rgb(0x4a3333)))
                .child("×")
                .on_click(move |_event, _window, cx| {
                    let _ = cx.update_entity(&dm_entity_del_rule, |this: &mut DialogManager, cx| {
                        let draft = this.settings_draft.get_or_insert_with(|| Config::load().unwrap_or_default());
                        if index < draft.agent_detect.agents.len() && ri < draft.agent_detect.agents[index].rules.len() {
                            draft.agent_detect.agents[index].rules.remove(ri);
                        }
                        cx.notify();
                    });
                });

            let rule_row = div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(0x666666))
                        .w(px(16.))
                        .child(format!("{}.", ri + 1)),
                )
                .child(status_btn)
                .child(patterns_display)
                .child(del_rule_btn);

            rules_container = rules_container.child(rule_row);
        }

        // Add rule button
        let dm_entity_add_rule = dm_entity.clone();
        let add_rule_btn = div()
            .id(SharedString::from(format!("agent-add-rule-{}", index)))
            .px(px(8.))
            .py(px(4.))
            .rounded(px(4.))
            .bg(rgb(0x2a2a2a))
            .text_color(rgb(0xcccccc))
            .text_size(px(11.))
            .cursor_pointer()
            .hover(|style: StyleRefinement| style.bg(rgb(0x3a3a3a)))
            .on_click(move |_event, _window, cx| {
                let _ = cx.update_entity(&dm_entity_add_rule, |this: &mut DialogManager, cx| {
                    let draft = this.settings_draft.get_or_insert_with(|| Config::load().unwrap_or_default());
                    if index < draft.agent_detect.agents.len() {
                        draft.agent_detect.agents[index].rules.push(crate::config::AgentRule {
                            status: "Running".to_string(),
                            patterns: vec!["pattern".to_string()],
                        });
                    }
                    cx.notify();
                });
            })
            .child("+ 添加规则");

        // Message skip patterns input
        let skip_patterns_str = agent.message_skip_patterns.join(", ");
        let skip_field_id = format!("agent-skip-patterns-{}", index);
        let skip_is_focused = self.settings_focused_field.as_deref() == Some(&skip_field_id);
        let dm_entity_skip = dm_entity.clone();
        let skip_field_id_for_click = skip_field_id.clone();
        let settings_focus_for_skip = self.settings_focus.clone();
        let skip_inner = if skip_patterns_str.is_empty() && !skip_is_focused {
            div().text_color(rgb(0x666666)).text_size(px(12.)).child("(无，逗号分隔)")
        } else {
            let mut row = div().flex().flex_row().items_center();
            if !skip_patterns_str.is_empty() {
                row = row.child(div().text_size(px(12.)).text_color(rgb(0xbbbbbb)).child(SharedString::from(skip_patterns_str)));
            }
            if skip_is_focused {
                row = row.child(div().w(px(1.5)).h(px(13.)).bg(rgb(0xffffff)).flex_shrink_0());
            }
            row
        };
        let skip_patterns_input = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0x999999))
                    .w(px(60.))
                    .child("跳过:"),
            )
            .child(
                div()
                    .id(SharedString::from(format!("agent-skip-input-{}", index)))
                    .flex_1()
                    .px(px(8.))
                    .py(px(4.))
                    .rounded(px(4.))
                    .bg(rgb(0x2a2a2a))
                    .when(skip_is_focused, |el| el.border_1().border_color(rgb(0x0066cc)))
                    .cursor(gpui::CursorStyle::IBeam)
                    .on_click(move |_event, window, cx| {
                        let _ = cx.update_entity(&dm_entity_skip, |this: &mut DialogManager, cx| {
                            this.settings_focused_field = Some(skip_field_id_for_click.clone());
                            cx.notify();
                        });
                        if let Some(ref focus) = settings_focus_for_skip {
                            window.focus(focus, cx);
                        }
                        cx.stop_propagation();
                    })
                    .child(skip_inner),
            );

        // Save button
        let dm_entity_done = dm_entity.clone();
        let save_btn = div()
            .id(SharedString::from(format!("agent-save-{}", index)))
            .py(px(6.))
            .rounded(px(4.))
            .bg(rgb(0x0066cc))
            .text_color(rgb(0xffffff))
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(|style: StyleRefinement| style.bg(rgb(0x0077dd)))
            .flex()
            .items_center()
            .justify_center()
            .on_click(move |_event, _window, cx| {
                let _ = cx.update_entity(&dm_entity_done, |this: &mut DialogManager, cx| {
                    if let Some(ref mut draft) = this.settings_draft {
                        for agent in &mut draft.agent_detect.agents {
                            for rule in &mut agent.rules {
                                for p in &mut rule.patterns {
                                    *p = p.trim().to_string();
                                }
                                rule.patterns.retain(|p| !p.is_empty());
                            }
                            for p in &mut agent.message_skip_patterns {
                                *p = p.trim().to_string();
                            }
                            agent.message_skip_patterns.retain(|p| !p.is_empty());
                        }
                    }
                    if let Some(ref draft) = this.settings_draft {
                        let mut current = Config::load().unwrap_or_default();
                        current.agent_detect = draft.agent_detect.clone();
                        current.remote_channels = draft.remote_channels.clone();
                        match current.save() {
                            Ok(()) => eprintln!("[pmux] Agent config saved ({} agents)", current.agent_detect.agents.len()),
                            Err(e) => eprintln!("[pmux] Agent config save FAILED: {}", e),
                        }
                    }
                    this.settings_editing_agent = None;
                    cx.notify();
                });
            })
            .child("Save");

        let rules_header = div()
            .text_size(px(11.))
            .text_color(rgb(0x888888))
            .child("检测规则（按顺序匹配，第一个命中的生效）：");

        let skip_header = div()
            .text_size(px(11.))
            .text_color(rgb(0x888888))
            .child("消息跳过模式（提取最后一条消息时跳过包含这些文本的行）：");

        div()
            .p(px(12.))
            .rounded(px(6.))
            .bg(rgb(0x353535))
            .border_1()
            .border_color(rgb(0x0066cc))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(name_input)
            .child(default_selector)
            .child(rules_header)
            .child(rules_container)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.))
                    .child(add_rule_btn),
            )
            .child(skip_header)
            .child(skip_patterns_input)
            .child(save_btn)
    }

    fn render_settings_config_guide(&self, dm_entity: &Entity<DialogManager>) -> Option<impl IntoElement> {
        let channel = self.settings_configuring_channel.as_ref()?.clone();
        let (title, steps, url) = match channel.as_str() {
            "discord" => (
                "Discord 配置指南",
                "1. 创建应用并添加 Bot\n2. 复制 Bot Token 到 secrets.json 的 discord.bot_token\n3. 邀请 Bot 到服务器\n4. 开启开发者模式，右键频道复制 Channel ID 到 config.json",
                "https://discord.com/developers/applications",
            ),
            "kook" => (
                "KOOK 配置指南",
                "1. 创建应用并添加机器人\n2. 复制 Token 到 secrets.json 的 kook.bot_token\n3. 邀请机器人到服务器\n4. 获取频道 ID 填入 config.json 的 kook.channel_id",
                "https://developer.kookapp.cn/",
            ),
            "feishu" => (
                "飞书配置指南",
                "1. 创建企业自建应用\n2. 记录 App ID、App Secret 填入 secrets.json\n3. 开通「获取与发送群消息」权限\n4. 将 chat_id 填入 config.json 的 feishu.chat_id",
                "https://open.feishu.cn/",
            ),
            _ => ("配置", "", ""),
        };
        let dm_entity_config = dm_entity.clone();
        let url_owned = url.to_string();
        let open_btn = div()
            .px(px(12.))
            .py(px(8.))
            .rounded(px(6.))
            .bg(rgb(0x0066cc))
            .text_color(rgb(0xffffff))
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(|s: StyleRefinement| s.bg(rgb(0x0077dd)))
            .on_mouse_down(gpui::MouseButton::Left, move |_event, _window, _cx| {
                let _ = open::that(&url_owned);
            })
            .child("在浏览器中打开");
        let done_btn = div()
            .px(px(12.))
            .py(px(8.))
            .rounded(px(6.))
            .bg(rgb(0x3d3d3d))
            .text_color(rgb(0xcccccc))
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(|s: StyleRefinement| s.bg(rgb(0x4d4d4d)))
            .on_mouse_down(gpui::MouseButton::Left, move |_event, _window, cx| {
                let _ = cx.update_entity(&dm_entity_config, |this: &mut DialogManager, cx| {
                    this.settings_configuring_channel = None;
                    cx.notify();
                });
            })
            .child("完成");
        Some(div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(16.))
            .rounded(px(6.))
            .bg(rgb(0x1e1e1e))
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(0xffffff))
                    .child(title)
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0xaaaaaa))
                    .whitespace_normal()
                    .child(steps)
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.))
                    .child(open_btn)
                    .child(done_btn)
            ))
    }
}
