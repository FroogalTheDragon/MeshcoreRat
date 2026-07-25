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
    // Channel for UI -> bluetooth commands
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel::<String>(32);
    // Channel for UI -> bluetooth prompt responses
    let (prompt_resp_tx, prompt_resp_rx) = tokio::sync::mpsc::channel::<String>(8);

    // Spawn UI task
    let ui_handle = tokio::spawn(async move {
        if let Err(e) = ui::run(ui_rx, cmd_tx, prompt_resp_tx).await {
            eprintln!("UI error: {}", e);
        }
    });

    // Spawn bluetooth task
    let bt_handle = tokio::spawn(async move {
        match bluetooth::run(session, Some(ui_tx), Some(cmd_rx), Some(prompt_resp_rx)).await {
            Ok(_) => info!("Bluetooth flow completed"),
            Err(e) => warn!("Bluetooth flow error: {}", e),
        }
    });

    let _ = tokio::try_join!(ui_handle, bt_handle);

    Ok(())
}
