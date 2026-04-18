//! STATE.md side panel for TPM sessions
//!
//! Renders the contents of `.tpm/STATE.md` (relative to the session's project
//! or worktree main-repo path) as styled text in a right-side TUI panel.
//! The panel is toggled with `S` and auto-refreshes by polling the file every
//! 2-3 seconds.

use std::path::{Path, PathBuf};
use std::time::Instant;

use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::session::Instance;
use crate::tui::styles::Theme;

/// Interval between STATE.md re-reads (seconds).
const POLL_INTERVAL_SECS: u64 = 2;

/// Cached STATE.md content and metadata.
pub(in crate::tui) struct StatePanelCache {
    /// Session ID this cache belongs to (invalidated on session switch).
    session_id: Option<String>,
    /// Raw file content (empty if file missing).
    pub(super) content: String,
    /// When we last read the file from disk.
    last_poll: Instant,
    /// Resolved path to the STATE.md file (None = not yet resolved).
    resolved_path: Option<PathBuf>,
    /// Scroll offset for the TUI panel.
    pub(super) scroll_offset: usize,
}

impl Default for StatePanelCache {
    fn default() -> Self {
        Self {
            session_id: None,
            content: String::new(),
            last_poll: Instant::now(),
            resolved_path: None,
            scroll_offset: 0,
        }
    }
}

impl StatePanelCache {
    /// Resolve the STATE.md path for an instance. Checks the worktree
    /// main_repo_path first (so all branches of the same repo share one
    /// STATE.md), then falls back to project_path.
    fn resolve_path(inst: &Instance) -> Option<PathBuf> {
        // Try worktree main repo path first
        if let Some(wt) = &inst.worktree_info {
            let p = Path::new(&wt.main_repo_path).join(".tpm/STATE.md");
            if p.exists() {
                return Some(p);
            }
        }

        // Try project_path
        let p = Path::new(&inst.project_path).join(".tpm/STATE.md");
        if p.exists() {
            return Some(p);
        }

        None
    }

    /// Refresh the cache if the session changed or the poll interval elapsed.
    /// Returns true if the content actually changed.
    pub(super) fn refresh_if_needed(&mut self, inst: &Instance) -> bool {
        let session_changed = self.session_id.as_deref() != Some(&inst.id);

        if session_changed {
            self.session_id = Some(inst.id.clone());
            self.resolved_path = Self::resolve_path(inst);
            self.content.clear();
            self.last_poll = Instant::now()
                .checked_sub(std::time::Duration::from_secs(POLL_INTERVAL_SECS + 1))
                .unwrap_or_else(Instant::now);
        }

        let elapsed = self.last_poll.elapsed().as_secs();
        if elapsed < POLL_INTERVAL_SECS && !session_changed {
            return false;
        }

        self.last_poll = Instant::now();

        // Re-resolve path periodically (STATE.md may appear/disappear)
        if elapsed >= POLL_INTERVAL_SECS * 3 || session_changed {
            self.resolved_path = Self::resolve_path(inst);
        }

        let new_content = self
            .resolved_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();

        if new_content != self.content {
            self.content = new_content;
            // Clamp scroll offset to new content length
            let line_count = self.content.lines().count();
            if self.scroll_offset > line_count {
                self.scroll_offset = line_count.saturating_sub(1);
            }
            true
        } else {
            false
        }
    }

    /// Whether there is STATE.md content to show.
    pub(super) fn has_content(&self) -> bool {
        !self.content.is_empty()
    }

    /// Whether a STATE.md path has been resolved (cached, no filesystem stat).
    pub(super) fn has_state_file(&self) -> bool {
        self.resolved_path.is_some()
    }

    /// Check if a STATE.md exists for the given instance (does filesystem stat).
    /// Use sparingly; prefer `has_state_file()` for hot paths like rendering.
    pub(super) fn exists_for(inst: &Instance) -> bool {
        Self::resolve_path(inst).is_some()
    }

    /// Reset scroll offset to zero.
    pub(super) fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
    }
}

