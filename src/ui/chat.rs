use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

pub fn draw_messaging_screen(
    f: &mut Frame<'_>,
    area: Rect,
    addr: &str,
    name: &str,
    channels: &[String],
    selected_channel: usize,
    current_channel: &str,
    messages: &[(String, String)],
    input: &str,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(area);

    let header = Paragraph::new(format!("Connected to {} ({}) | Channel: {}", name, addr, current_channel))
        .block(Block::default().title("Messaging").borders(Borders::ALL));
    f.render_widget(header, chunks[0]);

    let tabs_area = chunks[1];
    super::channels::draw_channel_tabs(f, tabs_area, channels, selected_channel);

    let messages_text = messages
        .iter()
        .map(|(sender, text)| format!("{}: {}", sender, text))
        .collect::<Vec<_>>()
        .join("\n");
    let messages_paragraph = Paragraph::new(messages_text)
        .block(Block::default().title("Messages").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    f.render_widget(messages_paragraph, chunks[2]);

    let input_paragraph = Paragraph::new(input.to_string())
        .block(Block::default().title("Type message and press Enter").borders(Borders::ALL))
        .style(Style::default().fg(Color::Yellow));
    f.render_widget(input_paragraph, chunks[3]);
}

pub fn draw_settings_screen(f: &mut Frame<'_>, area: Rect) {
    let block = Block::default().title("Settings").borders(Borders::ALL);
    let paragraph = Paragraph::new("Settings are coming soon. Press Esc to return.")
        .block(block)
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}
