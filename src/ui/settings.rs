use crate::model::{Model, SettingsField};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

/// Render the Settings popup. The popup fills the screen with a 3-cell padding
/// on every side; the directory list inside it scrolls when it overflows.
///
/// Styling is intentionally minimal: only shortcut keys (`[Key]`) are bold
/// yellow. Everything else uses the default terminal colors; the active field
/// is indicated with bold text rather than a color.
pub fn render_settings(f: &mut Frame, area: Rect, model: &Model) {
    let s = &model.ui.settings;

    // Full screen minus a 3-cell padding on each side.
    const PAD: u16 = 3;
    if area.width <= PAD * 2 + 4 || area.height <= PAD * 2 + 6 {
        return;
    }
    let popup = Rect::new(
        area.x + PAD,
        area.y + PAD,
        area.width - PAD * 2,
        area.height - PAD * 2,
    );

    f.render_widget(Clear, popup);

    // Match the active-panel border color so the popup feels like a focused panel.
    let block = Block::default()
        .title(" Settings ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    // The directory list flexes to fill the space; everything else is a fixed
    // height. The "add" row sits at the top, and the help keymaps are pinned to
    // the very bottom with a separator line directly above them (no trailing gap).
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // section label
            Constraint::Length(1), // new-dir input (always visible)
            Constraint::Min(1),    // directory list (scrollable)
            Constraint::Length(1), // spacer
            Constraint::Length(1), // use cwd toggle
            Constraint::Length(1), // separator
            Constraint::Length(3), // help lines (pinned to bottom)
        ])
        .split(inner);

    let list_active = s.field == SettingsField::DirList;
    let new_active = s.field == SettingsField::NewDir;
    let cwd_active = s.field == SettingsField::UseCurrentDir;

    // Section label
    f.render_widget(
        Paragraph::new(Line::from("Scanned directories (first = download target):")),
        rows[0],
    );

    // New directory input — always visible, sits above the list.
    let input_text = if s.new_dir.is_empty() {
        Span::raw("Type a path here...").dim()
    } else {
        Span::raw(s.new_dir.as_str())
    };
    let mut input_line = vec![Span::raw("+ "), input_text];
    if new_active {
        input_line.push(Span::raw("_").rapid_blink());
    }
    let input_style = if new_active {
        Style::default().bold()
    } else {
        Style::default()
    };
    f.render_widget(
        Paragraph::new(Line::from(input_line)).style(input_style),
        rows[1],
    );

    // Directory list (scrollable)
    render_dir_list(f, rows[2], model, list_active);

    // Use-current-directory checkbox
    let checkbox = if s.use_current_dir { "[x]" } else { "[ ]" };
    let cwd_style = if cwd_active {
        Style::default().bold()
    } else {
        Style::default()
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            format!("{checkbox} Also use the working directory (where tusic was started)"),
            cwd_style,
        )])),
        rows[4],
    );

    // Separator line directly above the help keymaps.
    f.render_widget(
        Paragraph::new(Line::from(
            Span::raw("─".repeat(rows[5].width as usize)).dim(),
        )),
        rows[5],
    );

    // Help lines — every shortcut shown as a bold-yellow [Key].
    let help = if s.editing.is_some() {
        Paragraph::new(vec![
            help_line(&[("Enter", "confirm edit"), ("Esc", "cancel edit")]),
            Line::from("  (clear the text and press Enter to remove the entry)"),
        ])
    } else {
        Paragraph::new(vec![
            help_line(&[
                ("↑/↓", "move between settings"),
                ("Space", "toggle checkbox"),
            ]),
            help_line(&[("e", "edit path"), ("d", "remove"), ("p", "make primary")]),
            help_line(&[("Enter", "save & close"), ("Esc", "cancel")]),
        ])
    };
    f.render_widget(help, rows[6]);
}

fn render_dir_list(f: &mut Frame, area: Rect, model: &Model, active: bool) {
    let s = &model.ui.settings;

    if s.dirs.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(
                Span::raw("  (no directories — add one with the row above)").dim(),
            )),
            area,
        );
        return;
    }

    let height = area.height.max(1) as usize;

    // Window the list around the selected entry so it scrolls when overflowing.
    let start = if s.selected >= height {
        s.selected + 1 - height
    } else {
        0
    };
    let end = (start + height).min(s.dirs.len());

    let more_above = start > 0;
    let more_below = end < s.dirs.len();

    let lines: Vec<Line> = (start..end)
        .map(|i| {
            let dir = &s.dirs[i];
            let editing = s.editing == Some(i);
            let is_sel = active && i == s.selected;
            let primary = i == 0;

            // Scroll hints replace the marker on the first/last visible rows.
            let marker = if editing {
                "✎ "
            } else if i == start && more_above {
                "▲ "
            } else if i == end - 1 && more_below {
                "▼ "
            } else if is_sel {
                "> "
            } else {
                "- "
            };
            let tag = if primary { " [primary]" } else { "" };

            // While editing this entry, show the working buffer with a caret.
            if editing {
                return Line::from(vec![
                    Span::raw(marker),
                    Span::raw(s.edit_buf.clone()).bold(),
                    Span::raw(symbols::block::FULL).rapid_blink(),
                    Span::raw(tag),
                ]);
            }

            // Mirror the active-track color (green + bold) for the selected path.
            let style = if is_sel {
                Style::default().fg(Color::Green).bold()
            } else {
                Style::default()
            };

            Line::from(vec![
                Span::raw(marker),
                Span::styled(dir.clone(), style),
                Span::raw(tag),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(lines), area);
}

/// Build a help line from `[Key] description` pairs, each key bold yellow.
fn help_line<'a>(pairs: &[(&'a str, &'a str)]) -> Line<'a> {
    let mut spans = Vec::new();
    for (key, desc) in pairs {
        spans.push(Span::styled(
            format!("[{key}] "),
            Style::default().fg(Color::Yellow).bold(),
        ));
        spans.push(Span::raw(format!("{desc}   ")));
    }
    Line::from(spans)
}