/// Render the STATE.md panel into the given area.
pub(in crate::tui) fn render_state_panel(
    frame: &mut Frame,
    area: Rect,
    content: &str,
    theme: &Theme,
    scroll_offset: usize,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .title(" TPM State ")
        .title_style(Style::default().fg(theme.accent).bold())
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = parse_state_md(content, theme, inner.width as usize);
    let total = lines.len();
    let visible = inner.height as usize;

    let clamped_offset = scroll_offset.min(total.saturating_sub(visible));
    let display_lines: Vec<Line> = lines
        .into_iter()
        .skip(clamped_offset)
        .take(visible)
        .collect();

    frame.render_widget(Paragraph::new(display_lines), inner);

    // Truncation indicator when content overflows
    if total > visible && clamped_offset + visible < total {
        let indicator = Span::styled(
            format!(" ▼ {}/{} ", clamped_offset + visible, total),
            Style::default().fg(theme.dimmed),
        );
        let indicator_area = Rect {
            x: area.x + 2,
            y: area.y + area.height.saturating_sub(1),
            width: area.width.saturating_sub(4).min(20),
            height: 1,
        };
        frame.render_widget(Paragraph::new(Line::from(indicator)), indicator_area);
    }
}

/// Parse STATE.md markdown into styled ratatui Lines.
fn parse_state_md(content: &str, theme: &Theme, width: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    // Detect if we're inside a markdown table
    let mut in_table = false;
    let mut col_widths: Vec<usize> = Vec::new();

    let raw_lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < raw_lines.len() {
        let line = raw_lines[i];
        let trimmed = line.trim();

        // Headers (check longest prefix first)
        if let Some(rest) = trimmed.strip_prefix("### ") {
            let style = Style::default().fg(theme.text).bold().italic();
            lines.extend(wrap_line(rest, style, width, "  "));
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            let style = Style::default().fg(theme.title).bold();
            lines.extend(wrap_line(rest, style, width, "  "));
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            let style = Style::default().fg(theme.title).bold();
            lines.extend(wrap_line(rest, style, width, "  "));
            i += 1;
            continue;
        }

        // Table detection: line starts with | and contains at least one more |
        if trimmed.starts_with('|') && trimmed.matches('|').count() >= 2 {
            if !in_table {
                in_table = true;
                col_widths.clear();
                // Scan ahead to compute column widths for alignment
                col_widths = compute_table_col_widths(&raw_lines[i..], width);
            }

            // Skip separator rows (|---|---|)
            if is_table_separator(trimmed) {
                i += 1;
                continue;
            }

            let cells = parse_table_row(trimmed);
            // A row is a header only if the next line is a separator
            let is_header = i + 1 < raw_lines.len() && is_table_separator(raw_lines[i + 1].trim());

            let styled_cells = if is_header {
                render_table_header(&cells, &col_widths, theme)
            } else {
                render_table_row(&cells, &col_widths, theme)
            };

            lines.push(styled_cells);
            i += 1;
            continue;
        }

        // End of table
        if in_table {
            in_table = false;
            col_widths.clear();
        }

        // Empty line
        if trimmed.is_empty() {
            lines.push(Line::from(""));
            i += 1;
            continue;
        }

        // Bullet points
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let bullet_content = &trimmed[2..];
            let text = format!("  \u{2022} {}", bullet_content);
            let style = Style::default().fg(theme.text);
            lines.extend(wrap_line(&text, style, width, "    "));
            i += 1;
            continue;
        }

        // Regular text
        let style = Style::default().fg(theme.text);
        lines.extend(wrap_line(trimmed, style, width, ""));
        i += 1;
    }

    lines
}

/// Check if a table line is a separator (e.g., |---|---|).
/// Each cell segment must contain at least 3 dashes.
fn is_table_separator(line: &str) -> bool {
    let trimmed = line.trim();
    let stripped = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let inner = stripped.strip_suffix('|').unwrap_or(stripped);
    if inner.is_empty() {
        return false;
    }
    inner.split('|').all(|segment| {
        let seg = segment.trim().trim_matches(':');
        seg.len() >= 3 && seg.chars().all(|c| c == '-')
    })
}

/// Parse a table row into cells.
fn parse_table_row(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    // Strip leading and trailing pipes
    let stripped = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let inner = stripped.strip_suffix('|').unwrap_or(stripped);

    inner.split('|').map(|s| s.trim().to_string()).collect()
}

