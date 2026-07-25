use crossterm::event::{self, Event as CEvent, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::{backend::CrosstermBackend, Terminal};
use ratatui::widgets::ListState;
use std::io::{self, Write};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::{self, Duration};

mod device_list;
mod pin_entry;

use device_list::draw_device_list;
use pin_entry::{draw_pin_entry, PinButton};

#[derive(Clone, PartialEq, Eq)]
enum UiMode {
    DeviceList,
    PinEntry {
        addr: String,
        name: String,
        input: String,
        button: PinButton,
    },
}

pub async fn run(
    mut rx: Receiver<String>,
    cmd_tx: Sender<String>,
    prompt_resp_tx: Sender<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    enable_raw_mode()?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut devices: Vec<(String, String)> = Vec::new();
    let mut selected = ListState::default();
    selected.select(Some(0));
    let mut mode = UiMode::DeviceList;
    let mut connection_status: Option<String> = None;
    let mut logs: Vec<String> = Vec::new();
    let mut tick = time::interval(Duration::from_millis(200));

    loop {
        tokio::select! {
            maybe = rx.recv() => {
                match maybe {
                    Some(s) => {
                        if let Some(prompt) = s.strip_prefix("PROMPT:") {
                            disable_raw_mode()?;
                            print!("{}: ", prompt);
                            io::stdout().flush()?;
                            let mut input = String::new();
                            io::stdin().read_line(&mut input)?;
                            enable_raw_mode()?;
                            let _ = prompt_resp_tx.send(input.trim().to_string()).await;
                            logs.push(format!("Answered prompt: {}", prompt));
                        } else if let Some(device) = s.strip_prefix("DEVICE:") {
                            let mut parts = device.splitn(2, '|');
                            if let (Some(addr), Some(name)) = (parts.next(), parts.next()) {
                                if !devices.iter().any(|(a, _)| a == addr) {
                                    devices.push((addr.to_string(), name.to_string()));
                                    logs.push(format!("Added device {} ({})", name, addr));
                                }
                                if selected.selected().is_none() {
                                    selected.select(Some(0));
                                }
                            }
                        } else if let Some(conn) = s.strip_prefix("CONN:") {
                            connection_status = Some(conn.to_string());
                            logs.push(conn.to_string());
                        } else if let Some(err) = s.strip_prefix("CONN_FAIL:") {
                            connection_status = Some(format!("Connection failed: {}", err));
                            logs.push(format!("Connection failed: {}", err));
                        } else if let Some(status) = s.strip_prefix("STATUS:") {
                            logs.push(status.to_string());
                        } else if let Some(log) = s.strip_prefix("LOG:") {
                            logs.push(log.to_string());
                        } else {
                            logs.push(s);
                        }
                        if logs.len() > 200 {
                            logs.drain(0..logs.len() - 200);
                        }
                    }
                    None => break Ok(()),
                }
            }
            _ = tick.tick() => {}
        }

        terminal.draw(|f| {
            let size = f.area();
            match &mode {
                UiMode::DeviceList => draw_device_list(f, size, &devices, &selected, &logs, connection_status.as_deref()),
                UiMode::PinEntry { addr, name, input, button } => {
                    draw_pin_entry(f, size, addr, name, input, *button);
                }
            }
        })?;

        while event::poll(Duration::from_millis(10))? {
            if let CEvent::Key(key) = event::read()? {
                match &mut mode {
                    UiMode::DeviceList => match key.code {
                        KeyCode::Char('q') => {
                            disable_raw_mode()?;
                            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                            terminal.show_cursor()?;
                            return Ok(());
                        }
                        KeyCode::Down => {
                            if !devices.is_empty() {
                                let next = selected.selected().unwrap_or(0).saturating_add(1);
                                let next = next.min(devices.len() - 1);
                                selected.select(Some(next));
                            }
                        }
                        KeyCode::Up => {
                            if !devices.is_empty() {
                                let next = selected.selected().unwrap_or(0).saturating_sub(1);
                                selected.select(Some(next));
                            }
                        }
                        KeyCode::Enter => {
                            if let Some(index) = selected.selected() {
                                if let Some((addr, name)) = devices.get(index) {
                                    mode = UiMode::PinEntry {
                                        addr: addr.clone(),
                                        name: name.clone(),
                                        input: String::new(),
                                        button: PinButton::Continue,
                                    };
                                }
                            }
                        }
                        _ => {}
                    },
                    UiMode::PinEntry { addr, name, input, button } => match key.code {
                        KeyCode::Char('q') => {
                            disable_raw_mode()?;
                            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                            terminal.show_cursor()?;
                            return Ok(());
                        }
                        KeyCode::Esc => {
                            mode = UiMode::DeviceList;
                        }
                        KeyCode::Tab | KeyCode::Right => {
                            *button = match *button {
                                PinButton::Back => PinButton::Continue,
                                PinButton::Continue => PinButton::Back,
                            };
                        }
                        KeyCode::Left => {
                            *button = match *button {
                                PinButton::Continue => PinButton::Back,
                                PinButton::Back => PinButton::Continue,
                            };
                        }
                        KeyCode::Backspace => {
                            input.pop();
                        }
                        KeyCode::Enter => {
                            if *button == PinButton::Back {
                                mode = UiMode::DeviceList;
                            } else {
                                if input.trim().is_empty() {
                                    logs.push("PIN cannot be empty".to_string());
                                } else {
                                    let _ = prompt_resp_tx.send(input.trim().to_string()).await;
                                    let cmd = format!("connect {}", addr);
                                    let _ = cmd_tx.send(cmd).await;
                                    logs.push(format!("Connecting to {} ({})", name, addr));
                                    mode = UiMode::DeviceList;
                                }
                            }
                        }
                        KeyCode::Char(c) => {
                            if !c.is_control() && input.len() < 32 {
                                input.push(c);
                            }
                        }
                        _ => {}
                    },
                }
            }
        }
    }
}
