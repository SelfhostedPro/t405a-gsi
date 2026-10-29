//! Raw-USB LK fastboot probe and unlock payload.

use crate::consts::{PAYLOAD, SUPPORTED_LK};
use crate::report::Reporter;
use crate::{device, Result, Settings};
use rusb::{Context, DeviceHandle, Direction, TransferType, UsbContext};
use std::fs;
use std::time::Duration;

const VID: u16 = 0x0e8d;
const PID: u16 = 0x201c;

fn find_lk_fastboot() -> Result<rusb::Device<Context>> {
    let ctx = Context::new().map_err(|e| anyhow::anyhow!("libusb init failed: {e}"))?;
    let devices = ctx
        .devices()
        .map_err(|e| anyhow::anyhow!("libusb list failed: {e}"))?;
    for device in devices.iter() {
        let desc = device
            .device_descriptor()
            .map_err(|e| anyhow::anyhow!("usb descriptor failed: {e}"))?;
        if desc.vendor_id() == VID && desc.product_id() == PID {
            return Ok(device);
        }
    }
    anyhow::bail!("no 0e8d:201c LK fastboot device found")
}

fn bulk_endpoints(device: &rusb::Device<Context>) -> Result<(u8, u8, u8)> {
    let config = device
        .active_config_descriptor()
        .map_err(|e| anyhow::anyhow!("usb config failed: {e}"))?;
    let mut found = Vec::new();
    for iface in config.interfaces() {
        for desc in iface.descriptors() {
            let mut ep_in = None;
            let mut ep_out = None;
            for ep in desc.endpoint_descriptors() {
                if ep.transfer_type() == TransferType::Bulk {
                    match ep.direction() {
                        Direction::In => ep_in = Some(ep.address()),
                        Direction::Out => ep_out = Some(ep.address()),
                    }
                }
            }
            if let (Some(i), Some(o)) = (ep_in, ep_out) {
                found.push((iface.number(), i, o));
            }
        }
    }
    if found.len() != 1 {
        anyhow::bail!("expected one fastboot bulk interface, found {found:?}");
    }
    Ok(found[0])
}

fn command(
    handle: &DeviceHandle<Context>,
    ep_out: u8,
    ep_in: u8,
    data: &[u8],
    timeout_ms: u64,
) -> Result<Vec<u8>> {
    let timeout = Duration::from_millis(timeout_ms);
    let n = handle
        .write_bulk(ep_out, data, timeout)
        .map_err(|e| anyhow::anyhow!("usb write failed: {e}"))?;
    if n != data.len() {
        anyhow::bail!("short USB write: {n}/{}", data.len());
    }
    let mut buf = [0u8; 512];
    let n = handle
        .read_bulk(ep_in, &mut buf, timeout)
        .map_err(|e| anyhow::anyhow!("usb read failed: {e}"))?;
    Ok(buf[..n].to_vec())
}

fn send_payload(rep: &dyn Reporter) -> Result<()> {
    let device = find_lk_fastboot()?;
    let (iface, ep_in, ep_out) = bulk_endpoints(&device)?;
    let handle = device
        .open()
        .map_err(|e| anyhow::anyhow!("usb open failed: {e}"))?;
    let _ = handle.set_auto_detach_kernel_driver(true);
    handle
        .claim_interface(iface)
        .map_err(|e| anyhow::anyhow!("usb claim interface failed: {e}"))?;

    rep.log(&format!(
        "raw USB: interface {iface} IN {ep_in:#04x} OUT {ep_out:#04x}"
    ));
    let r = command(&handle, ep_out, ep_in, b"getvar:is-userspace", 3500)?;
    rep.log(&format!("getvar:is-userspace = {r:?}"));
    if !r.starts_with(b"OKAY") {
        anyhow::bail!("unexpected fastboot response; no payload sent");
    }
    if r.starts_with(b"OKAYyes") || r.starts_with(b"OKAY1") {
        anyhow::bail!("this is fastbootd, not LK fastboot; no payload sent");
    }

    let r = command(&handle, ep_out, ep_in, b"getvar:unlocked", 3500)?;
    rep.log(&format!("getvar:unlocked = {r:?}"));
    if !r.starts_with(b"OKAY") {
        anyhow::bail!("unexpected fastboot response; no payload sent");
    }
    if r.starts_with(b"OKAYyes") {
        anyhow::bail!("already unlocked; refusing to send the payload");
    }

    rep.log(&format!(
        "payload ({} bytes): {}",
        PAYLOAD.len(),
        hex(&PAYLOAD)
    ));
    rep.log("sending exactly once");
    match command(&handle, ep_out, ep_in, &PAYLOAD, 5500) {
        Ok(resp) => rep.log(&format!("response = {resp:?}")),
        Err(e) => rep.log(&format!(
            "USB error after send (expected if the tablet rebooted): {e}"
        )),
    }
    rep.log("SENT ONCE. Do not resend. Check the tablet for the orange state.");
    Ok(())
}

fn hex(data: &[u8]) -> String {
    data.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn unlock(rep: &dyn Reporter, settings: &Settings) -> Result<()> {
    let summary = device::require_model(rep)?;
    if summary.locked == "0" {
        anyhow::bail!("device is already unlocked (ro.boot.flash.locked=0); nothing to do");
    }

    device::ensure_fastboot(rep)?;
    let getvar = device::fastboot_getvar_all(rep)?;
    fs::create_dir_all(&settings.workdir)?;
    fs::write(settings.workdir.join("getvar-all.txt"), getvar)?;

    let lk = device::fastboot_getvar(rep, "version-bootloader")?;
    rep.log(&format!("version-bootloader: {lk}"));
    let supported = SUPPORTED_LK.iter().map(|(v, _)| *v).collect::<Vec<_>>();
    if !device::supported_lk(&lk) {
        anyhow::bail!(
            "bootloader {lk:?} is not in the verified list {supported:?}. Do not run the payload on an unverified LK."
        );
    }
    if device::fastboot_getvar(rep, "product")? != "T405A" {
        anyhow::bail!("fastboot product is not T405A; refusing");
    }
    if device::fastboot_getvar(rep, "is-userspace")?.to_lowercase() != "no" {
        anyhow::bail!("not in LK fastboot (is-userspace); refusing");
    }
    if device::fastboot_getvar(rep, "unlocked")?.to_lowercase() != "no" {
        anyhow::bail!("already unlocked; refusing");
    }

    rep.log("");
    rep.log("WARNING - irreversible step:");
    rep.log("  * writes lock state 3 to seccfg -> unlocked/orange bootloader");
    rep.log("  * invalidates userdata encryption; a factory reset wipes all data");
    rep.log("  * no BROM/DA recovery: a bad LK/seccfg/GPT write bricks the tablet");
    rep.log("  * the payload is sent once; never resend it");
    rep.log("");

    send_payload(rep)?;

    rep.log("");
    rep.log("Wait for the tablet to reboot into recovery (init_user0_failed).");
    rep.log("On the tablet: Power + Volume Up, then Wipe data/factory reset ->");
    rep.log("Factory data reset -> Reboot system now.");
    device::wait_for_adb(rep, 900, true)?;
    let summary = device::summary(rep)?;
    if !summary.is_t405a() {
        anyhow::bail!("device not recognised after unlock");
    }
    if summary.locked != "0" {
        anyhow::bail!("still locked after the payload; do NOT resend - inspect the tablet");
    }
    rep.log(&format!(
        "unlocked: flash.locked={} verifiedbootstate={}",
        summary.locked, summary.verified_boot
    ));
    rep.log("unlock complete; take a stock backup before flashing");
    Ok(())
}
