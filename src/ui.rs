//! UI event handling and device list rendering.
//!
//! This module drives the TUI and handles messages from the Bluetooth subsystem.

use crossterm::event::{self, Event as CEvent, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::{backend::CrosstermBackend, Terminal};
use ratatui::widgets::ListState;
use std::io::{self, Write};
use tokio::sync::mpsc::{Receiver, Sender};
use std::collections::HashMap;
use tokio::time::{self, Duration};

mod channels;
mod chat;
mod device_list;
mod pin_entry;

use chat::{draw_messaging_screen, draw_settings_screen};
use device_list::draw_device_list;
use pin_entry::{draw_pin_entry, PinButton};

/// Insert a device into the list with its MAC address displayed.
///
/// Every device line is shown as `name (MAC)` so the address is always visible.
pub fn insert_device_display_name(
    devices: &mut Vec<(String, String, String)>,
    addr: String,
    raw_name: String,
) -> bool {
    if devices.iter().any(|(existing_addr, _, _)| existing_addr == &addr) {
        return false;
    }

    let display_name = format!("{} ({})", raw_name, addr);
    devices.push((addr, raw_name, display_name));
    true
}

#[derive(Clone, PartialEq, Eq)]
enum UiMode {
    DeviceList,
    PinEntry {
        addr: String,
        name: String,
        input: String,
        button: PinButton,
    },
    Confirm {
        addr: String,
        name: String,
    },
    Messaging {
        addr: String,
        name: String,
        channels: Vec<String>,
        selected_channel: usize,
        current_channel: String,
        messages: Vec<(String, String)>,
        input: String,
    },
    Settings {
        return_to: Box<UiMode>,
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

    let mut devices: Vec<(String, String, String)> = Vec::new(); // (addr, raw_name, display_name)
    let mut selected = ListState::default();
    selected.select(Some(0));
    let mut mode = UiMode::DeviceList;
    let mut connection_map: HashMap<String, String> = HashMap::new();
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
                                let addr_string = addr.to_string();
                                let raw_name = name.to_string();
                                if insert_device_display_name(&mut devices, addr_string.clone(), raw_name.clone()) {
                                    logs.push(format!("Added device {}", devices.last().unwrap().2));
                                }
                                if selected.selected().is_none() {
                                    selected.select(Some(0));
                                }
                            }
                        } else if let Some(conn) = s.strip_prefix("CONN:") {
                            // e.g. "Connected to <addr>"
                            if let Some(addr) = conn.trim_start_matches("Connected to ").trim().split_whitespace().next() {
                                connection_map.insert(addr.to_string(), "Connected".to_string());
                            }
                            logs.push(conn.to_string());
                        } else if let Some(err) = s.strip_prefix("CONN_FAIL:") {
                            logs.push(format!("Connection failed: {}", err));
                            // try to capture an address where possible
                            if err.starts_with("Device ") {
                                if let Some(part) = err.split_whitespace().nth(1) {
                                    connection_map.insert(part.to_string(), "Connection failed".to_string());
                                }
                            }
                        } else if let Some(d) = s.strip_prefix("DISCONN:") {
                            // e.g. "Disconnected from <addr>"
                            if let Some(addr) = d.trim_start_matches("Disconnected from ").trim().split_whitespace().next() {
                                connection_map.remove(addr);
                            }
                            logs.push(d.to_string());
                        } else if let Some(err) = s.strip_prefix("DISCONN_FAIL:") {
                            logs.push(format!("Disconnect failed: {}", err));
                            if err.starts_with("Device ") {
                                if let Some(part) = err.split_whitespace().nth(1) {
                                    connection_map.insert(part.to_string(), "Disconnect failed".to_string());
                                }
                            }
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
            UiMode::DeviceList => draw_device_list(f, size, &devices, &mut selected, &logs, &connection_map),
            UiMode::PinEntry { addr, name, input, button } => {
                draw_pin_entry(f, size, addr, name, input, *button);
            }
            UiMode::Confirm { addr, name } => {
                // draw underlying device list
                draw_device_list(f, size, &devices, &mut selected, &logs, &connection_map);
                // overlay a small centered confirmation box
                let area = f.area();
                let w = (area.width / 2).max(20);
                let h = 5u16;
                let x = (area.width.saturating_sub(w)) / 2;
                let y = (area.height.saturating_sub(h)) / 2;
                let rect = ratatui::layout::Rect::new(x, y, w, h);
                let para = ratatui::widgets::Paragraph::new(format!("Disconnect {} ({})? (y/n)", name, addr))
                    .block(ratatui::widgets::Block::default().title("Confirm").borders(ratatui::widgets::Borders::ALL));
                f.render_widget(para, rect);
            }
            UiMode::Messaging { addr, name, channels, selected_channel, current_channel, messages, input } => {
                draw_messaging_screen(f, size, addr, name, channels, *selected_channel, current_channel, messages, input);
            }
            UiMode::Settings { .. } => {
                draw_settings_screen(f, size);
            }
        }
        })?;

        while event::poll(Duration::from_millis(10))? {
            if let CEvent::Key(key) = event::read()? {
                match &mut mode {
                    UiMode::DeviceList => match key.code {
                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            println!("Refresh Device List");
                            let _ = cmd_tx.send("refresh".to_string()).await;
                            logs.push("Refresh device list requested by user".to_string());
                        }
                        KeyCode::Char('d') => {
                            if let Some(index) = selected.selected()
                                && let Some((addr, _, display_name)) = devices.get(index)
                            {
                                mode = UiMode::Confirm { addr: addr.clone(), name: display_name.clone() };
                            }
                        }
                        KeyCode::Char('q') => {
                            disable_raw_mode()?;
                            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                            terminal.show_cursor()?;
                            return Ok(());
                        }
                        KeyCode::Down if !devices.is_empty() => {
                            let next = selected.selected().unwrap_or(0).saturating_add(1);
                            let next = next.min(devices.len() - 1);
                            selected.select(Some(next));
                        }
                        KeyCode::Up if !devices.is_empty() => {
                            let next = selected.selected().unwrap_or(0).saturating_sub(1);
                            selected.select(Some(next));
                        }
                        KeyCode::Enter => {
                            if let Some(index) = selected.selected()
                                && let Some((addr, _, display_name)) = devices.get(index)
                            {
                                mode = UiMode::PinEntry {
                                    addr: addr.clone(),
                                    name: display_name.clone(),
                                    input: String::new(),
                                    button: PinButton::Continue,
                                };
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
                                    mode = UiMode::Messaging {
                                        addr: addr.clone(),
                                        name: name.clone(),
                                        channels: vec!["public".to_string(), "private".to_string()],
                                        selected_channel: 0,
                                        current_channel: "public".to_string(),
                                        messages: vec![("system".to_string(), "Welcome to Meshy chat.".to_string())],
                                        input: String::new(),
                                    };
                                }
                            }
                        }
                        KeyCode::Char(c) if !c.is_control() && input.len() < 32 => {
                            input.push(c);
                        }
                        _ => {}
                    },
                    UiMode::Confirm { addr, name } => match key.code {
                        KeyCode::Char('y') => {
                            let cmd = format!("disconnect {}", addr);
                            let _ = cmd_tx.send(cmd).await;
                            logs.push(format!("Disconnect confirmed for {} ({})", name, addr));
                            mode = UiMode::DeviceList;
                        }
                        KeyCode::Char('n') | KeyCode::Esc => {
                            logs.push(format!("Disconnect canceled for {} ({})", name, addr));
                            mode = UiMode::DeviceList;
                        }
                        _ => {}
                    },
                    UiMode::Messaging { addr, name, channels, selected_channel, current_channel, messages, input } => match key.code {
                        KeyCode::Esc => {
                            mode = UiMode::DeviceList;
                        }
                        KeyCode::Char('s') => {
                            mode = UiMode::Settings {
                                return_to: Box::new(UiMode::Messaging {
                                    addr: addr.clone(),
                                    name: name.clone(),
                                    channels: channels.clone(),
                                    selected_channel: *selected_channel,
                                    current_channel: current_channel.clone(),
                                    messages: messages.clone(),
                                    input: input.clone(),
                                }),
                            };
                        }
                        KeyCode::Tab => {
                            *selected_channel = (*selected_channel + 1) % channels.len();
                            *current_channel = channels[*selected_channel].clone();
                        }
                        KeyCode::Enter => {
                            if !input.trim().is_empty() {
                                messages.push((name.clone(), input.clone()));
                                input.clear();
                            }
                        }
                        KeyCode::Backspace => {
                            input.pop();
                        }
                        KeyCode::Char(c) if !c.is_control() => {
                            input.push(c);
                        }
                        _ => {}
                    },
                    UiMode::Settings { return_to } => match key.code {
                        KeyCode::Esc => {
                            mode = *return_to.clone();
                        }
                        KeyCode::Char('q') => {
                            disable_raw_mode()?;
                            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                            terminal.show_cursor()?;
                            return Ok(());
                        }
                        _ => {}
                    },
                }
            }
        }
    }
}
