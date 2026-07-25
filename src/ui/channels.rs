use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, Tabs};
use ratatui::Frame;

pub fn draw_channel_tabs(
    f: &mut Frame<'_>,
    area: Rect,
    channels: &[String],
    selected_channel: usize,
) {
    let titles = channels.iter().map(|name| name.as_str()).collect::<Vec<_>>();
    let tabs = Tabs::new(titles)
        .block(Block::default().title("Channels").borders(Borders::ALL))
        .highlight_style(Style::default().fg(Color::Black).bg(Color::LightGreen).add_modifier(Modifier::BOLD))
        .select(selected_channel);
    f.render_widget(tabs, area);
}

pub fn draw_channel_list(
    f: &mut Frame<'_>,
    area: Rect,
    channels: &[String],
    selected_channel: usize,
) {
    let items: Vec<ListItem> = channels.iter().map(|c| ListItem::new(c.clone())).collect();
    let list = List::new(items)
        .block(Block::default().title("Subscribed Channels").borders(Borders::ALL))
        .highlight_style(Style::default().fg(Color::Black).bg(Color::LightGreen))
        .highlight_symbol("▶ ");
    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(selected_channel));
    f.render_stateful_widget(list, area, &mut state);
}
