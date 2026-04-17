//! Confirmation dialog for installing the tpm-workflow Claude Code plugin.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use super::DialogResult;
use crate::tui::styles::Theme;

pub struct TpmPluginInstallDialog {
    selected: bool, // true = Accept, false = Cancel
    scroll_offset: u16,
}

impl Default for TpmPluginInstallDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl TpmPluginInstallDialog {
    pub fn new() -> Self {
        Self {
            selected: true,
            scroll_offset: 0,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> DialogResult<bool> {
        match key.code {
            KeyCode::Esc => DialogResult::Cancel,
            KeyCode::Char('y') | KeyCode::Char('Y') => DialogResult::Submit(true),
            KeyCode::Char('n') | KeyCode::Char('N') => DialogResult::Cancel,
            KeyCode::Enter => {
                if self.selected {
                    DialogResult::Submit(true)
                } else {
                    DialogResult::Cancel
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.selected = true;
                DialogResult::Continue
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.selected = false;
                DialogResult::Continue
            }
            KeyCode::Tab => {
                self.selected = !self.selected;
                DialogResult::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                DialogResult::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let total_lines = self.build_content_lines().len() as u16;
                if self.scroll_offset + 1 < total_lines {
                    self.scroll_offset += 1;
                }
                DialogResult::Continue
            }
            _ => DialogResult::Continue,
        }
    }

    fn build_content_lines(&self) -> Vec<Line<'_>> {
        vec![
            Line::from(Span::styled(
                "Files written or modified:",
                Style::default().bold(),
            )),
            Line::from("  ~/.claude/plugins/marketplaces/tpm-workflow/"),
            Line::from("  ~/.claude/plugins/cache/tpm-workflow/tpm-workflow/<version>/"),
            Line::from("  ~/.claude/plugins/known_marketplaces.json"),
            Line::from("  ~/.claude/plugins/installed_plugins.json"),
            Line::from(""),
            Line::from(Span::styled("Source:", Style::default().bold())),
            Line::from("  github.com/Loulen/tpm-workflow (cloned via git)"),
            Line::from(""),
            Line::from(Span::styled(
                "This does NOT run any code from the plugin.",
                Style::default().bold(),
            )),
            Line::from("AoE only places files; Claude Code reads them next session."),
        ]
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let content_lines = self.build_content_lines();
        let content_height = content_lines.len() as u16 + 6; // header + spacing + buttons

        let dialog_width = 70.min(area.width.saturating_sub(4));
        let dialog_height = (content_height + 6).min(area.height.saturating_sub(4));
        let dialog_area = super::centered_rect(area, dialog_width, dialog_height);

        frame.render_widget(Clear, dialog_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.accent))
            .title(" TPM Workflow Plugin ")
            .title_style(Style::default().fg(theme.accent).bold());

        let inner = block.inner(dialog_area);
        frame.render_widget(block, dialog_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // header
                Constraint::Min(1),    // content
                Constraint::Length(2), // buttons
            ])
            .split(inner);

        let header = Paragraph::new(
            "AoE wants to install the tpm-workflow plugin into your ~/.claude\ndirectory. Accept to install; cancel to leave TPM mode off.",
        )
        .style(Style::default().fg(theme.text))
        .wrap(Wrap { trim: true });
        frame.render_widget(header, chunks[0]);

        let visible_lines: Vec<Line> = content_lines
            .into_iter()
            .skip(self.scroll_offset as usize)
            .collect();
        let content_paragraph = Paragraph::new(visible_lines)
            .style(Style::default().fg(theme.dimmed))
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(theme.border)),
            );
        frame.render_widget(content_paragraph, chunks[1]);

        let accept_style = if self.selected {
            Style::default().fg(theme.running).bold()
        } else {
            Style::default().fg(theme.dimmed)
        };
        let cancel_style = if !self.selected {
            Style::default().fg(theme.accent).bold()
        } else {
            Style::default().fg(theme.dimmed)
        };

        let buttons = Line::from(vec![
            Span::raw("  "),
            Span::styled("[Accept (y)]", accept_style),
            Span::raw("    "),
            Span::styled("[Cancel (Esc)]", cancel_style),
        ]);

        frame.render_widget(
            Paragraph::new(buttons).alignment(Alignment::Center),
            chunks[2],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn default_selection_is_accept() {
        let dialog = TpmPluginInstallDialog::new();
        assert!(dialog.selected);
    }

    #[test]
    fn y_submits_true() {
        let mut dialog = TpmPluginInstallDialog::new();
        let result = dialog.handle_key(key(KeyCode::Char('y')));
        assert!(matches!(result, DialogResult::Submit(true)));
    }

    #[test]
    fn uppercase_y_submits_true() {
        let mut dialog = TpmPluginInstallDialog::new();
        let result = dialog.handle_key(key(KeyCode::Char('Y')));
        assert!(matches!(result, DialogResult::Submit(true)));
    }

    #[test]
    fn n_cancels() {
        let mut dialog = TpmPluginInstallDialog::new();
        let result = dialog.handle_key(key(KeyCode::Char('n')));
        assert!(matches!(result, DialogResult::Cancel));
    }

    #[test]
    fn esc_cancels() {
        let mut dialog = TpmPluginInstallDialog::new();
        let result = dialog.handle_key(key(KeyCode::Esc));
        assert!(matches!(result, DialogResult::Cancel));
    }

    #[test]
    fn enter_with_accept_submits() {
        let mut dialog = TpmPluginInstallDialog::new();
        dialog.selected = true;
        let result = dialog.handle_key(key(KeyCode::Enter));
        assert!(matches!(result, DialogResult::Submit(true)));
    }

    #[test]
    fn enter_with_cancel_cancels() {
        let mut dialog = TpmPluginInstallDialog::new();
        dialog.selected = false;
        let result = dialog.handle_key(key(KeyCode::Enter));
        assert!(matches!(result, DialogResult::Cancel));
    }

    #[test]
    fn tab_toggles_selection() {
        let mut dialog = TpmPluginInstallDialog::new();
        assert!(dialog.selected);
        dialog.handle_key(key(KeyCode::Tab));
        assert!(!dialog.selected);
        dialog.handle_key(key(KeyCode::Tab));
        assert!(dialog.selected);
    }

    #[test]
    fn left_and_right_set_selection() {
        let mut dialog = TpmPluginInstallDialog::new();
        dialog.handle_key(key(KeyCode::Right));
        assert!(!dialog.selected);
        dialog.handle_key(key(KeyCode::Left));
        assert!(dialog.selected);
    }

    #[test]
    fn content_lines_mention_key_files() {
        let dialog = TpmPluginInstallDialog::new();
        let lines = dialog.build_content_lines();
        let text: String = lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("known_marketplaces.json"));
        assert!(text.contains("cache/tpm-workflow"));
        assert!(text.contains("installed_plugins.json"));
        assert!(text.contains("github.com/Loulen/tpm-workflow"));
    }
}
