//! Keyboard shortcuts: each frame's key presses become `Action`s.

use super::actions::Action;
use super::pickers::Picker;
use super::{App, GroomMode, Screen};
use egui::{Key, KeyboardShortcut, Modifiers};

impl App {
    pub(super) fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        let screens = [(Key::Num1, Screen::Issues), (Key::Num2, Screen::Grooming), (Key::Num3, Screen::Team), (Key::Num4, Screen::Roadmap)];
        for (k, s) in screens {
            if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, k))) {
                self.actions.push(Action::SetScreen(s));
            }
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::R))) {
            self.actions.push(Action::Refresh);
        }
        let shortcut = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
        // Help owns the keyboard while open, ahead of pickers and the Issues keys.
        if self.help_open {
            if shortcut(Modifiers::NONE, Key::Escape) || shortcut(Modifiers::NONE, Key::Questionmark) {
                self.actions.push(Action::ToggleHelp);
            }
            return;
        }
        // `?` is Shift+/; its logical key is Questionmark, so it never reads as `/`.
        if !ctx.text_edit_focused() && shortcut(Modifiers::NONE, Key::Questionmark) {
            self.actions.push(Action::ToggleHelp);
            return;
        }
        // An open picker owns the keyboard. Assign's filter box has focus, so
        // j/k type there; Edit has text fields, so only Esc and Enter apply.
        if let Some((edit, assign)) = self.picker.as_ref().map(|p| (matches!(p, Picker::Edit { .. }), matches!(p, Picker::Assign { .. }))) {
            if shortcut(Modifiers::NONE, Key::Escape) {
                self.actions.push(Action::PickerCancel);
                return;
            }
            // Edit's fields are single-line, so Enter confirms from any of them (Tab moves between them).
            if edit {
                if shortcut(Modifiers::NONE, Key::Enter) {
                    self.actions.push(Action::PickerConfirm);
                }
                return;
            }
            let letters = !assign;
            if shortcut(Modifiers::NONE, Key::ArrowDown) || (letters && shortcut(Modifiers::NONE, Key::J)) {
                self.actions.push(Action::PickerMove(1));
            }
            if shortcut(Modifiers::NONE, Key::ArrowUp) || (letters && shortcut(Modifiers::NONE, Key::K)) {
                self.actions.push(Action::PickerMove(-1));
            }
            if shortcut(Modifiers::NONE, Key::Enter) {
                self.actions.push(Action::PickerConfirm);
            }
            return;
        }
        if self.screen == Screen::Grooming {
            self.grooming_shortcuts(ctx);
            return;
        }
        if self.screen != Screen::Issues {
            return;
        }
        if shortcut(cmd, Key::K) {
            self.actions.push(Action::ToggleGoTo);
        }
        if self.goto.is_some() {
            if shortcut(Modifiers::NONE, Key::Escape) {
                self.goto = None;
            }
            return;
        }
        if shortcut(cmd, Key::F) {
            self.focus_search = true;
        }
        let browse = self.selected.as_ref().map(|k| self.cfg.browse_url(k));
        if shortcut(cmd, Key::O)
            && let Some(u) = &browse
        {
            self.actions.push(Action::OpenUrl(u.clone()));
        }
        // Plain keys belong to a focused text field.
        if ctx.text_edit_focused() {
            return;
        }
        // A search the box ran stays clearable with the box unfocused (the hint bar keeps "Esc clear").
        if crate::views::issues::esc_clears_search(self.search_active, self.typed_search, self.text_search.is_some()) && shortcut(Modifiers::NONE, Key::Escape)
        {
            self.filter.text.clear();
            self.actions.push(Action::RunSearch(String::new()));
        }
        if shortcut(Modifiers::NONE, Key::Slash) {
            self.focus_search = true;
        }
        // Before plain j/k: consume_key(NONE, J) also matches Shift+J.
        if shortcut(Modifiers::SHIFT, Key::J) {
            self.actions.push(Action::Rank(1));
        }
        if shortcut(Modifiers::SHIFT, Key::K) {
            self.actions.push(Action::Rank(-1));
        }
        for (k, a) in [
            (Key::T, Action::OpenTransition),
            (Key::A, Action::OpenAssign),
            (Key::S, Action::OpenSprint),
            (Key::E, Action::OpenEdit),
            (Key::L, Action::ApplyTeamLabel),
            (Key::M, Action::ToggleRoadmap),
        ] {
            if shortcut(Modifiers::NONE, k) {
                self.actions.push(a);
            }
        }
        if shortcut(Modifiers::NONE, Key::J) | shortcut(Modifiers::NONE, Key::ArrowDown) {
            self.actions.push(Action::Move(1));
        }
        if shortcut(Modifiers::NONE, Key::K) | shortcut(Modifiers::NONE, Key::ArrowUp) {
            self.actions.push(Action::Move(-1));
        }
        if shortcut(Modifiers::NONE, Key::C)
            && let Some(u) = browse
        {
            self.actions.push(Action::Copy(u));
        }
    }
}

impl App {
    /// The Grooming screen's keys (`keys::Group::Grooming`).
    fn grooming_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.text_edit_focused() {
            return;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::NONE, Key::Tab))) {
            self.actions.push(Action::GroomMode(other_mode(self.grooming.mode)));
        }
    }
}

/// The mode Tab switches to.
fn other_mode(m: GroomMode) -> GroomMode {
    match m {
        GroomMode::PreCheck => GroomMode::Session,
        GroomMode::Session => GroomMode::PreCheck,
    }
}

/// ⌘W closes the window on every screen, signed in or not.
pub(super) fn close_shortcut(ctx: &egui::Context) {
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::W))) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
