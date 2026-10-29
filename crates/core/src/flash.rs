//! Flashing a GSI and restoring stock images.

use crate::backup::{load_backup, verify_backup};
use crate::consts::RESTORE_PHYSICAL_A;
use crate::download;
use crate::proc;
use crate::report::Reporter;
use crate::{device, Result, Settings};
use std::fs;
use std::path::Path;

fn parse_size(value: &str) -> Option<u64> {
    let v = value.trim();
    let v = v.strip_prefix("0x").unwrap_or(v);
    u64::from_str_radix(v, 16).ok()
}

/// Flash a raw ext4 arm64 GSI.  `image` overrides the default LineageOS image.
pub fn flash(
    rep: &dyn Reporter,
    settings: &Settings,
    backup: &Path,
    image: Option<&Path>,
    gapps: bool,
    force: bool,
    dry_run: bool,
) -> Result<()> {
    let manifest = load_backup(backup)?;
    verify_backup(rep, backup, &manifest)?;

    if image.is_some() && gapps {
        anyhow::bail!("--gapps cannot be combined with a custom image");
    }
    let custom = image.is_some();
    let image = match image {
        Some(path) => download::ensure_custom_image(rep, path)?,
        None => download::ensure_lineage(rep, settings, gapps)?,
    };
    let image_size = fs::metadata(&image)?.len();
    let variant = if custom {
        "Custom GSI"
    } else if gapps {
        "LineageOS 23.2 + GApps"
    } else {
        "LineageOS 23.2 (vanilla)"
    };
    rep.log(&format!(
        "{variant}: {} ({image_size} bytes)",
        image.display()
    ));

    if dry_run {
        rep.log("dry run; would run:");
        rep.log("  adb reboot fastboot");
        rep.log("  fastboot resize-logical-partition system_a <size>   # if needed");
        rep.log(&format!("  fastboot flash system_a {}", image.display()));
        rep.log("  fastboot reboot bootloader");
        rep.log(&format!(
            "  fastboot --disable-verity --disable-verification flash vbmeta_a {}",
            backup.join("vbmeta_a.img").display()
        ));
        rep.log("  fastboot reboot recovery   # then Factory data reset on the tablet");
        return Ok(());
    }

    let summary = device::require_model(rep)?;
    if summary.locked != "0" {
        anyhow::bail!("bootloader is locked; unlock first");
    }
    if !summary.lineage.is_empty() && !force {
        anyhow::bail!("a LineageOS GSI is already running; pass --force to replace it");
    }

    device::ensure_adb(rep)?;
    proc::adb_ok(rep, &["reboot", "fastboot"])?;
    device::wait_for_fastboot(rep, 180)?;
    if device::fastboot_getvar(rep, "is-userspace")?.to_lowercase() != "yes" {
        anyhow::bail!("not in fastbootd (is-userspace != yes); try `adb reboot fastboot` again");
    }

    let current =
        parse_size(&device::fastboot_getvar(rep, "partition-size:system_a")?).unwrap_or(0);
    rep.log(&format!("system_a size: {current} bytes"));
    if current < image_size {
        let target = std::cmp::max(image_size + 64 * 1024 * 1024, 3_000_000_000);
        rep.log(&format!("resizing system_a to {target} bytes"));
        proc::fastboot_ok(
            rep,
            &["resize-logical-partition", "system_a", &target.to_string()],
        )?;
        let now =
            parse_size(&device::fastboot_getvar(rep, "partition-size:system_a")?).unwrap_or(0);
        if now < image_size {
            anyhow::bail!("system_a is still too small ({now} < {image_size})");
        }
    }

    let image_str = image
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("non-UTF8 image path"))?;
    rep.log("flashing system_a");
    proc::fastboot_ok(rep, &["flash", "system_a", image_str])?;

    rep.log("rebooting to LK fastboot to patch vbmeta_a");
    proc::fastboot_ok(rep, &["reboot", "bootloader"])?;
    device::wait_for_fastboot(rep, 180)?;
    let vbmeta = backup.join("vbmeta_a.img");
    let vbmeta_str = vbmeta
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("non-UTF8 vbmeta path"))?;
    proc::fastboot_ok(
        rep,
        &[
            "--disable-verity",
            "--disable-verification",
            "flash",
            "vbmeta_a",
            vbmeta_str,
        ],
    )?;

    rep.log("");
    rep.log("Now do the required factory reset on the tablet:");
    rep.log(
        "  recovery menu -> Wipe data/factory reset -> Factory data reset -> Reboot system now",
    );
    proc::fastboot_ok(rep, &["reboot", "recovery"])?;
    device::wait_for_adb(rep, 3600, true)?;
    let now = device::summary(rep)?;
    if !now.lineage.is_empty() {
        rep.log(&format!("LineageOS booted: {}", now.lineage));
    } else {
        rep.log(&format!(
            "GSI booted: fingerprint={:?} slot={}",
            now.fingerprint, now.slot
        ));
    }
    rep.log("Run the hardware checks: touch, rotation, speakers/mic, both cameras,");
    rep.log("Bluetooth pairing, brightness, charging, sleep/wake.");
    Ok(())
}

/// Restore the stock boot/vbmeta/super images from a backup.
pub fn restore(
    rep: &dyn Reporter,
    _settings: &Settings,
    backup: &Path,
    no_super: bool,
    dry_run: bool,
) -> Result<()> {
    let manifest = load_backup(backup)?;
    verify_backup(rep, backup, &manifest)?;

    let mut commands: Vec<Vec<String>> = Vec::new();
    for name in RESTORE_PHYSICAL_A {
        commands.push(vec![
            "flash".into(),
            (*name).to_string(),
            backup.join(format!("{name}.img")).display().to_string(),
        ]);
    }
    if !no_super {
        commands.push(vec![
            "flash".into(),
            "super".into(),
            backup.join("super.img").display().to_string(),
        ]);
    }

    if dry_run {
        rep.log("would run (in LK fastboot):");
        for cmd in &commands {
            rep.log(&format!("  fastboot {}", cmd.join(" ")));
        }
        rep.log("  fastboot reboot recovery   # then Factory data reset");
        rep.log(
            "Never flashed: lk, preloader, tee, gz, seccfg, protect, nvram, nvdata, nvcfg, GPT.",
        );
        return Ok(());
    }

    device::ensure_fastboot(rep)?;
    if device::fastboot_getvar(rep, "is-userspace")?.to_lowercase() == "yes" {
        rep.log("in fastbootd; rebooting to LK fastboot");
        proc::fastboot_ok(rep, &["reboot", "bootloader"])?;
        device::wait_for_fastboot(rep, 180)?;
    }
    if device::fastboot_getvar(rep, "is-userspace")?.to_lowercase() != "no" {
        anyhow::bail!("restore needs LK fastboot, not fastbootd");
    }

    for cmd in &commands {
        let path = Path::new(cmd.last().unwrap());
        if !path.exists() {
            anyhow::bail!("missing backup image {}", path.display());
        }
    }
    for cmd in &commands {
        let args: Vec<&str> = cmd.iter().map(String::as_str).collect();
        proc::fastboot_ok(rep, &args)?;
    }

    rep.log("");
    rep.log("Restore images written. On the tablet: recovery -> Wipe data/factory reset.");
    proc::fastboot_ok(rep, &["reboot", "recovery"])?;
    rep.log("The bootloader stays unlocked; do not run `fastboot flashing lock`.");
    Ok(())
}
