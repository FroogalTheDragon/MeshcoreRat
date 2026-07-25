use bluer::Device;
use futures::StreamExt;
use std::error::Error;
use tokio::sync::mpsc::Sender;
use tokio::time::Duration;

/// Attempt to pair and connect to a device, sending status updates to `ui_tx` if provided.
pub async fn attempt_pair_and_connect(
    device: Device,
    ui_tx: Option<Sender<String>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
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

/// Disconnect the given `device`, sending status updates to `ui_tx` if provided.
pub async fn disconnect(
    device: Device,
    ui_tx: Option<Sender<String>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let addr = device.address();
    if let Some(tx) = &ui_tx {
        let _ = tx.send(format!("Attempting disconnect from {}", addr)).await;
    }

    if let Err(e) = device.disconnect().await {
        if let Some(tx) = &ui_tx {
            let _ = tx.send(format!("DISCONN_FAIL:{}", e)).await;
            let _ = tx.send(format!("Disconnect failed: {}", e)).await;
        }
        return Err(Box::new(e));
    }

    if let Some(tx) = &ui_tx {
        let _ = tx.send(format!("DISCONN:Disconnected from {}", addr)).await;
    }

    Ok(())
}
