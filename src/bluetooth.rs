//! Bluetooth management and connection flow.
//!
//! This module drives adapter discovery, refresh handling, pairing, and connection logic.

use bluer::{Address, AddressType, Session};
use std::error::Error;
use tokio::time::Duration;
use std::collections::HashSet;
use tracing::{debug, info, warn};
// helper modules: scanner, agent, connection

pub mod scanner;
pub mod agent;
pub mod connection;

use tokio::sync::mpsc::{Sender, Receiver};

pub async fn run(
    session: Session,
    ui_tx: Option<Sender<String>>,
    mut cmd_rx: Option<Receiver<String>>,
    mut prompt_resp_rx: Option<Receiver<String>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // 1. Register agent to handle PIN/passkey prompts
    agent::setup_agent(&session, ui_tx.clone(), prompt_resp_rx.take()).await?;

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

    // We'll handle UI commands inline below (so we can use scanner/connection modules)

    // 3. Start discovering devices using the shared scanner helper
    let mut target_addr: Option<Address> = None;
    let mut seen_meshcore_devices: HashSet<(AddressType, String, Option<Vec<String>>)> = HashSet::new();

    let devices = scanner::scan_meshcore_devices(&adapter, Duration::from_secs(5)).await?;
    for (addr, name) in devices {
        let device = adapter.device(addr)?;
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

    // Command loop: respond to refresh/connect requests from UI
    let mut cmd_rx = cmd_rx.take().expect("command channel required");
    while let Some(cmd) = cmd_rx.recv().await {
        if cmd == "refresh" {
            // run a short scan and forward only newly discovered MeshCore devices
            let devices = scanner::scan_meshcore_devices(&adapter, Duration::from_secs(5)).await?;
            for (addr, name) in devices {
                let device = adapter.device(addr)?;
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
                    debug!("Skipping duplicate refresh MeshCore device {} ({})", name, addr);
                    continue;
                }

                if let Some(tx) = &ui_tx {
                    let _ = tx.send(format!("DEVICE:{}|{}", addr, name)).await;
                }
            }
        } else if cmd.starts_with("connect ") {
            let addr_str = cmd.trim_start_matches("connect ").trim();
            if let Ok(addr) = addr_str.parse::<Address>() {
                if let Some(tx) = &ui_tx {
                    let _ = tx.send(format!("UI requested connect to {}", addr)).await;
                }
                if let Ok(device) = adapter.device(addr) {
                    let txc = ui_tx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = connection::attempt_pair_and_connect(device, txc.clone()).await
                            && let Some(tx) = &txc
                        {
                            let _ = tx.send(format!("CONN_FAIL:{}", e)).await;
                            let _ = tx.send(format!("Connect failed: {}", e)).await;
                        }
                    });
                } else if let Some(tx) = &ui_tx {
                    let _ = tx.send(format!("CONN_FAIL:Device {} not found on adapter", addr)).await;
                    let _ = tx.send(format!("Device {} not found on adapter", addr)).await;
                }
            } else if let Some(tx) = &ui_tx {
                let _ = tx.send(format!("CONN_FAIL:Invalid address: {}", addr_str)).await;
                let _ = tx.send(format!("Invalid address: {}", addr_str)).await;
            }
        } else if cmd.starts_with("disconnect ") {
            let addr_str = cmd.trim_start_matches("disconnect ").trim();
            if let Ok(addr) = addr_str.parse::<Address>() {
                if let Some(tx) = &ui_tx {
                    let _ = tx.send(format!("UI requested disconnect from {}", addr)).await;
                }
                if let Ok(device) = adapter.device(addr) {
                    let txc = ui_tx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = connection::disconnect(device, txc.clone()).await
                            && let Some(tx) = &txc
                        {
                            let _ = tx.send(format!("DISCONN_FAIL:{}", e)).await;
                            let _ = tx.send(format!("Disconnect failed: {}", e)).await;
                        }
                    });
                } else if let Some(tx) = &ui_tx {
                    let _ = tx.send(format!("DISCONN_FAIL:Device {} not found on adapter", addr)).await;
                    let _ = tx.send(format!("Device {} not found on adapter", addr)).await;
                }
            } else if let Some(tx) = &ui_tx {
                let _ = tx.send(format!("DISCONN_FAIL:Invalid address: {}", addr_str)).await;
                let _ = tx.send(format!("Invalid address: {}", addr_str)).await;
            }
        } else {
            if let Some(tx) = &ui_tx {
                let _ = tx.send(format!("Received UI command: {}", cmd)).await;
            }
        }
    }

    Ok(())
}