/// Compute column widths by scanning all rows of a table block.
fn compute_table_col_widths(table_lines: &[&str], max_width: usize) -> Vec<usize> {
    let mut widths: Vec<usize> = Vec::new();

    for line in table_lines {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            break;
        }
        if is_table_separator(trimmed) {
            continue;
        }
        let cells = parse_table_row(trimmed);
        if widths.len() < cells.len() {
            widths.resize(cells.len(), 0);
        }
        for (j, cell) in cells.iter().enumerate() {
            widths[j] = widths[j].max(cell.len());
        }
    }

    if widths.is_empty() {
        return widths;
    }

    // Clamp total width
    let total: usize = widths.iter().sum::<usize>() + widths.len().saturating_sub(1) * 3;
    if total > max_width {
        let scale = max_width as f64 / total as f64;
        for w in &mut widths {
            *w = ((*w as f64) * scale).max(3.0) as usize;
        }
    }

    widths
}

/// Render a table header row with bold styling.
fn render_table_header(cells: &[String], col_widths: &[usize], theme: &Theme) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    for (j, cell) in cells.iter().enumerate() {
        let w = col_widths.get(j).copied().unwrap_or(cell.len());
        let padded = format!("{:<width$}", cell, width = w);

        spans.push(Span::styled(
            padded,
            Style::default().fg(theme.title).bold(),
        ));

        if j < cells.len() - 1 {
            spans.push(Span::styled(
                " \u{2502} ",
                Style::default().fg(theme.border),
            ));
        }
    }

    Line::from(spans)
}

/// Render a table data row with status-aware coloring.
fn render_table_row(cells: &[String], col_widths: &[usize], theme: &Theme) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    for (j, cell) in cells.iter().enumerate() {
        let w = col_widths.get(j).copied().unwrap_or(cell.len());
        let padded = format!("{:<width$}", cell, width = w);

        let style = status_color_for_cell(cell, theme);
        spans.push(Span::styled(padded, style));

        if j < cells.len() - 1 {
            spans.push(Span::styled(
                " \u{2502} ",
                Style::default().fg(theme.border),
            ));
        }
    }

    Line::from(spans)
}

/// Map status-like text in a cell to a color.
fn status_color_for_cell(text: &str, theme: &Theme) -> Style {
    let lower = text.trim().to_lowercase();

    // Check for status keywords
    if lower.contains("done") || lower.contains("completed") || lower.contains("\u{2705}") {
        return Style::default().fg(theme.running); // green
    }
    if lower.contains("implementing")
        || lower.contains("in_progress")
        || lower.contains("in-progress")
        || lower.contains("running")
        || lower.contains("\u{1f7e1}")
    {
        return Style::default().fg(theme.waiting); // yellow
    }
    if lower.contains("reviewing") || lower.contains("pending") {
        return Style::default().fg(theme.accent);
    }
    if lower.contains("blocked") || lower.contains("failed") || lower.contains("\u{274c}") {
        return Style::default().fg(theme.error); // red
    }
    if lower.contains("not-started") || lower.contains("not_started") || lower.contains("skipped") {
        return Style::default().fg(theme.dimmed);
    }

    Style::default().fg(theme.text)
}

