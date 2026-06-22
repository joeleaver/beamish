// Beamish — Quick Share for Linux: a small Rinch GUI over rqs_lib.
// Receiving (incl. our BLE receiver advert) is handled by rqs_lib::run(); this
// app adds: accept/reject UI, sending to discovered devices, and an options panel.

use std::path::PathBuf;

use rinch::menu::{Menu, MenuItem};
use rinch::prelude::*;
use rinch::tray::TrayIconBuilder;
use rqs_lib::channel::{ChannelAction, ChannelDirection, ChannelMessage};
use rqs_lib::{EndpointInfo, OutboundPayload, SendInfo, State, Visibility, RQS};

/// Handle SIGINT/SIGTERM: stop RQS gracefully (this cancels the cancellation token,
/// so an in-flight WIFI_HOTSPOT SoftAP is torn down before we go) then terminate the
/// whole process. We must exit explicitly: tokio's signal handlers suppress the
/// default terminate action, and the rinch GUI runs on the main thread, so otherwise
/// the process would linger after the backend stopped. The stop is bounded so a
/// wedged teardown can't make beamish unkillable.
async fn shutdown_and_exit(rqs: &mut RQS, sig: &str) -> ! {
    eprintln!("beamish: {sig} — stopping RQS (tears down any SoftAP), then exiting");
    let _ = tokio::time::timeout(std::time::Duration::from_secs(6), rqs.stop()).await;
    std::process::exit(0);
}

#[derive(Clone, PartialEq)]
struct Incoming {
    id: String,
    name: String,
    files: String,
    pin: String,
}

#[derive(Clone, PartialEq)]
struct Device {
    id: String,
    name: String,
    // "ip:port" for an mDNS/WiFi endpoint; empty for a BLE-only one.
    addr: String,
    // Set when the device was found over BLE ("visible to everyone") rather than
    // mDNS. Sending to it goes over L2CAP, which isn't wired up yet (Phase 2).
    bt_address: Option<String>,
    psm: Option<u16>,
}

// Commands from the UI thread to the async backend.
enum Cmd {
    Accept(String),
    Reject(String),
    Send {
        id: String,
        name: String,
        addr: String,
        path: String,
    },
    SetDownload(String),
    SetAutoAccept(bool),
    StartDiscovery,
    StopDiscovery,
}

// Which view the bottom nav is showing. Receive is the default (the app's main
// job); an incoming transfer auto-switches here so the consent is never missed.
#[derive(Clone, Copy, PartialEq)]
enum View {
    Receive,
    Send,
    Options,
}

