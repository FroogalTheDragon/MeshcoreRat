# meshy

meshy is a command-line / terminal UI (TUI) client for MeshCore companion radios.
It connects to devices over Bluetooth Low Energy (BLE) using BlueZ (via the
`bluer` crate) and provides a Ratatui-based interface for discovery, pairing,
GATT exploration and sending/receiving packets over the Nordic UART Service
(NUS) used by MeshCore devices.

This workspace is an early-stage implementation focused on feature parity with
the MeshCore Android companion app: discovery, pairing (with agent prompts),
binary packet send/receive (NUS), and an extendable TUI.

## Requirements

- Linux with BlueZ and D-Bus available (`bluetooth` systemd service)
- Rust toolchain (stable) with `cargo`
- A Bluetooth adapter supported by BlueZ

Optional runtime environment variables
- `RUST_LOG` — set to `debug` to see more verbose tracing output (e.g. `RUST_LOG=debug`)

## Build

Install Rust (rustup) and then:

```bash
cargo build --release
```

## Run

Run the TUI client from a terminal that has access to your Bluetooth adapter.

```bash
RUST_LOG=info cargo run --release
```

Notes:
- The program registers a Bluetooth Agent with BlueZ to handle PIN/passkey
	requests during pairing. When a pairing request appears the app will print a
	prompt in the terminal (and the TUI will display a notice). Reply by typing
	the PIN/passkey and pressing Enter.
- If BlueZ or the adapter are not ready the app will retry until an adapter is
	found and will report status in the TUI and logs.

## Usage (TUI)

- The TUI currently displays two panes: a status/controls pane and a scrolling
  log of events. Keybindings:
  - `Up` / `Down`: move selection between discovered devices
  - `Enter`: select the highlighted MeshCore device and prompt for PIN/passkey
  - `q`: quit the UI

The TUI is a work-in-progress. CLI-style commands (for sending NUS packets)
still route through the stdin dispatcher; future iterations will add input
widgets to the TUI for composing and sending binary packets.

## Testing

A small unit test covers MeshCore name detection and ensures device discovery
filtering behaves as expected. Run:

```bash
cargo test
```

## Development notes

- Bluetooth logic lives in `src/bluetooth.rs`.
- The TUI lives in `src/ui.rs` using `ratatui` + `crossterm`.
- Logging uses `tracing` / `tracing-subscriber`; control verbosity with `RUST_LOG`.
- Important files:
	- `src/main.rs` — program bootstrap and task wiring
	- `src/bluetooth.rs` — adapter/agent/discovery/pairing/GATT logic
	- `src/ui.rs` — minimal ratatui UI and event loop

## Next work items (suggested)

- Add TUI-driven pairing prompt UI (avoid stdin blocking fallback)
- Add device list and selection UI panel
- Add packet builder UI for common MeshCore commands
- Add graceful shutdown (unregister agent, disconnect device) on quit

## Contributing

Contributions welcome — open issues or pull requests for features, bugfixes,
and improvements. No license file is included in this repository; add one if
you intend to publish.
