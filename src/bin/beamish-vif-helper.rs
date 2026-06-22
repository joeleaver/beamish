//! `beamish-vif-helper` — the one privileged operation the WIFI_HOTSPOT bandwidth
//! upgrade needs: create / delete the `__ap` virtual interface used for the SoftAP.
//!
//! Everything else (the NM AP, shared IPv4/NAT/DHCP) is driven unprivileged over
//! NetworkManager by an active session, so this helper is deliberately tiny: it
//! ONLY adds or deletes an `apN` vif on a Wi-Fi STA. It is meant to be invoked via
//! `pkexec` (polkit action `org.beamish.vif-helper`, `allow_active=yes` → no
//! password prompt for an active local session) so the app never needs sudo.
//!
//! Because it runs as root, it validates its arguments strictly and execs `iw`/`ip`
//! directly (never a shell): the interface to create/delete must match `apN`
//! (1–2 digits), so the helper can never be coerced into touching the real STA or
//! any other interface. Usage:
//!   beamish-vif-helper add <sta> <ap>   # create apN on the STA + bring it up
//!   beamish-vif-helper del <ap>         # delete apN

use std::process::{exit, Command};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("add") => {
            let sta = arg(&args, 2, "add <sta> <ap>");
            let ap = arg(&args, 3, "add <sta> <ap>");
            if !valid_sta(sta) {
                die(&format!("refusing: '{sta}' is not a valid STA interface name"));
            }
            if !valid_ap(ap) {
                die(&format!("refusing: '{ap}' is not an apN interface name"));
            }
            // Clear any stale vif of the same name first (best-effort).
            let _ = exec("iw", &["dev", ap, "del"]);
            if !exec("iw", &["dev", sta, "interface", "add", ap, "type", "__ap"]) {
                die("iw interface add failed");
            }
            if !exec("ip", &["link", "set", ap, "up"]) {
                // Roll back the half-created vif so we don't leak it.
                let _ = exec("iw", &["dev", ap, "del"]);
                die("ip link set up failed");
            }
        }
        Some("del") => {
            let ap = arg(&args, 2, "del <ap>");
            if !valid_ap(ap) {
                die(&format!("refusing: '{ap}' is not an apN interface name"));
            }
            if !exec("iw", &["dev", ap, "del"]) {
                die("iw dev del failed");
            }
        }
        _ => die("usage: beamish-vif-helper add <sta> <ap> | del <ap>"),
    }
}

/// Fetch positional arg `i` or exit with a usage error.
fn arg<'a>(args: &'a [String], i: usize, usage: &str) -> &'a str {
    match args.get(i) {
        Some(a) => a.as_str(),
        None => die(&format!("usage: beamish-vif-helper {usage}")),
    }
}

/// An `apN` vif name we are allowed to manage: exactly `ap` + 1–2 digits.
fn valid_ap(name: &str) -> bool {
    match name.strip_prefix("ap") {
        Some(rest) => !rest.is_empty() && rest.len() <= 2 && rest.bytes().all(|b| b.is_ascii_digit()),
        None => false,
    }
}

/// A plausible Wi-Fi STA netdev name to add the vif onto: alphanumeric, bounded
/// length, and NOT itself an `apN` name (so `add` can't target a vif).
fn valid_sta(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 15
        && name.bytes().all(|b| b.is_ascii_alphanumeric())
        && !valid_ap(name)
}

/// Run `cmd args…` (no shell), returning whether it exited 0.
fn exec(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd)
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn die(msg: &str) -> ! {
    eprintln!("beamish-vif-helper: {msg}");
    exit(1);
}
