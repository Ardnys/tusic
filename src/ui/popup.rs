use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::model::Model;

/// Centered "are you sure?" popup shown before a track is deleted from disk.
pub fn render_delete_confirm(f: &mut Frame, area: Rect, model: &Model) {
    let Some(idx) = model.ui.confirm_delete else {
        return;
    };
    let name = model
        .playlist
        .tracks()
        .get(idx)
        .map(|t| t.display_name())
        .unwrap_or_default();

    // Centered popup, capped at 60 cols / 8 rows.
    let w = area.width.clamp(20, 60);
    let h = 8u16.min(area.height);
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    let popup = Rect::new(x, y, w, h);

    f.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Şarkıyı sil ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let lines = vec![
        Line::from(Span::raw(
            "Bu şarkıyı diskten silmek istediğinize emin misiniz?",
        )),
        Line::from(Span::styled(
            name,
            Style::default().fg(Color::Yellow).bold(),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("[y] ", Style::default().fg(Color::Green).bold()),
            Span::raw("Evet, sil   "),
            Span::styled("[n/Esc] ", Style::default().fg(Color::Green).bold()),
            Span::raw("İptal"),
        ]),
    ];
    let p = Paragraph::new(lines).block(Block::default());
    f.render_widget(p, inner);
}
