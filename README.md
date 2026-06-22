# Beamish

A small Linux desktop app for [Google Quick Share](https://www.android.com/better-together/quick-share/)
(formerly Nearby Share) — send and receive files with Android phones, right from
your PC. Built in Rust on the [Rinch](https://github.com/joeleaver/rinch) GUI framework over a patched
[rquickshare](https://github.com/Martichou/rquickshare) core.

- **Receive** files from a nearby phone with an on-screen PIN-confirm consent prompt.
- **Send** files to discovered devices.
- Lives in the **system tray** and stays discoverable in the background.
- Transfers negotiate BLE → L2CAP → Wi-Fi for full-speed transfers, and can
  upgrade to a **direct Wi-Fi hotspot** — the phone joins a short-lived access
  point Beamish brings up co-channel — to sidestep the phone's flaky
  infrastructure Wi-Fi, falling back to LAN Wi-Fi when that isn't available.

## Install

Grab a package for your distro from the
[Releases](https://github.com/joeleaver/beamish/releases) page:

**Debian / Ubuntu**

```sh
sudo apt install ./beamish_*_amd64.deb
```

**Fedora / RHEL**

```sh
sudo dnf install ./beamish-*.x86_64.rpm
```

**AppImage** (any distro)

```sh
chmod +x Beamish-x86_64.AppImage
./Beamish-x86_64.AppImage
```

Beamish talks to BlueZ for Bluetooth, so make sure it's running
(`systemctl status bluetooth`). The `.deb`/`.rpm` declare `bluez` as a dependency.

The direct Wi-Fi-hotspot upgrade additionally uses **NetworkManager**, **iw**, and
**pkexec/polkit** (the `.deb`/`.rpm` recommend these). A tiny privileged helper —
`beamish-vif-helper`, authorized by a bundled polkit policy with no password prompt
for the active local session — creates the hotspot interface; everything else runs
unprivileged. Without them, transfers simply stay on LAN Wi-Fi. The AppImage can't
install the helper/policy, so it always uses LAN Wi-Fi.

**Start at login** (optional) — copy the bundled autostart entry so Beamish boots
into the tray ready to receive:

```sh
cp /usr/share/doc/beamish/beamish-autostart.desktop ~/.config/autostart/
```

## Build from source

Dependencies are fetched from git — no sibling checkouts needed:

```sh
cargo run --release
```

Use `--release`; debug builds are too slow for the consent/PIN dialog. The built
binary is `target/release/beamish`. Beamish pulls two git crates:

- [`rinch`](https://github.com/joeleaver/rinch) — the GUI framework (tracks `main`).
- [`rqs_lib`](https://github.com/joeleaver/rquickshare/tree/beamish) — a fork of
  [rquickshare](https://github.com/Martichou/rquickshare) with the BLE/L2CAP
  receive path and the Wi-Fi-upgrade keepalive patch (`beamish` branch).

Build-time system libraries (Debian/Ubuntu names): `pkg-config libdbus-1-dev
libudev-dev libssl-dev libxkbcommon-dev libwayland-dev libgtk-3-dev
libayatana-appindicator3-dev` plus the usual X11 `-dev` libs (see the CI
workflow for the exact list).

For development with the Rinch debug server (localhost DOM/screenshot capture),
build with the opt-in feature — it is **off** in shipped builds:

```sh
cargo run --release --features dev-debug
```

## Releasing

Native packages are wired up with `cargo-deb`, `cargo-generate-rpm`, and AppImage
in [`.github/workflows/release.yml`](.github/workflows/release.yml). Push a version
tag to build all three and attach them to a GitHub Release:

```sh
git tag v0.1.0
git push origin v0.1.0
```

## Status

Working end-to-end for phone → PC transfers (BLE/L2CAP and the Wi-Fi upgrade).
See `src/main.rs` — the app is a single-file Rinch GUI.
