use bluer::agent::Agent;
use bluer::Session;
use std::error::Error;
use tokio::sync::{mpsc, oneshot};
use tokio::sync::mpsc::{Receiver, Sender};

#[derive(Debug)]
pub enum AgentRequest {
    Pin(String, oneshot::Sender<String>),
    Passkey(String, oneshot::Sender<String>),
}

/// Register a bluer Agent and forward prompts to the UI prompt responder channel.
pub async fn setup_agent(
    session: &Session,
    ui_tx: Option<Sender<String>>,
    mut prompt_resp_rx: Option<Receiver<String>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (prompt_tx, mut prompt_rx) = mpsc::channel::<AgentRequest>(8);

    let mut agent = Agent { request_default: true, ..Default::default() };

    let pin_tx = prompt_tx.clone();
    agent.request_pin_code = Some(Box::new(move |req| {
        let pin_tx = pin_tx.clone();
        Box::pin(async move {
            let prompt = format!("Device {:?} is requesting a PIN.", req);
            let (resp_tx, resp_rx) = oneshot::channel();
            if pin_tx.send(AgentRequest::Pin(prompt.clone(), resp_tx)).await.is_ok()
                && let Ok(pin) = resp_rx.await
            {
                return Ok(pin);
            }

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

    let _agent_handle = session.register_agent(agent).await?;

    if let Some(tx) = &ui_tx {
        let _ = tx.send("Bluetooth Agent registered.".to_string()).await;
    }

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

    Ok(())
}