// Visual identity: "Beamish — ready to receive". Slate-indigo field, one electric
// cyan signal accent (ready/accept), amber for the live transfer PIN. The hero is
// the device's readiness — a breathing signal pulse + the computer's name — and
// the incoming-transfer consent is the centerpiece. Rinch renders via Stylo, so
// this is real CSS: keyframes, gradients, transitions, and reduced-motion.
const QS_CSS: &str = r#"
html, body { margin: 0; padding: 0; background: #0D1016; }

.qs-root {
  min-height: 100vh; box-sizing: border-box; padding: 22px 20px 16px;
  display: flex; flex-direction: column; gap: 18px;
  font-family: "Inter", "SF Pro Text", system-ui, sans-serif; color: #E7EDF5;
  background:
    radial-gradient(120% 60% at 50% 6%, rgba(52,225,208,0.10), rgba(52,225,208,0) 60%),
    #0D1016;
}

.qs-app { min-height: 100vh; box-sizing: border-box; display: flex; flex-direction: column;
  font-family: "Inter", "SF Pro Text", system-ui, sans-serif; color: #E7EDF5;
  background: radial-gradient(120% 52% at 50% 3%, rgba(52,225,208,0.10), rgba(52,225,208,0) 56%), #0D1016; }
.qs-view { flex: 1; display: flex; flex-direction: column; overflow: auto; }

.qs-receive { flex: 1; display: flex; flex-direction: column; align-items: stretch;
  justify-content: center; text-align: center; gap: 6px; padding: 20px 26px 30px; }
.qs-ready, .qs-consent { display: flex; flex-direction: column; align-items: stretch; gap: 6px; }

.qs-send { padding: 18px 20px 12px; display: flex; flex-direction: column; gap: 14px; }
.qs-options { padding: 18px 20px; display: flex; flex-direction: column; gap: 2px; }
.qs-view-title { font-size: 20px; font-weight: 700; color: #EAF1F8; }
.qs-view-sub { font-size: 13px; color: #7C879A; margin-top: 1px; }

.qs-opt-row { display: flex; align-items: center; justify-content: space-between; gap: 14px;
  padding: 15px 2px; border-bottom: 1px solid #181F2A; }
.qs-opt-label { font-size: 14px; color: #DCE4EF; font-weight: 500; }
.qs-opt-sub { font-size: 12px; color: #6B7689; margin-top: 2px; }

.qs-nav { display: flex; border-top: 1px solid #1A212C; background: rgba(12,15,21,0.92); }
.qs-tab { flex: 1; display: flex; background: none; border: none; cursor: pointer; padding: 0; }
.qs-tab-in { flex: 1; display: flex; flex-direction: column; align-items: center; gap: 3px;
  padding: 11px 0 10px; color: #66728A; font-size: 11px; font-weight: 600; letter-spacing: 0.3px; }
.qs-tab-on { color: #34E1D0; }
.qs-tab-icon { font-size: 17px; line-height: 1; }

.qs-topbar { display: flex; align-items: center; justify-content: space-between; padding: 16px 20px 6px; }
.qs-wordmark { display: flex; align-items: center; gap: 8px; font-size: 12px;
  font-weight: 600; letter-spacing: 2.5px; text-transform: uppercase; color: #6B7689; }
.qs-mark { color: #34E1D0; font-size: 14px; }

.qs-chip { display: flex; align-items: center; gap: 7px; padding: 5px 11px;
  border-radius: 999px; font-size: 12px; font-weight: 600; color: #34E1D0;
  background: rgba(52,225,208,0.08); border: 1px solid rgba(52,225,208,0.22); }
.qs-dot { width: 7px; height: 7px; border-radius: 50%; background: #34E1D0;
  box-shadow: 0 0 8px 1px rgba(52,225,208,0.80); }

.qs-stage { background: #141A24; border: 1px solid #222B3A; border-radius: 18px;
  padding: 30px 22px 26px; display: flex; flex-direction: column; align-items: stretch;
  text-align: center; gap: 6px; }

.qs-signal { position: relative; align-self: center; width: 144px; height: 144px;
  display: flex; align-items: center; justify-content: center; margin-bottom: 8px; }
.qs-ring { position: absolute; top: 50%; left: 50%; border-radius: 50%;
  border: 1.5px solid rgba(52,225,208,0.30); }
.qs-ring-1 { width: 82px;  height: 82px;  margin: -41px 0 0 -41px; border-color: rgba(52,225,208,0.42); }
.qs-ring-2 { width: 110px; height: 110px; margin: -55px 0 0 -55px; border-color: rgba(52,225,208,0.22); }
.qs-ring-3 { width: 140px; height: 140px; margin: -70px 0 0 -70px; border-color: rgba(52,225,208,0.10); }
.qs-node { position: relative; width: 60px; height: 60px; border-radius: 50%;
  display: flex; align-items: center; justify-content: center; font-size: 26px; color: #07140F;
  background: radial-gradient(circle at 35% 30%, #7CF5E6, #21C7B4);
  box-shadow: 0 0 22px 4px rgba(52,225,208,0.45); }

.qs-eyebrow { font-size: 11px; font-weight: 700; letter-spacing: 2.5px;
  text-transform: uppercase; color: #34E1D0; }
.qs-eyebrow-amber { color: #FFB454; }
.qs-ready-title { font-size: 19px; font-weight: 700; color: #EAF1F8; }
.qs-device { font-size: 13px; font-weight: 600; color: #34E1D0; letter-spacing: 0.2px; }
.qs-hint { font-size: 13px; color: #7C879A; }

.qs-sender { font-size: 20px; font-weight: 700; color: #F2F6FB; }
.qs-files { font-size: 13px; color: #8A95A8; }
.qs-pin-label { font-size: 12px; color: #7C879A; margin-top: 8px; }
.qs-send-pin { display: flex; flex-direction: column; align-items: center; gap: 2px; margin: 8px 0 4px; }
.qs-pin { font-family: "SF Mono", "JetBrains Mono", ui-monospace, monospace;
  font-size: 36px; font-weight: 700; letter-spacing: 12px; color: #FFB454;
  text-shadow: 0 0 18px rgba(255,180,84,0.35); padding-left: 12px; margin: 2px 0 6px; }

.qs-actions { display: flex; gap: 10px; width: 100%; margin-top: 10px; }
.qs-btn { border: none; border-radius: 12px; padding: 12px 16px; font-size: 14px;
  font-weight: 600; cursor: pointer;
  transition: transform 120ms ease, background-color 150ms ease; }
.qs-btn:active { transform: translateY(1px); }
.qs-btn-accept { flex: 1; color: #07140F; background: #2DD4C0;
  box-shadow: 0 6px 18px -6px rgba(45,212,192,0.70); }
.qs-btn-accept:hover { background: #46E5D2; }
.qs-btn-decline { flex: 1; color: #F2849A; background: transparent;
  border: 1px solid rgba(240,101,122,0.40); }
.qs-btn-decline:hover { background: rgba(240,101,122,0.10); }

.qs-panel { background: rgba(20,26,36,0.55); border: 1px solid #1E2532;
  border-radius: 14px; padding: 14px 16px; display: flex; flex-direction: column; gap: 10px; }
.qs-panel-head { font-size: 12px; font-weight: 700; letter-spacing: 1.5px;
  text-transform: uppercase; color: #6B7689; }
.qs-row { display: flex; align-items: center; gap: 10px; }
.qs-btn-ghost { background: #1B2230; color: #CDD6E3; border: 1px solid #2A3342;
  border-radius: 10px; padding: 8px 14px; font-size: 13px; font-weight: 600;
  cursor: pointer; transition: background-color 150ms ease; }
.qs-btn-ghost:hover { background: #222B3B; }
.qs-muted { font-size: 13px; color: #7C879A; }
.qs-device-chip { display: flex; align-items: center; gap: 9px; width: 100%;
  text-align: left; background: #161D29; color: #DCE4EF; border: 1px solid #242E3D;
  border-radius: 10px; padding: 10px 13px; font-size: 14px; font-weight: 500;
  cursor: pointer; transition: border-color 150ms ease, background-color 150ms ease; }
.qs-device-chip:hover { border-color: #34E1D0; background: #18222E; }
.qs-chip-glyph { color: #34E1D0; font-weight: 700; }
.qs-chip-badge { margin-left: auto; font-size: 11px; font-weight: 600;
  color: #F2B441; background: #2A2233; border: 1px solid #4A3A1E;
  border-radius: 6px; padding: 2px 7px; white-space: nowrap; }

.qs-footer { display: flex; align-items: center; justify-content: space-between;
  margin-top: auto; padding-top: 4px; font-size: 13px; color: #7C879A; }
.qs-toggle { display: flex; align-items: center; gap: 8px; }
.qs-link { background: none; border: none; color: #34E1D0; font-size: 13px;
  font-weight: 600; cursor: pointer; padding: 0; }
.qs-link:hover { color: #5CEFDD; }

@keyframes qs-pulse {
  0%   { transform: scale(0.7); opacity: 0.6; }
  80%  { transform: scale(2.0); opacity: 0; }
  100% { transform: scale(2.0); opacity: 0; }
}
@keyframes qs-breathe {
  0%, 100% { opacity: 1; transform: scale(1); }
  50%      { opacity: 0.4; transform: scale(0.8); }
}
@keyframes qs-rise {
  from { opacity: 0; transform: translateY(10px); }
  to   { opacity: 1; transform: translateY(0); }
}
.qs-rise { animation: qs-rise 320ms ease-out; }

@media (prefers-reduced-motion: reduce) {
  .qs-ring, .qs-dot, .qs-rise { animation: none; }
  .qs-ring { opacity: 0.4; }
}
"#;

#[component]
fn app() -> NodeHandle {
    let status = Signal::new("Starting…".to_string());
    let incoming = Signal::new(Option::<Incoming>::None);
    let devices = Signal::new(Vec::<Device>::new());
    // QS_SEND_FILE=/path preselects the file to send, bypassing the native file
    // dialog — for hands-free/automated send testing via the rinch debug MCP
    // (the GTK/portal dialog isn't part of the rinch DOM, so it can't be driven).
    let selected_file = Signal::new(std::env::var("QS_SEND_FILE").ok());
    // The app always advertises (mDNS + BLE) while it's open — a receiver that
    // isn't discoverable can't receive, so there's no user-facing toggle for it.
    // The hidden QS_START_INVISIBLE=1 dev env var starts with mDNS off to force
    // the phone onto the BLE->L2CAP path for testing the WiFi bandwidth upgrade.
    let start_visible = std::env::var_os("QS_START_INVISIBLE").is_none();
    let download_path = Signal::new(String::new());
    let auto_accept = Signal::new(false);
    // Active bottom-nav view; Receive by default.
    let view = Signal::new(View::Receive);
    // Outbound verification PIN + the id of the device we're sending to, so the
    // code can be shown inline while connecting / before the transfer is confirmed.
    let send_pin = Signal::new(Option::<String>::None);
    let sending_id = Signal::new(Option::<String>::None);

    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<Cmd>();
    // Wrap the sender in a (Copy) Signal so every UI closure can grab a clone
    // via cmd.get() without move/borrow gymnastics.
    let cmd = Signal::new(cmd_tx);

    // ---- backend: owns RQS on a tokio runtime, bridges events to signals ----
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => {
                status.send(format!("runtime error: {e}"));
                return;
            }
        };
        rt.block_on(async move {
            // Fixed receiver port (not :0). With a random port every launch, a
            // killed/restarted app leaves a stale mDNS SRV record pointing at the
            // old, dead port cached on avahi and the phone — so the service name
            // resolves to two ports and the phone may connect to the dead one.
            // A stable port makes every restart's mDNS record identical, so stale
            // caches are harmless.
            let init_visibility = if start_visible {
                Visibility::Visible
            } else {
                Visibility::Invisible
            };
            let mut rqs = RQS::new(init_visibility, Some(52382), None);
            let sender_file = match rqs.run().await {
                Ok((sf, _ble)) => sf,
                Err(e) => {
                    status.send(format!("failed to start: {e}"));
                    return;
                }
            };
            let msg_sender = rqs.message_sender.clone();
            let dch = tokio::sync::broadcast::channel::<EndpointInfo>(32).0;
            // Discovery (and its legacy 0xFE2C "I'm sending" beacon) is started
            // ON DEMAND when the user picks a file to send — running it while
            // idle makes phones treat us as a sender and breaks receiving.
            let mut discovery_active = false;
            // True while the user is on the Send tab, so a send can pause
            // discovery to free the radio and we know to resume it afterward.
            let mut on_send_tab = false;
            // Mirrors the UI "Auto-accept" toggle, kept in sync via Cmd so the
            // backend never has to read a UI Signal off-thread.
            let mut auto_accept_enabled = false;
            // True between Cmd::Send and the transfer's terminal state, so we only
            // surface the outbound PIN (the lib emits it at an intermediate state).
            let mut is_sending = false;

            let mut msg_rx = msg_sender.subscribe();
            let mut dch_rx = dch.subscribe();
            status.send("Ready — discoverable to nearby phones".to_string());

            // After a transfer ends, the "complete/rejected/cancelled" banner drops
            // back to "Ready" a few seconds later so the next file can come in —
            // unless a new transfer started meanwhile (guarded by a generation count
            // that every transfer-activity state bumps, cancelling a stale reset).
            let activity_gen = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
            let schedule_ready_reset = {
                let ag = activity_gen.clone();
                move || {
                    let g = ag.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    let ag2 = ag.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                        if ag2.load(std::sync::atomic::Ordering::Relaxed) == g {
                            status.send("Ready".to_string());
                        }
                    });
                }
            };

            // On a normal kill/logout/shutdown (SIGTERM) or Ctrl-C (SIGINT), stop
            // RQS gracefully: that cancels the cancellation token, so an in-flight
            // L2CAP connection breaks to its deterministic teardown and a live
            // WIFI_HOTSPOT SoftAP is torn down instead of outliving the process.
            // .ok() so a (near-impossible) registration failure just disables this
            // arm rather than killing the backend.
            let mut sigterm =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();

            loop {
                tokio::select! {
                    r = msg_rx.recv() => {
                        if let Ok(m) = r {
                            if m.direction != ChannelDirection::LibToFront { continue; }
                            let name = m.meta.as_ref()
                                .and_then(|md| md.source.as_ref())
                                .map(|s| s.name.clone())
                                .unwrap_or_else(|| "Unknown".to_string());
                            // The outbound PIN rides an intermediate state we don't
                            // otherwise match; grab it whenever it appears mid-send.
                            if is_sending {
                                if let Some(pin) = m.meta.as_ref().and_then(|md| md.pin_code.clone()) {
                                    send_pin.send(Some(pin));
                                }
                            }
                            match m.state {
                                Some(State::WaitingForUserConsent) => {
                                    activity_gen.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    view.send(View::Receive);
                                    if auto_accept_enabled {
                                        // Skip the consent card and accept immediately.
                                        let _ = msg_sender.send(ChannelMessage { id: m.id.clone(), direction: ChannelDirection::FrontToLib, action: Some(ChannelAction::AcceptTransfer), ..Default::default() });
                                        status.send(format!("Auto-accepting from {name}…"));
                                    } else {
                                        let files = m.meta.as_ref()
                                            .and_then(|md| md.files.clone())
                                            .map(|f| f.join(", "))
                                            .unwrap_or_default();
                                        let pin = m.meta.as_ref()
                                            .and_then(|md| md.pin_code.clone())
                                            .unwrap_or_default();
                                        incoming.send(Some(Incoming { id: m.id.clone(), name: name.clone(), files, pin }));
                                        status.send(format!("Incoming from {name}"));
                                    }
                                }
                                Some(State::ReceivingFiles) => { activity_gen.fetch_add(1, std::sync::atomic::Ordering::Relaxed); view.send(View::Receive); incoming.send(None); status.send(format!("Receiving from {name}…")); }
                                Some(State::SendingFiles) => { activity_gen.fetch_add(1, std::sync::atomic::Ordering::Relaxed); status.send(format!("Sending to {name}…")); }
                                Some(State::Finished) => { status.send("✓ Transfer complete".to_string()); incoming.send(None); is_sending = false; send_pin.send(None); sending_id.send(None); schedule_ready_reset(); }
                                Some(State::Rejected) => { status.send("Rejected".to_string()); incoming.send(None); is_sending = false; send_pin.send(None); sending_id.send(None); schedule_ready_reset(); }
                                Some(State::Cancelled) => { status.send("Cancelled".to_string()); incoming.send(None); is_sending = false; send_pin.send(None); sending_id.send(None); schedule_ready_reset(); }
                                Some(State::Disconnected) => { status.send("Ready".to_string()); incoming.send(None); is_sending = false; send_pin.send(None); sending_id.send(None); }
                                _ => {}
                            }
                            // A send pauses discovery to free the radio; resume it once the
                            // transfer ends, if the user is still on the Send tab.
                            if matches!(m.state, Some(State::Finished | State::Rejected | State::Cancelled | State::Disconnected))
                                && on_send_tab && !discovery_active {
                                let _ = rqs.discovery(dch.clone());
                                discovery_active = true;
                            }
                        }
                    }
                    r = dch_rx.recv() => {
                        if let Ok(ep) = r {
                            if ep.present == Some(false) {
                                let id = ep.id.clone();
                                devices.update_send(move |list| list.retain(|d| d.id != id));
                            } else {
                                // mDNS endpoints carry ip+port; BLE ("visible to
                                // everyone") ones carry a Bluetooth address + PSM
                                // and no ip. Accept either.
                                let name = ep.name.clone().unwrap_or_else(|| ep.id.clone());
                                let dev = if let (Some(ip), Some(port)) = (ep.ip.clone(), ep.port.clone()) {
                                    Some(Device { id: ep.id.clone(), name, addr: format!("{ip}:{port}"), bt_address: None, psm: None })
                                } else if let Some(bt) = ep.bt_address.clone() {
                                    Some(Device { id: ep.id.clone(), name, addr: String::new(), bt_address: Some(bt), psm: ep.psm })
                                } else {
                                    None
                                };
                                if let Some(dev) = dev {
                                    devices.update_send(move |list| {
                                        if let Some(d) = list.iter_mut().find(|d| d.id == dev.id) {
                                            *d = dev.clone();
                                        } else {
                                            list.push(dev.clone());
                                        }
                                    });
                                }
                            }
                        }
                    }
                    c = cmd_rx.recv() => {
                        match c {
                            None => break,
                            Some(Cmd::Accept(id)) => { let _ = msg_sender.send(ChannelMessage { id, direction: ChannelDirection::FrontToLib, action: Some(ChannelAction::AcceptTransfer), ..Default::default() }); }
                            Some(Cmd::Reject(id)) => { let _ = msg_sender.send(ChannelMessage { id, direction: ChannelDirection::FrontToLib, action: Some(ChannelAction::RejectTransfer), ..Default::default() }); }
                            Some(Cmd::Send { id, name, addr, path }) => {
                                is_sending = true;
                                // Pause discovery for the duration of the transfer: the BLE
                                // scan/advert + mDNS browse share the 2.4GHz radio and were
                                // stalling Wi-Fi sends. Resumed on the terminal state below.
                                if discovery_active {
                                    rqs.stop_discovery();
                                    discovery_active = false;
                                }
                                let _ = sender_file.send(SendInfo { id, name, addr, ob: OutboundPayload::Files(vec![path]) }).await;
                            }
                            Some(Cmd::SetDownload(p)) => rqs.set_download_path(Some(PathBuf::from(p))),
                            Some(Cmd::SetAutoAccept(v)) => auto_accept_enabled = v,
                            Some(Cmd::StartDiscovery) => {
                                on_send_tab = true;
                                if !discovery_active {
                                    let _ = rqs.discovery(dch.clone());
                                    discovery_active = true;
                                    status.send("Searching for nearby devices…".to_string());
                                }
                            }
                            // Leaving the Send tab stops discovery so we revert to a
                            // pure receiver — running it idle advertises us as a sender
                            // (0xFE2C) and stops phones from listing us to receive.
                            Some(Cmd::StopDiscovery) => {
                                on_send_tab = false;
                                if discovery_active {
                                    rqs.stop_discovery();
                                    discovery_active = false;
                                }
                            }
                        }
                    }
                    _ = tokio::signal::ctrl_c() => {
                        shutdown_and_exit(&mut rqs, "SIGINT").await;
                    }
                    Some(_) = async { sigterm.as_mut()?.recv().await } => {
                        shutdown_and_exit(&mut rqs, "SIGTERM").await;
                    }
                }
            }
        });
    });

    // When a transfer needs consent (the backend sets `incoming`), pop the window
    // up from the tray so the prompt is never missed. Window control is main-thread
    // only (thread-local event-loop proxy), so this must run as a UI-thread Effect
    // reacting to the signal — NOT from the backend thread (where it'd be a no-op).
    rinch::Effect::new(move || {
        if incoming.get().is_some() {
            show_current_window();
        }
    });

    // The hero identity — the computer's own name (same hostname rqs_lib advertises).
    let device_name = Signal::new(
        std::fs::read_to_string("/etc/hostname")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "This computer".to_string()),
    );

    rsx! {
        div { class: "qs-app",
            style { {QS_CSS} }

            // ── top bar: wordmark + live readiness ──
            div { class: "qs-topbar",
                div { class: "qs-wordmark",
                    span { class: "qs-mark", "◉" }
                    "Beamish"
                }
                div { class: "qs-chip",
                    span { class: "qs-dot" }
                    "Discoverable"
                }
            }

            // ── active view: Receive / Send / Options ──
            div { class: "qs-view",
                match view.get() {
                    View::Receive => div { class: "qs-receive",
                        if incoming.get().is_some() {
                            div { class: "qs-consent qs-rise",
                                div { class: "qs-eyebrow qs-eyebrow-amber", "Incoming transfer" }
                                div { class: "qs-sender", {|| incoming.get().map(|i| i.name).unwrap_or_default()} }
                                div { class: "qs-files", {|| incoming.get().map(|i| i.files).unwrap_or_default()} }
                                div { class: "qs-pin-label", "Confirm this code matches your phone" }
                                div { class: "qs-pin", {|| incoming.get().map(|i| i.pin).unwrap_or_default()} }
                                div { class: "qs-actions",
                                    button {
                                        class: "qs-btn qs-btn-accept",
                                        onclick: move || { if let Some(i) = incoming.get() { let _ = cmd.get().send(Cmd::Accept(i.id)); } },
                                        "Accept"
                                    }
                                    button {
                                        class: "qs-btn qs-btn-decline",
                                        onclick: move || { if let Some(i) = incoming.get() { let _ = cmd.get().send(Cmd::Reject(i.id)); } },
                                        "Decline"
                                    }
                                }
                            }
                        } else {
                            div { class: "qs-ready",
                                div { class: "qs-signal",
                                    span { class: "qs-ring qs-ring-3" }
                                    span { class: "qs-ring qs-ring-2" }
                                    span { class: "qs-ring qs-ring-1" }
                                    div { class: "qs-node", "↓" }
                                }
                                div { class: "qs-ready-title",
                                    {|| { let s = status.get();
                                        if s.starts_with("Receiving") { "Receiving files…".to_string() }
                                        else if s.starts_with("Sending") { "Sending…".to_string() }
                                        else if s.starts_with("✓") { "Transfer complete".to_string() }
                                        else { "Ready to receive".to_string() }
                                    }}
                                }
                                div { class: "qs-device", {|| device_name.get()} }
                                div { class: "qs-hint",
                                    {|| { let s = status.get();
                                        if s.is_empty() || s.starts_with("Ready") || s.starts_with("Starting") {
                                            "Files shared from your phone land here.".to_string()
                                        } else { s }
                                    }}
                                }
                            }
                        }
                    },
                    View::Send => div { class: "qs-send",
                        div {
                            div { class: "qs-view-title", "Send a file" }
                            div { class: "qs-view-sub", "Pick a file, then choose a nearby device." }
                        }
                        div { class: "qs-row",
                            button {
                                class: "qs-btn-ghost",
                                onclick: move || {
                                    // The portal file dialog (rfd -> ashpd -> zbus) must run inside a
                                    // Tokio runtime, off the UI thread so it doesn't block the Wayland
                                    // loop. Only Signal::send() is cross-thread safe (.get()/.set()
                                    // touch a thread-local store and panic off-thread). Discovery is
                                    // already running (started on Send-tab entry); this just sets the file.
                                    std::thread::spawn(move || {
                                        let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                                            .enable_all().build() else { return; };
                                        if let Some(f) = rt.block_on(rfd::AsyncFileDialog::new().pick_file()) {
                                            selected_file.send(Some(f.path().display().to_string()));
                                        }
                                    });
                                },
                                "Choose file…"
                            }
                            span { class: "qs-muted",
                                {|| selected_file.get()
                                    .map(|p| p.rsplit('/').next().unwrap_or(&p).to_string())
                                    .unwrap_or_else(|| "no file chosen".to_string())}
                            }
                        }
                        if sending_id.get().is_some() {
                            div { class: "qs-send-pin",
                                div { class: "qs-pin-label",
                                    {|| { let sid = sending_id.get().unwrap_or_default();
                                        let name = devices.get().into_iter().find(|d| d.id == sid).map(|d| d.name).unwrap_or_else(|| "device".to_string());
                                        if send_pin.get().is_some() { format!("Confirm this code matches {name}") } else { format!("Connecting to {name}…") } }}
                                }
                                div { class: "qs-pin", {|| send_pin.get().unwrap_or_default()} }
                            }
                        }
                        div { class: "qs-panel-head", "Nearby devices" }
                        for d in devices.get() {
                            button {
                                key: d.id.clone(),
                                class: "qs-device-chip",
                                onclick: {
                                    let d = d.clone();
                                    move || {
                                        // Sending requires a chosen file; otherwise this click is a no-op.
                                        if let Some(f) = selected_file.get() {
                                            if d.bt_address.is_some() {
                                                // Found over BLE; sending over L2CAP isn't wired up yet (Phase 2).
                                                status.set(format!("{} is visible over Bluetooth — direct send is coming soon. For now, open its Quick Share receive screen.", d.name));
                                            } else {
                                                sending_id.set(Some(d.id.clone()));
                                                send_pin.set(None);
                                                let _ = cmd.get().send(Cmd::Send { id: d.id.clone(), name: d.name.clone(), addr: d.addr.clone(), path: f });
                                                status.set(format!("Sending to {}…", d.name));
                                            }
                                        }
                                    }
                                },
                                span { class: "qs-chip-glyph", "→" }
                                {d.name.clone()}
                                if d.bt_address.is_some() {
                                    span { class: "qs-chip-badge", "visible to everyone" }
                                }
                            }
                        }
                        if devices.get().is_empty() {
                            div { class: "qs-muted", "Searching for nearby devices…" }
                        }
                    },
                    View::Options => div { class: "qs-options",
                        div { class: "qs-view-title", "Options" }
                        div { class: "qs-opt-row",
                            div {
                                div { class: "qs-opt-label", "Auto-accept" }
                                div { class: "qs-opt-sub", "Accept incoming files without confirming." }
                            }
                            Checkbox {
                                checked_fn: move || auto_accept.get(),
                                onchange: move || { let nv = !auto_accept.get(); auto_accept.set(nv); let _ = cmd.get().send(Cmd::SetAutoAccept(nv)); },
                            }
                        }
                        div { class: "qs-opt-row",
                            div {
                                div { class: "qs-opt-label", "Save files to" }
                                div { class: "qs-opt-sub", {|| { let p = download_path.get(); if p.is_empty() { "~/Downloads".to_string() } else { p } }} }
                            }
                            button {
                                class: "qs-link",
                                onclick: move || {
                                    // Off the UI thread, inside a Tokio runtime — see "Choose file…".
                                    let tx = cmd.get();
                                    std::thread::spawn(move || {
                                        let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                                            .enable_all().build() else { return; };
                                        if let Some(f) = rt.block_on(rfd::AsyncFileDialog::new().pick_folder()) {
                                            let s = f.path().display().to_string();
                                            download_path.send(s.clone()); // cross-thread update
                                            let _ = tx.send(Cmd::SetDownload(s));
                                        }
                                    });
                                },
                                "Change…"
                            }
                        }
                        div { class: "qs-opt-row",
                            div {
                                div { class: "qs-opt-label", "This device" }
                                div { class: "qs-opt-sub", {|| format!("{} · discoverable over Wi-Fi & Bluetooth", device_name.get())} }
                            }
                        }
                    },
                }
            }

            // ── bottom nav ──
            div { class: "qs-nav",
                button { class: "qs-tab", onclick: move || { view.set(View::Receive); let _ = cmd.get().send(Cmd::StopDiscovery); },
                    if view.get() == View::Receive {
                        div { class: "qs-tab-in qs-tab-on", span { class: "qs-tab-icon", "↓" } div { "Receive" } }
                    } else {
                        div { class: "qs-tab-in", span { class: "qs-tab-icon", "↓" } div { "Receive" } }
                    }
                }
                button { class: "qs-tab", onclick: move || { view.set(View::Send); let _ = cmd.get().send(Cmd::StartDiscovery); },
                    if view.get() == View::Send {
                        div { class: "qs-tab-in qs-tab-on", span { class: "qs-tab-icon", "↑" } div { "Send" } }
                    } else {
                        div { class: "qs-tab-in", span { class: "qs-tab-icon", "↑" } div { "Send" } }
                    }
                }
                button { class: "qs-tab", onclick: move || { view.set(View::Options); let _ = cmd.get().send(Cmd::StopDiscovery); },
                    if view.get() == View::Options {
                        div { class: "qs-tab-in qs-tab-on", span { class: "qs-tab-icon", "⚙" } div { "Options" } }
                    } else {
                        div { class: "qs-tab-in", span { class: "qs-tab-icon", "⚙" } div { "Options" } }
                    }
                }
            }
        }
    }
}

fn main() {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var(
            "RUST_LOG",
            "info,rqs_lib=debug,mdns_sd=error,polling=error,neli=error,bluez_async=error",
        );
    }
    tracing_subscriber::fmt::init();

    // System tray: the app keeps running in the background to stay discoverable.
    // Closing the window hides it to the tray (see on_close_requested) rather than
    // quitting; the tray menu restores it or quits for real. (KDE/SNI is native;
    // GNOME needs an AppIndicator extension.) `_tray` must outlive run().
    let tray_menu = Menu::new()
        .item(MenuItem::new("Show Beamish").on_click(show_current_window))
        .separator()
        .item(MenuItem::new("Quit").on_click(close_current_window));
    let _tray = TrayIconBuilder::new()
        .with_tooltip("Beamish — ready to receive")
        .with_icon_png(include_bytes!("../assets/icon.png"))
        .expect("failed to load tray icon")
        .with_menu(tray_menu)
        .build()
        .expect("failed to create tray icon");

    rinch::run_with_window_props(
        app,
        WindowProps {
            title: "Beamish".into(),
            width: 460,
            height: 640,
            // App icon (assets/icon.svg → .png). Shown in the title bar
            // and taskbar; on Wayland a .desktop file is auto-generated from app_id.
            icon: Some(include_bytes!("../assets/icon.png")),
            // Defaults to "beamish" for native builds; the Flatpak sets
            // BEAMISH_APP_ID so the Wayland app-id matches the flatpak id and
            // the window groups under our installed icon/.desktop.
            app_id: Some(
                std::env::var("BEAMISH_APP_ID").unwrap_or_else(|_| "beamish".into()),
            ),
            // Hide to tray on window-close instead of quitting (keeps receiving).
            on_close_requested: Some(std::sync::Arc::new(|| {
                hide_current_window();
                false
            })),
            ..Default::default()
        },
        Some(ThemeProviderProps {
            primary_color: Some("cyan".into()),
            default_radius: Some("md".into()),
            dark_mode: true,
            ..Default::default()
        }),
    );
}
