use bluer::agent::Agent;
use bluer::{AdapterEvent, Address, Session};
use futures::StreamExt;
use std::error::Error;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Duration;
use std::collections::VecDeque;
use tracing::{debug, info, warn};

#[derive(Debug)]
pub enum AgentRequest {
    Pin(String, oneshot::Sender<String>),
    Passkey(String, oneshot::Sender<String>),
}

use tokio::sync::mpsc::Sender;

pub async fn run(session: Session, ui_tx: Option<Sender<String>>) -> Result<(), Box<dyn Error>> {
    // 1. Set up the Bluetooth Agent to handle PIN requests
    let (prompt_tx, prompt_rx) = mpsc::channel::<AgentRequest>(8);

    let mut agent = Agent::default();
    agent.request_default = true;

    // Define what happens when a device asks for a PIN - send a request to the dispatcher
    let pin_tx = prompt_tx.clone();
    agent.request_pin_code = Some(Box::new(move |req| {
        let pin_tx = pin_tx.clone();
        Box::pin(async move {
            let prompt = format!("Device {:?} is requesting a PIN.", req);
            let (resp_tx, resp_rx) = oneshot::channel();
            // If dispatcher is alive, send request and await response. Otherwise fallback to blocking read.
            if pin_tx.send(AgentRequest::Pin(prompt.clone(), resp_tx)).await.is_ok() {
                if let Ok(pin) = resp_rx.await {
                    return Ok(pin);
                }
            }

            // Fallback: blocking prompt (only used if dispatcher isn't running)
            let pin = tokio::task::spawn_blocking(move || {
                println!("{}", prompt);
                println!("Please enter the PIN:");
                let mut buffer = String::new();
                std::io::stdin().read_line(&mut buffer).unwrap();
                buffer.trim().to_string()
            })
            .await
            .unwrap();

            Ok(pin)
        })
    }));

    let pass_tx = prompt_tx.clone();
    agent.request_passkey = Some(Box::new(move |req| {
        let pass_tx = pass_tx.clone();
        Box::pin(async move {
            let prompt = format!("Device {:?} is requesting a Passkey (numeric PIN).", req);
            let (resp_tx, resp_rx) = oneshot::channel();
            if pass_tx.send(AgentRequest::Passkey(prompt.clone(), resp_tx)).await.is_ok() {
                if let Ok(pass_str) = resp_rx.await {
                    if let Ok(pk) = pass_str.trim().parse::<u32>() {
                        return Ok(pk);
                    }
                }
            }

            let passkey = tokio::task::spawn_blocking(move || {
                println!("{}", prompt);
                println!("Please enter the 6-digit Passkey:");
                let mut buffer = String::new();
                std::io::stdin().read_line(&mut buffer).unwrap();
                buffer.trim().parse::<u32>().unwrap_or(000000)
            })
            .await
            .unwrap();

            Ok(passkey)
        })
    }));

    // Register the agent and set it as the default so BlueZ routes requests to us
    let _agent_handle = session.register_agent(agent).await?;
    info!("Bluetooth Agent registered.");
    if let Some(tx) = &ui_tx {
        let _ = tx.send("Bluetooth Agent registered.".to_string()).await;
    }

    // 2. Initialize Adapter
    // Try to obtain the default adapter, retrying a few times rather than failing immediately.
    let adapter = loop {
        match session.default_adapter().await {
            Ok(a) => break a,
            Err(e) => {
                warn!("No Bluetooth adapter available yet: {}", e);
                if let Some(tx) = &ui_tx {
                    let _ = tx.send(format!("No Bluetooth adapter available yet: {}", e)).await;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    };

    if let Err(e) = adapter.set_powered(true).await {
        warn!("Failed to power adapter: {}", e);
        if let Some(tx) = &ui_tx {
            let _ = tx.send(format!("Failed to power adapter: {}", e)).await;
        }
    }

    // Ensure the adapter is capable of pairing!
    let _ = adapter.set_pairable(true).await;

    info!("Scanning for devices using {}...", adapter.name());
    if let Some(tx) = &ui_tx {
        let _ = tx.send(format!("Scanning on adapter {}", adapter.name())).await;
    }

    // 3. Start discovering devices
    let mut discover_events = adapter.discover_devices().await?;
    let mut target_addr: Option<Address> = None;

    while let Some(event) = discover_events.next().await {
        if let AdapterEvent::DeviceAdded(addr) = event {
            let device = adapter.device(addr)?;
            let name = device
                .name()
                .await?
                .unwrap_or_else(|| "Unknown".to_string());

            debug!("Discovered device {} ({})", name, addr);
            if let Some(tx) = &ui_tx {
                let _ = tx.send(format!("Discovered device {} ({})", name, addr)).await;
            }

            if name == "MeshCore-🐲Froogal" {
                target_addr = Some(addr);
                break;
            }
        }
    }

    // 4. Pair and Connect
    // Drop the discovery stream to ensure the adapter stops scanning
    drop(discover_events);

    if let Some(addr) = target_addr {
        let device = adapter.device(addr)?;

        info!("Found device {}. Waiting 4 seconds for BlueZ background queries to settle...", addr);
        if let Some(tx) = &ui_tx {
            let _ = tx.send(format!("Found device {}", addr)).await;
        }
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;

        if !device.is_paired().await? {
            info!("Pairing with {}... (Agent will automatically provide PIN)", addr);
            let _ = device.set_trusted(true).await;

            let mut attempts = 1;
            loop {
                match device.pair().await {
                    Ok(_) => {
                        info!("Successfully paired after {} attempts!", attempts);
                        if let Some(tx) = &ui_tx {
                            let _ = tx.send(format!("Paired after {} attempts", attempts)).await;
                        }
                        break;
                    }
                    Err(e) => {
                        warn!("Pairing attempt {} failed: {}", attempts, e);
                        if let Some(tx) = &ui_tx {
                            let _ = tx.send(format!("Pairing attempt {} failed: {}", attempts, e)).await;
                        }
                        attempts += 1;
                        // Keep trying indefinitely
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    }
                }
            }
        }

        info!("Connecting via BLE GATT...");
        device.connect().await?;

        info!("Connected! Searching for Nordic UART Service (NUS)...");

        // Wait a moment for GATT services to be resolved
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let mut tx_char = None;
        let mut rx_char = None;

        for service in device.services().await? {
            let uuid = service.uuid().await?;
            // Nordic UART Service UUID
            if uuid.to_string().to_uppercase() == "6E400001-B5A3-F393-E0A9-E50E24DCCA9E" {
                info!("Found Nordic UART Service!");

                for charac in service.characteristics().await? {
                    let cuuid = charac.uuid().await?;
                    if cuuid.to_string().to_uppercase() == "6E400002-B5A3-F393-E0A9-E50E24DCCA9E" {
                        info!("Found RX Characteristic (App -> Device)");
                        rx_char = Some(charac);
                    } else if cuuid.to_string().to_uppercase()
                        == "6E400003-B5A3-F393-E0A9-E50E24DCCA9E"
                    {
                        info!("Found TX Characteristic (Device -> App)");
                        tx_char = Some(charac);
                    }
                }
            }
        }

        if let (Some(rx), Some(tx)) = (rx_char, tx_char) {
                info!("Subscribing to TX notifications...");
                if let Some(tx) = &ui_tx {
                    let _ = tx.send("Subscribing to TX notifications".to_string()).await;
                }
            let notify = tx.notify().await?;

            println!("=====================================================");
            println!("  BLE BINARY / SERIAL TERMINAL OPEN");
            println!("  MeshCore uses a binary protocol: [Packet Type (1 byte)] [Data...]");
            println!("  Type '/hex 01 FF' to send raw binary bytes, or regular text to send ASCII.");
            println!("  Press Ctrl+C to exit.");
            println!("=====================================================");

            // Task 1: Read from device and print to stdout
            let read_task = tokio::spawn(async move {
                tokio::pin!(notify);
                while let Some(data) = notify.next().await {
                    // Try to print as text if it's perfectly valid UTF-8
                    if let Ok(text) = String::from_utf8(data.clone()) {
                        if text
                            .chars()
                            .all(|c| !c.is_control() || c == '\n' || c == '\r')
                        {
                            print!("{}", text);
                            use std::io::Write;
                            let _ = std::io::stdout().flush();
                            if let Some(tx) = &ui_tx {
                                let _ = tx.send(format!("RX text: {}", text)).await;
                            }
                            continue;
                        }
                    }

                    // Otherwise print as a hex array since it's a binary protocol
                    print!("Received (binary): ");
                    for b in &data {
                        print!("{:02X} ", b);
                    }
                    println!();
                    if let Some(tx) = &ui_tx {
                        let mut s = String::from("RX binary: ");
                        for b in &data { s.push_str(&format!("{:02X} ", b)); }
                        let _ = tx.send(s).await;
                    }

                    // Extract and print any hidden ASCII text inside the binary payload
                    let mut current_string = String::new();
                    let mut found_strings = Vec::new();
                    for &b in &data {
                        // Check if it's a printable ASCII character
                        if b >= 32 && b <= 126 {
                            current_string.push(b as char);
                        } else {
                            // If we hit a non-printable byte, save the string if it's long enough
                            if current_string.len() >= 4 {
                                found_strings.push(current_string.clone());
                            }
                            current_string.clear();
                        }
                    }
                    if current_string.len() >= 4 {
                        found_strings.push(current_string);
                    }

                    if !found_strings.is_empty() {
                        println!("  -> Decoded Text: {:?}", found_strings);
                        if let Some(tx) = &ui_tx {
                            let _ = tx.send(format!("Decoded Text: {:?}", found_strings)).await;
                        }
                    }
                }
            });

            // Single stdin dispatcher + write task
            let (user_tx, mut user_rx) = mpsc::channel::<String>(32);

            // Dispatcher: handles incoming Agent prompt requests and stdin lines,
            // routing stdin to either the prompt responder (if a prompt is pending)
            // or to the user input channel for the write task.
            let mut prompt_rx = prompt_rx; // take ownership
            let dispatcher = tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt;

                let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
                let mut pending_prompts: VecDeque<AgentRequest> = VecDeque::new();

                loop {
                    tokio::select! {
                        maybe_req = prompt_rx.recv() => {
                            match maybe_req {
                                Some(req) => {
                                    match &req {
                                        AgentRequest::Pin(prompt, _) | AgentRequest::Passkey(prompt, _) => {
                                            // Show the prompt so user knows why input is requested
                                            println!("{}", prompt);
                                        }
                                    }
                                    pending_prompts.push_back(req);
                                }
                                None => break,
                            }
                        }
                        line = lines.next_line() => {
                            match line {
                                Ok(Some(l)) => {
                                    if let Some(req) = pending_prompts.pop_front() {
                                        match req {
                                            AgentRequest::Pin(_, responder) | AgentRequest::Passkey(_, responder) => {
                                                let _ = responder.send(l);
                                            }
                                        }
                                    } else {
                                        if user_tx.send(l).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                                Ok(None) => break,
                                Err(_) => break,
                            }
                        }
                    }
                }
            });

            // Write task: consumes user-entered lines routed by the dispatcher
            let write_task = tokio::spawn(async move {
                while let Some(line) = user_rx.recv().await {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    let mut data_to_send = Vec::new();

                    if trimmed == "start" {
                        println!("Sending CMD_APP_START...");
                        data_to_send = vec![0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
                        data_to_send.extend_from_slice(b"RustTest");
                    } else if trimmed == "query" {
                        println!("Sending CMD_DEVICE_QUERY...");
                        data_to_send = vec![0x16, 0x03];
                    } else if trimmed.starts_with("/hex ") {
                        let hex_str = trimmed.strip_prefix("/hex ").unwrap().replace(" ", "");
                        for i in (0..hex_str.len()).step_by(2) {
                            if i + 2 <= hex_str.len() {
                                if let Ok(byte) = u8::from_str_radix(&hex_str[i..i + 2], 16) {
                                    data_to_send.push(byte);
                                }
                            }
                        }
                        println!("Sending raw binary: {:?}", data_to_send);
                    } else {
                        data_to_send = format!("{}\r\n", trimmed).into_bytes();
                    }

                    if let Err(e) = rx.write(&data_to_send).await {
                        println!("Failed to send: {}", e);
                    }
                }
            });

            // Run read + dispatcher + write tasks
            let _ = tokio::try_join!(read_task, dispatcher, write_task);
        } else {
            warn!("Could not find both RX and TX characteristics on this device.");
        }
    } else {
        info!("Target device was not found.");
    }

    Ok(())
}

// Helper: parse a hex string (possibly containing spaces) into bytes.
fn parse_hex_input(s: &str) -> Vec<u8> {
    let filtered: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let mut out = Vec::new();
    let mut chars = filtered.chars();
    while let Some(hi) = chars.next() {
        if let Some(lo) = chars.next() {
            let pair = format!("{}{}", hi, lo);
            if let Ok(b) = u8::from_str_radix(&pair, 16) {
                out.push(b);
            }
        } else {
            break;
        }
    }
    out
}

// Helper: extract printable ASCII substrings of at least `min_len` from bytes
fn extract_printable_ascii(data: &[u8], min_len: usize) -> Vec<String> {
    let mut current = String::new();
    let mut found = Vec::new();
    for &b in data {
        if b >= 32 && b <= 126 {
            current.push(b as char);
        } else {
            if current.len() >= min_len {
                found.push(current.clone());
            }
            current.clear();
        }
    }
    if current.len() >= min_len {
        found.push(current);
    }
    found
}

fn build_start_packet(app_name: &str) -> Vec<u8> {
    let mut data = vec![0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    data.extend_from_slice(app_name.as_bytes());
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_input() {
        assert_eq!(parse_hex_input("01 FF"), vec![0x01, 0xFF]);
        assert_eq!(parse_hex_input("0A0B0C"), vec![0x0A, 0x0B, 0x0C]);
        assert_eq!(parse_hex_input("A B C"), vec![0xAB]);
        assert_eq!(parse_hex_input(""), Vec::<u8>::new());
    }

    #[test]
    fn test_extract_printable_ascii() {
        let data = b"Hello\x00World!!!\x01abcde";
        let found = extract_printable_ascii(data, 3);
        assert!(found.contains(&"Hello".to_string()));
        assert!(found.contains(&"World!!!".to_string()));
        assert!(found.contains(&"abcde".to_string()));
    }

    #[test]
    fn test_build_start_packet() {
        let pkt = build_start_packet("RustTest");
        assert_eq!(pkt[0], 0x01);
        assert_eq!(&pkt[8..], b"RustTest");
    }
}
