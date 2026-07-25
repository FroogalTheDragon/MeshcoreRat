mod bluetooth;
mod ui;

use bluer::Session;
use std::error::Error;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // initialize tracing subscriber for logging
    tracing_subscriber::fmt::init();

    let session = Session::new().await?;
    info!("Created bluer Session");

    // Channel for bluetooth -> UI events
    let (ui_tx, ui_rx) = tokio::sync::mpsc::channel::<String>(256);

    // Spawn UI task
    let ui_handle = tokio::spawn(async move {
        if let Err(e) = ui::run(ui_rx).await {
            eprintln!("UI error: {}", e);
        }
    });

    // Spawn bluetooth task
    let bt_handle = tokio::spawn(async move {
        match bluetooth::run(session, Some(ui_tx)).await {
            Ok(_) => info!("Bluetooth flow completed"),
            Err(e) => warn!("Bluetooth flow error: {}", e),
        }
    });

    let _ = tokio::try_join!(ui_handle, bt_handle);

    Ok(())
}
