# Beamish

A small Linux desktop app for [Google Quick Share](https://www.android.com/better-together/quick-share/)
(formerly Nearby Share) — send and receive files with Android phones, right from
your PC. Built in Rust on the [Rinch](https://github.com/joeleaver/rinch) GUI framework over a patched
[rquickshare](https://github.com/Martichou/rquickshare) core.

- **Receive** files from a nearby phone with an on-screen PIN-confirm consent prompt.
- **Send** files to discovered devices.
- Lives in the **system tray** and stays discoverable in the background.
- Transfers negotiate BLE → L2CAP → Wi-Fi for full-speed transfers.

## Build & run

Dependencies are fetched from git — no sibling checkouts needed:

```sh
cargo run --release
```

The release profile is recommended — debug builds are too slow for the
consent/PIN dialog. The built binary is `target/release/beamish`.

Beamish pulls two git crates:

- [`rinch`](https://github.com/joeleaver/rinch) — the GUI framework. Currently
  tracks the branch for [PR #45](https://github.com/joeleaver/rinch/pull/45) (a
  background-thread repaint fix Beamish needs); repoint this dep to `main` once
  that PR merges.
- [`rqs_lib`](https://github.com/joeleaver/rquickshare/tree/beamish) — a fork of
  [rquickshare](https://github.com/Martichou/rquickshare) with the BLE/L2CAP
  receive path and the Wi-Fi-upgrade keepalive patch (`beamish` branch).

## Status

Working end-to-end for phone → PC transfers (BLE/L2CAP and the Wi-Fi upgrade).
See `src/main.rs` — the app is a single-file Rinch GUI.