/// Soft-wrap a single logical line into multiple display Lines.
/// `indent` is prepended to continuation lines (e.g. "    " for bullets).
fn wrap_line(text: &str, style: Style, width: usize, indent: &str) -> Vec<Line<'static>> {
    if width == 0 || text.is_empty() {
        return vec![Line::from(Span::styled(text.to_string(), style))];
    }

    let mut result = Vec::new();
    let mut remaining = text;
    let mut first = true;

    while !remaining.is_empty() {
        let max = if first {
            width
        } else {
            width.saturating_sub(indent.len())
        };
        if remaining.len() <= max {
            let line_text = if first {
                remaining.to_string()
            } else {
                format!("{}{}", indent, remaining)
            };
            result.push(Line::from(Span::styled(line_text, style)));
            break;
        }

        // Find a word boundary to break at
        let break_at = remaining[..max]
            .rfind(' ')
            .map(|pos| pos + 1)
            .unwrap_or(max);

        let (chunk, rest) = remaining.split_at(break_at);
        let line_text = if first {
            chunk.trim_end().to_string()
        } else {
            format!("{}{}", indent, chunk.trim_end())
        };
        result.push(Line::from(Span::styled(line_text, style)));
        remaining = rest.trim_start();
        first = false;
    }

    if result.is_empty() {
        result.push(Line::from(Span::styled(String::new(), style)));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_table_separator() {
        assert!(is_table_separator("|---|---|"));
        assert!(is_table_separator("| --- | --- |"));
        assert!(is_table_separator("|:---|---:|"));
        assert!(is_table_separator("|:---:|:---:|"));
        assert!(!is_table_separator("| foo | bar |"));
        assert!(!is_table_separator("not a table"));
        // Requires at least 3 dashes per segment
        assert!(!is_table_separator("|-|-|"));
        assert!(!is_table_separator("|--|--|"));
    }

    #[test]
    fn test_parse_table_row() {
        let cells = parse_table_row("| foo | bar | baz |");
        assert_eq!(cells, vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn test_parse_table_row_no_trailing_pipe() {
        let cells = parse_table_row("| foo | bar");
        assert_eq!(cells, vec!["foo", "bar"]);
    }

    #[test]
    fn test_status_color_detection() {
        let theme = Theme::default();

        // "done" should get running (green) color
        let style = status_color_for_cell("done", &theme);
        assert_eq!(style.fg, Some(theme.running));

        // "implementing" should get waiting (yellow) color
        let style = status_color_for_cell("implementing", &theme);
        assert_eq!(style.fg, Some(theme.waiting));

        // "not-started" should get dimmed color
        let style = status_color_for_cell("not-started", &theme);
        assert_eq!(style.fg, Some(theme.dimmed));

        // Regular text should get text color
        let style = status_color_for_cell("hello world", &theme);
        assert_eq!(style.fg, Some(theme.text));
    }

    #[test]
    fn test_parse_state_md_headers() {
        let content = "# Title\n## Section\n### Subsection\n";
        let theme = Theme::default();
        let lines = parse_state_md(content, &theme, 80);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_parse_state_md_table() {
        let content =
            "| Task | Status |\n|---|---|\n| task-01 | done |\n| task-02 | implementing |\n";
        let theme = Theme::default();
        let lines = parse_state_md(content, &theme, 80);
        // Header + 2 data rows (separator skipped)
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_parse_state_md_empty() {
        let theme = Theme::default();
        let lines = parse_state_md("", &theme, 80);
        assert!(lines.is_empty());
    }

    #[test]
    fn test_wrap_line_short() {
        let style = Style::default();
        let result = wrap_line("hello world", style, 80, "");
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_wrap_line_long() {
        let style = Style::default();
        let result = wrap_line("this is a longer sentence that should wrap", style, 20, "");
        assert!(result.len() > 1);
        // Each line should fit within width
        for line in &result {
            assert!(line.width() <= 20);
        }
    }

    #[test]
    fn test_wrap_line_with_indent() {
        let style = Style::default();
        let result = wrap_line(
            "  • some bullet text that is long enough to wrap around",
            style,
            25,
            "    ",
        );
        assert!(result.len() > 1);
        // Continuation lines get indented
        if result.len() > 1 {
            let second = &result[1];
            let text = second
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>();
            assert!(text.starts_with("    "));
        }
    }

    #[test]
    fn test_first_data_row_not_bold() {
        let content = "| Task | Status |\n|---|---|\n| task-01 | done |\n| task-02 | done |\n";
        let theme = Theme::default();
        let lines = parse_state_md(content, &theme, 80);
        // Header (bold) + 2 data rows (not bold)
        assert_eq!(lines.len(), 3);
        // First data row should NOT be bold
        let first_data = &lines[1];
        for span in &first_data.spans {
            assert!(
                !span.style.add_modifier.contains(Modifier::BOLD),
                "First data row should not be bold"
            );
        }
    }

    #[test]
    fn test_compute_col_widths() {
        let table = vec!["| foo | barbaz |", "|---|---|", "| a | b |"];
        let widths = compute_table_col_widths(&table, 80);
        assert_eq!(widths, vec![3, 6]); // "foo"=3, "barbaz"=6
    }
}
