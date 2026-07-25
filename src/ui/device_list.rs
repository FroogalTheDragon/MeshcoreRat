use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use std::collections::HashMap;

pub fn draw_device_list(
    f: &mut Frame<'_>,
    area: Rect,
    devices: &[(String, String)], // (addr, name)
    selected: &mut ListState,
    logs: &[String],
    connection_map: &HashMap<String, String>,
) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
        .split(area);

    let device_items: Vec<ListItem> = if devices.is_empty() {
        vec![ListItem::new("No MeshCore devices discovered yet")]
    } else {
        devices
            .iter()
            .map(|(addr, name)| {
                let status = connection_map.get(addr).map(|s| format!(" [{}]", s)).unwrap_or_default();
                ListItem::new(format!("{} ({}){}", name, addr, status))
            })
            .collect()
    };

    let device_list = List::new(device_items)
        .block(Block::default().title("Devices").borders(Borders::ALL))
        .highlight_style(Style::default().fg(Color::Black).bg(Color::LightGreen).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");
    f.render_stateful_widget(device_list, chunks[0], selected);

    let right_chunks = if selected.selected().is_some() {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
            .split(chunks[1])
    } else {
        vec![chunks[1]].into()
    };
    // Show the selected device connection status
    if let Some(sel) = selected.selected() {
        if let Some((addr, _name)) = devices.get(sel) {
            let status = connection_map.get(addr).map(|s| s.as_str()).unwrap_or("");
            let status_block = Paragraph::new(status)
                .block(Block::default().title("Connection").borders(Borders::ALL))
                .style(Style::default().fg(Color::White).bg(Color::DarkGray));
            f.render_widget(status_block, right_chunks[0]);
        }
    }

    let log_area = if selected.selected().is_some() {
        right_chunks[1]
    } else {
        chunks[1]
    };

    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)].as_ref())
        .split(log_area);

    let log_items: Vec<ListItem> = logs.iter().rev().map(|l| ListItem::new(l.as_str())).collect();
    let log_list = List::new(log_items).block(Block::default().title("Logs").borders(Borders::ALL));
    f.render_widget(log_list, areas[0]);

    let help = Paragraph::new("Keys: r/R Refresh | d Disconnect | Enter Connect | q Quit | ↑/↓ Navigate")
        .block(Block::default().borders(Borders::ALL).title("Keys"))
        .style(Style::default().fg(Color::White).bg(Color::DarkGray));
    f.render_widget(help, areas[1]);
}
