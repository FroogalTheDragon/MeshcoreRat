use bluer::{Adapter, AdapterEvent, Address, AddressType};
use futures::StreamExt;
use std::collections::HashSet;
use std::error::Error;
use tokio::time::Duration;
use tracing::debug;

/// Scan for MeshCore devices for the given duration and return `(Address, name)` pairs.
/// Keeps deduplication simple using address type, name and sorted UUID list.
pub async fn scan_meshcore_devices(
    adapter: &Adapter,
    scan_duration: Duration,
) -> Result<Vec<(Address, String)>, Box<dyn Error + Send + Sync>> {
    let mut results = Vec::new();
    let mut seen: HashSet<(AddressType, String, Option<Vec<String>>)> = HashSet::new();

    let mut discover_events = adapter.discover_devices().await?;
    let timeout = tokio::time::sleep(scan_duration);
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            _ = &mut timeout => break,
            maybe_event = discover_events.next() => {
                match maybe_event {
                    Some(AdapterEvent::DeviceAdded(addr)) => {
                        if let Ok(device) = adapter.device(addr) {
                            let name = device.name().await?.unwrap_or_else(|| "Unknown".to_string());
                            if !name.starts_with("MeshCore") {
                                debug!("Skipping non-meshcore device {}", name);
                                continue;
                            }

                            let address_type = device.address_type().await.unwrap_or_default();
                            let uuids = device.uuids().await.ok().flatten().map(|set|{
                                let mut list = set.into_iter().map(|u| u.to_string()).collect::<Vec<String>>();
                                list.sort();
                                list
                            });

                            let fingerprint = (address_type, name.clone(), uuids.clone());
                            if !seen.insert(fingerprint) {
                                continue;
                            }

                            results.push((addr, name));
                        }
                    }
                    Some(_) => {}
                    None => break,
                }
            }
        }
    }

    // ensure discovery is dropped
    drop(discover_events);
    Ok(results)
}
