//! Device discovery, properties and mode transitions.

use crate::consts;
use crate::proc;
use crate::report::Reporter;
use crate::{DeviceSummary, Result};
use regex::Regex;
use std::collections::HashMap;
use std::process::Command;
use std::time::{Duration, Instant};

fn quiet(prog: &str, args: &[&str]) -> Option<std::process::Output> {
    Command::new(prog).args(args).output().ok()
}

pub fn adb_state() -> String {
    match quiet("adb", &["get-state"]) {
        Some(out) => {
            let mut s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.is_empty() {
                s = String::from_utf8_lossy(&out.stderr).trim().to_string();
            }
            s
        }
        None => "no adb".to_string(),
    }
}

pub fn adb_present() -> bool {
    adb_state() == "device"
}

pub fn fastboot_serial() -> Option<String> {
    let out = quiet("fastboot", &["devices"])?;
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if let (Some(serial), Some(_)) = (parts.next(), parts.next()) {
            return Some(serial.to_string());
        }
    }
    None
}

pub fn wait_for_adb(rep: &dyn Reporter, timeout_secs: u64, require_boot: bool) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let mut announced = false;
    while Instant::now() < deadline {
        let state = adb_state();
        if state == "device" {
            if !require_boot {
                return Ok(());
            }
            let boot = quiet("adb", &["shell", "getprop", "sys.boot_completed"])
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            if boot == "1" {
                return Ok(());
            }
        } else if state.contains("unauthorized") && !announced {
            rep.log("waiting for the tablet: tap Allow on the USB debugging prompt");
            announced = true;
        }
        std::thread::sleep(Duration::from_secs(3));
    }
    anyhow::bail!(
        "timed out after {timeout_secs}s waiting for adb (state: {})",
        adb_state()
    )
}

pub fn wait_for_fastboot(_rep: &dyn Reporter, timeout_secs: u64) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    while Instant::now() < deadline {
        if fastboot_serial().is_some() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    anyhow::bail!("timed out after {timeout_secs}s waiting for fastboot")
}

pub fn getprops(rep: &dyn Reporter) -> Result<HashMap<String, String>> {
    let text = proc::adb(rep, &["shell", "getprop"])?;
    let re = Regex::new(r"^\[(.+?)\]: \[(.*)\]$")?;
    let mut map = HashMap::new();
    for line in text.lines() {
        if let Some(caps) = re.captures(line.trim()) {
            map.insert(caps[1].to_string(), caps[2].to_string());
        }
    }
    Ok(map)
}

pub fn getprop(rep: &dyn Reporter, name: &str) -> Result<String> {
    Ok(proc::adb(rep, &["shell", "getprop", name])?
        .trim()
        .to_string())
}

pub fn summary(rep: &dyn Reporter) -> Result<DeviceSummary> {
    let p = getprops(rep)?;
    let get = |k: &str| p.get(k).cloned().unwrap_or_default();
    Ok(DeviceSummary {
        model: get("ro.product.device"),
        board: get("ro.product.board"),
        fingerprint: get("ro.build.fingerprint"),
        incremental: get("ro.build.version.incremental"),
        patch: get("ro.build.version.security_patch"),
        slot: get("ro.boot.slot_suffix"),
        locked: get("ro.boot.flash.locked"),
        verified_boot: get("ro.boot.verifiedbootstate"),
        lineage: get("ro.lineage.version"),
        bootloader: String::new(),
        supported_lk: false,
    })
}

pub fn require_model(rep: &dyn Reporter) -> Result<DeviceSummary> {
    wait_for_adb(rep, 300, true)?;
    let s = summary(rep)?;
    if !s.is_t405a() {
        anyhow::bail!(
            "this is not an onn 11\" Core (T405A): device={:?} board={:?}",
            s.model,
            s.board
        );
    }
    Ok(s)
}

pub fn fastboot_getvar(rep: &dyn Reporter, name: &str) -> Result<String> {
    let out = proc::run(rep, "fastboot", &["getvar", name])?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let re = Regex::new(&format!(
        r"(?m)^(?:\(bootloader\)\s*)?{}:\s*(.*)$",
        regex::escape(name)
    ))?;
    for line in text.lines() {
        if let Some(caps) = re.captures(line.trim()) {
            return Ok(caps[1].trim().to_string());
        }
    }
    Ok(String::new())
}

pub fn fastboot_getvar_all(rep: &dyn Reporter) -> Result<String> {
    let out = proc::run(rep, "fastboot", &["getvar", "all"])?;
    Ok(format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    ))
}

pub fn ensure_fastboot(rep: &dyn Reporter) -> Result<()> {
    if fastboot_serial().is_some() {
        return Ok(());
    }
    if adb_present() {
        proc::adb_ok(rep, &["reboot", "bootloader"])?;
    }
    wait_for_fastboot(rep, 120)
}

pub fn ensure_adb(rep: &dyn Reporter) -> Result<()> {
    if adb_present() {
        return Ok(());
    }
    if fastboot_serial().is_some() {
        proc::fastboot_ok(rep, &["reboot"])?;
    }
    wait_for_adb(rep, 300, true)
}

pub fn supported_lk(lk: &str) -> bool {
    consts::SUPPORTED_LK.iter().any(|(v, _)| *v == lk)
}
