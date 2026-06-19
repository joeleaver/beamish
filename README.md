# Beamish

A small Linux desktop app for [Google Quick Share](https://www.android.com/better-together/quick-share/)
(formerly Nearby Share) — send and receive files with Android phones, right from
your PC. Built in Rust on the [Rinch](../rinch) GUI framework over a patched
[rquickshare](https://github.com/Martichou/rquickshare) core.

- **Receive** files from a nearby phone with an on-screen PIN-confirm consent prompt.
- **Send** files to discovered devices.
- Lives in the **system tray** and stays discoverable in the background.
- Transfers negotiate BLE → L2CAP → Wi-Fi for full-speed transfers.

## Build & run

Beamish uses path dependencies on sibling checkouts, so it expects this layout:

```
dev/
├── beamish/        # this repo
├── rinch/          # the Rinch GUI framework
└── rquickshare/    # rquickshare (core_lib provides rqs_lib)
```

Then:

```sh
cargo run --release
```

The release profile is recommended — debug builds are too slow for the
consent/PIN dialog. The built binary is `target/release/beamish`.

## Status

Working end-to-end for phone → PC transfers (BLE/L2CAP and the Wi-Fi upgrade).
See `src/main.rs` — the app is a single-file Rinch GUI.
