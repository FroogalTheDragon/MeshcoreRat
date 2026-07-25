use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::Frame;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::text::Text;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PinButton {
    Back,
    Continue,
}

pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(vertical[1])[1]
}

pub fn draw_pin_entry(
    f: &mut Frame<'_>,
    area: Rect,
    addr: &str,
    name: &str,
    input: &str,
    button: PinButton,
) {
    let area = centered_rect(60, 50, area);
    let block = Block::default()
        .title("Enter PIN")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    f.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .margin(2)
        .split(area);

    let title = Paragraph::new(Text::from(format!("Enter PIN for {} ({})", name, addr)))
        .block(Block::default());
    f.render_widget(title, inner[0]);

    let input_block = Paragraph::new(Text::from(input.to_string()))
        .block(Block::default().title("PIN").borders(Borders::ALL))
        .style(Style::default().fg(Color::White).bg(Color::Black));
    f.render_widget(input_block, inner[1]);

    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(inner[3]);

    let back_style = if button == PinButton::Back {
        Style::default().fg(Color::Black).bg(Color::LightGreen).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let continue_style = if button == PinButton::Continue {
        Style::default().fg(Color::Black).bg(Color::LightGreen).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let back = Paragraph::new(Text::from("< Back "))
        .style(back_style)
        .block(Block::default().borders(Borders::ALL));
    let cont = Paragraph::new(Text::from(" Continue >"))
        .style(continue_style)
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(back, buttons[0]);
    f.render_widget(cont, buttons[1]);

    let hint = Paragraph::new(Text::from(
        "Type PIN and use ←/→ or Tab to switch buttons. Enter to activate.",
    ))
    .block(Block::default());
    f.render_widget(hint, inner[2]);
}
