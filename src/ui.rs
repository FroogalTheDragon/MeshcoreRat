use crossterm::event::{self, Event as CEvent, KeyCode};
use ratatui::{backend::CrosstermBackend, Terminal};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::{Block, Borders, List, ListItem};
use std::io::{self};
use tokio::sync::mpsc::Receiver;
use tokio::time::{self, Duration};

pub async fn run(mut rx: Receiver<String>) -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    let mut stdout = io::stdout();
    crossterm::terminal::enable_raw_mode()?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    terminal.clear()?; // Clear terminal before rendering main window

    let mut logs: Vec<String> = Vec::new();

    let mut tick = time::interval(Duration::from_millis(200));

    loop {
        tokio::select! {
            maybe = rx.recv() => {
                match maybe {
                    Some(s) => {
                        logs.push(s);
                        if logs.len() > 200 { logs.remove(0); }
                    }
                    None => return Ok(()),
                }
            }
            _ = tick.tick() => {}
        }

        dbg!(&rx);

        // // draw
        // terminal.draw(|f| {
        //     let size = f.area();
        //     let chunks = Layout::default()
        //         .direction(Direction::Horizontal)
        //         .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
        //         .split(size);

        //     let left = Block::default().title("Status").borders(Borders::ALL);
        //     f.render_widget(left, chunks[0]);

        //     let items: Vec<ListItem> = logs.iter().rev().map(|l| ListItem::new(l.as_str())).collect();
        //     let list = List::new(items).block(Block::default().title("Logs").borders(Borders::ALL));
        //     f.render_widget(list, chunks[1]);
        // })?;

        // // handle input non-blocking
        // while crossterm::event::poll(Duration::from_millis(0))? {
        //     if let CEvent::Key(key) = event::read()? {
        //         match key.code {
        //             KeyCode::Char('q') => {
        //                 // restore terminal
        //                 crossterm::terminal::disable_raw_mode()?;
        //                 crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;
        //                 return Ok(());
        //             }
        //             _ => {}
        //         }
        //     }
        // }
    }
}
