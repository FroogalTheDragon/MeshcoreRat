use bluer::agent::Agent;
use bluer::{AdapterEvent, Address, AddressType, Session};
use futures::StreamExt;
use std::error::Error;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Duration;
use std::collections::HashSet;
use tracing::{debug, info, warn};
use bluer::Device;

#[derive(Debug)]
pub enum AgentRequest {
    Pin(String, oneshot::Sender<String>),
    Passkey(String, oneshot::Sender<String>),
}

use tokio::sync::mpsc::{Sender, Receiver};

pub async fn run(
    session: Session,
    ui_tx: Option<Sender<String>>,
    mut cmd_rx: Option<Receiver<String>>,
    mut prompt_resp_rx: Option<Receiver<String>>,
) -> Result<(), Box<dyn Error>> {
    // 1. Set up the Bluetooth Agent to handle PIN requests
    let (prompt_tx, mut prompt_rx) = mpsc::channel::<AgentRequest>(8);

    let mut agent = Agent { request_default: true, ..Default::default() };

    // Define what happens when a device asks for a PIN - send a request to the dispatcher
    let pin_tx = prompt_tx.clone();
    agent.request_pin_code = Some(Box::new(move |req| {
        let pin_tx = pin_tx.clone();
        Box::pin(async move {
            let prompt = format!("Device {:?} is requesting a PIN.", req);
            let (resp_tx, resp_rx) = oneshot::channel();
            // If dispatcher is alive, send request and await response. Otherwise fallback to blocking read.
            if pin_tx.send(AgentRequest::Pin(prompt.clone(), resp_tx)).await.is_ok()
                && let Ok(pin) = resp_rx.await
            {
                return Ok(pin);
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
            if pass_tx.send(AgentRequest::Passkey(prompt.clone(), resp_tx)).await.is_ok()
                && let Ok(pass_str) = resp_rx.await
                && let Ok(pk) = pass_str.trim().parse::<u32>()
            {
                return Ok(pk);
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

    // If a prompt response channel was provided, spawn a task to forward agent prompts to UI
    if let Some(mut prx) = prompt_resp_rx.take() {
        let tx_clone = ui_tx.clone();
        tokio::spawn(async move {
            while let Some(req) = prompt_rx.recv().await {
                match req {
                    AgentRequest::Pin(prompt, responder) => {
                        if let Some(tx) = &tx_clone {
                            let _ = tx.send(format!("PROMPT:{}", prompt)).await;
                        }
                        if let Some(resp) = prx.recv().await {
                            let _ = responder.send(resp);
                        }
                    }
                    AgentRequest::Passkey(prompt, responder) => {
                        if let Some(tx) = &tx_clone {
                            let _ = tx.send(format!("PROMPT:{}", prompt)).await;
                        }
                        if let Some(resp) = prx.recv().await {
                            let _ = responder.send(resp);
                        }
                    }
                }
            }
        });
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
        let _ = tx.send(format!("STATUS:Scanning on adapter {}", adapter.name())).await;
    }

    // If a command channel was provided, spawn a task to handle commands (requires adapter)
    if let Some(mut rx) = cmd_rx.take() {
        let tx_clone = ui_tx.clone();
        let adapter_clone = adapter.clone();
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                if cmd.starts_with("connect ") {
                    let addr_str = cmd.trim_start_matches("connect ").trim();
                    if let Ok(addr) = addr_str.parse::<Address>() {
                        if let Some(tx) = &tx_clone {
                            let _ = tx.send(format!("UI requested connect to {}", addr)).await;
                        }
                        // attempt to connect
                        if let Ok(device) = adapter_clone.device(addr) {
                            let txc = tx_clone.clone();
                            tokio::spawn(async move {
                                if let Err(e) = attempt_pair_and_connect(device, txc.clone()).await
                                    && let Some(tx) = &txc
                                {
                                    let _ = tx.send(format!("CONN_FAIL:{}", e)).await;
                                    let _ = tx.send(format!("Connect failed: {}", e)).await;
                                }
                            });
                        } else if let Some(tx) = &tx_clone {
                            let _ = tx.send(format!("CONN_FAIL:Device {} not found on adapter", addr)).await;
                            let _ = tx.send(format!("Device {} not found on adapter", addr)).await;
                        }
                    } else if let Some(tx) = &tx_clone {
                        let _ = tx.send(format!("CONN_FAIL:Invalid address: {}", addr_str)).await;
                        let _ = tx.send(format!("Invalid address: {}", addr_str)).await;
                    }
                } else {
                    if let Some(tx) = &tx_clone {
                        let _ = tx.send(format!("Received UI command: {}", cmd)).await;
                    }
                }
            }
        });
    }

    // 3. Start discovering devices
    let mut discover_events = adapter.discover_devices().await?;
    let mut target_addr: Option<Address> = None;
    let mut seen_meshcore_devices: HashSet<(AddressType, String, Option<Vec<String>>)> = HashSet::new();

    while let Some(event) = discover_events.next().await {
        if let AdapterEvent::DeviceAdded(addr) = event {
            let device = adapter.device(addr)?;
            let name = device
                .name()
                .await?
                .unwrap_or_else(|| "Unknown".to_string());

            if !is_meshcore_name(&name) {
                continue;
            }

            let address_type = device.address_type().await.unwrap_or_default();
            let uuids = device
                .uuids()
                .await
                .ok()
                .flatten()
                .map(|set| {
                    let mut list = set.into_iter().map(|u| u.to_string()).collect::<Vec<String>>();
                    list.sort();
                    list
                });

            let fingerprint = (address_type, name.clone(), uuids.clone());
            if !seen_meshcore_devices.insert(fingerprint) {
                debug!("Ignoring duplicate MeshCore device {} ({})", name, addr);
                continue;
            }

            debug!("Discovered device {} ({}) type={:?} uuids={:?}", name, addr, address_type, uuids);
            if let Some(tx) = &ui_tx {
                let _ = tx.send(format!("DEVICE:{}|{}", addr, name)).await;
                let _ = tx.send(format!("LOG:Discovered device {} ({})", name, addr)).await;
            }

            if target_addr.is_none() {
                target_addr = Some(addr);
            }
        }
    }

    // Drop the discovery stream to ensure the adapter stops scanning
    drop(discover_events);

    // Keep the bluetooth task alive to respond to UI commands and prompts
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

fn is_meshcore_name(name: &str) -> bool {
    name.starts_with("MeshCore")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_meshcore_name() {
        assert!(is_meshcore_name("MeshCore-1234"));
        assert!(is_meshcore_name("MeshCore Node"));
        assert!(!is_meshcore_name("OtherDevice"));
        assert!(!is_meshcore_name("meshcore-lowercase"));
    }
}

async fn attempt_pair_and_connect(device: Device, ui_tx: Option<Sender<String>>) -> Result<(), Box<dyn Error + Send + Sync>> {
    let addr = device.address();
    if let Some(tx) = &ui_tx {
        let _ = tx.send(format!("Attempting pair/connect to {}", addr)).await;
    }

    if !device.is_paired().await? {
        let _ = device.set_trusted(true).await;
        let mut attempts = 1;
        loop {
            match device.pair().await {
                Ok(_) => {
                    if let Some(tx) = &ui_tx {
                        let _ = tx.send(format!("Paired {} after {} attempts", addr, attempts)).await;
                    }
                    break;
                }
                Err(e) => {
                    if let Some(tx) = &ui_tx {
                        let _ = tx.send(format!("Pairing attempt {} failed: {}", attempts, e)).await;
                    }
                    attempts += 1;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }

    device.connect().await?;
    if let Some(tx) = &ui_tx {
        let _ = tx.send(format!("CONN:Connected to {}", addr)).await;
        let _ = tx.send(format!("Connected to {}", addr)).await;
    }

    // Wait for GATT
    tokio::time::sleep(Duration::from_secs(2)).await;

    let mut tx_char = None;
    let mut rx_char = None;
    for service in device.services().await? {
        let uuid = service.uuid().await?;
        if uuid.to_string().to_uppercase() == "6E400001-B5A3-F393-E0A9-E50E24DCCA9E" {
            for charac in service.characteristics().await? {
                let cuuid = charac.uuid().await?;
                if cuuid.to_string().to_uppercase() == "6E400002-B5A3-F393-E0A9-E50E24DCCA9E" {
                    rx_char = Some(charac);
                } else if cuuid.to_string().to_uppercase() == "6E400003-B5A3-F393-E0A9-E50E24DCCA9E" {
                    tx_char = Some(charac);
                }
            }
        }
    }

    if let (Some(_rx), Some(tx)) = (rx_char, tx_char) {
        if let Some(uix) = &ui_tx {
            let _ = uix.send("Subscribing to TX notifications (connected)".to_string()).await;
        }
        let notify = tx.notify().await?;
        tokio::spawn(async move {
            tokio::pin!(notify);
            while let Some(data) = notify.next().await {
                if let Ok(text) = String::from_utf8(data.clone())
                    && text.chars().all(|c| !c.is_control() || c == '\n' || c == '\r')
                {
                    if let Some(uix) = &ui_tx {
                        let _ = uix.send(format!("RX text: {}", text)).await;
                    }
                    continue;
                }
                if let Some(uix) = &ui_tx {
                    let mut s = String::from("RX binary: ");
                    for b in &data { s.push_str(&format!("{:02X} ", b)); }
                    let _ = uix.send(s).await;
                }
            }
        });
    } else {
        if let Some(tx) = &ui_tx {
            let _ = tx.send("Could not find both RX and TX characteristics on this device.".to_string()).await;
        }
    }

    Ok(())
}
