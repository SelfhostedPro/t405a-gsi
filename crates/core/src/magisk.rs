//! Magisk root and Play certification (GSF ID) helpers.
//!
//! Magisk v31 no longer prompts for root access: the Superuser tab pre-grants
//! it, and images are patched inside the app (the release ships no host
//! patcher). This module automates everything around that one app step:
//! download, push, watch for the patched image, flash and verify.
//!
//! Play Store "device is not certified" is fixed by registering the GSF ID at
//! Google's uncertified page, not by Magisk. Magisk and PlayIntegrityFork are
//! for Play Integrity verdicts (Wallet, banking apps).

use crate::consts::{MAGISK, PLAY_INTEGRITY_FIX};
use crate::download;
use crate::proc;
use crate::report::Reporter;
use crate::{backup, device, Result, Settings};
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const MAGISK_PACKAGE: &str = "com.topjohnwu.magisk";
const REGISTER_URL: &str = "https://www.google.com/android/uncertified/";
const PATCH_TIMEOUT_SECS: u64 = 1800;

/// The IDs Google's Play certification page uses.
#[derive(Clone, Debug)]
pub struct PlayIds {
    /// `Settings.Secure.ANDROID_ID`; shown for reference, not for registration.
    pub android_id: String,
    /// The GSF ID exactly as the device printed it, ready to paste.
    pub gsf_value: Option<String>,
    /// GSF ID as 16 lowercase hex digits.
    pub gsf_hex: Option<String>,
    /// The same GSF ID in decimal.
    pub gsf_decimal: Option<u64>,
    /// The full output of the command that produced the GSF ID.
    pub raw: Option<String>,
    /// Where the value came from ("content provider", "gservices.db", ...).
    pub via: &'static str,
}

impl PlayIds {
    pub fn register_url(&self) -> &'static str {
        REGISTER_URL
    }
}

fn adb_quiet(args: &[&str]) -> Option<String> {
    let out = Command::new("adb").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn root_shell_uid() -> Option<String> {
    let out = Command::new("adb")
        .args(["shell", "su -c 'id -u'"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn has_shell_root() -> bool {
    root_shell_uid().as_deref() == Some("0")
}

pub fn ensure_magisk_apk(rep: &dyn Reporter, settings: &Settings) -> Result<PathBuf> {
    download::download_asset(rep, &MAGISK, &settings.magisk_apk())
}

pub fn ensure_pif(rep: &dyn Reporter, settings: &Settings) -> Result<PathBuf> {
    download::download_asset(rep, &PLAY_INTEGRITY_FIX, &settings.pif_zip())
}

/// `_a` or `_b`, from the booted system.
fn active_slot(rep: &dyn Reporter) -> Result<String> {
    let mut slot = device::getprop(rep, "ro.boot.slot_suffix")?;
    if slot.is_empty() {
        slot = device::getprop(rep, "ro.boot.slot")?;
        if !slot.is_empty() && !slot.starts_with('_') {
            slot.insert(0, '_');
        }
    }
    if slot != "_a" && slot != "_b" {
        anyhow::bail!("cannot determine the active slot (got {slot:?})");
    }
    Ok(slot)
}

fn check_boot_image(path: &Path) -> Result<()> {
    let mut f = fs::File::open(path)?;
    let mut magic = [0u8; 8];
    f.read_exact(&mut magic)
        .map_err(|_| anyhow::anyhow!("{} is too small to be an init_boot image", path.display()))?;
    if &magic != b"ANDROID!" {
        anyhow::bail!(
            "{} is not an Android boot/init_boot image (magic {:02x?})",
            path.display(),
            magic
        );
    }
    Ok(())
}

fn patched_files() -> Vec<String> {
    let Some(listing) = adb_quiet(&["shell", "ls", "-t", "/sdcard/Download"]) else {
        return Vec::new();
    };
    listing
        .lines()
        .map(str::trim)
        .filter(|name| name.starts_with("magisk_patched") && name.ends_with(".img"))
        .map(str::to_string)
        .collect()
}

fn wait_for_new_patch(
    rep: &dyn Reporter,
    before: &HashSet<String>,
    timeout_secs: u64,
) -> Result<String> {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let mut warned = false;
    while Instant::now() < deadline {
        if device::adb_state() != "device" {
            if !warned {
                rep.log("waiting for adb to come back...");
                warned = true;
            }
        } else if let Some(name) = patched_files().into_iter().find(|n| !before.contains(n)) {
            return Ok(name);
        }
        std::thread::sleep(Duration::from_secs(4));
    }
    anyhow::bail!(
        "timed out after {timeout_secs}s waiting for a new magisk_patched-*.img in Download; \
         patch init_boot in the Magisk app, or pass --patched with an image patched earlier"
    )
}

fn print_plan(rep: &dyn Reporter, backup: &Path, patched: Option<&Path>, slot: &str) {
    let patched = match patched {
        Some(path) => path.display().to_string(),
        None => "<workdir>/magisk/magisk_patched.img".to_string(),
    };
    rep.log("dry run; would run:");
    rep.log("  adb install -r <workdir>/magisk/Magisk-v31.1.apk");
    rep.log(&format!(
        "  adb push {}/init_boot{slot}.img /sdcard/Download/init_boot{slot}.img",
        backup.display()
    ));
    rep.log("  # on the tablet: Magisk -> Install -> Patch firmware / image");
    rep.log("  #                -> Select from storage -> Download/init_boot<slot>.img");
    rep.log("  adb pull /sdcard/Download/magisk_patched-*.img <workdir>/magisk/");
    rep.log("  adb reboot fastboot; fastboot reboot bootloader");
    rep.log(&format!("  fastboot flash init_boot{slot} {patched}"));
    rep.log("  fastboot reboot");
    rep.log("");
    rep.log("This writes init_boot<slot> only; it never touches lk, preloader, seccfg or vbmeta.");
}

/// Patch init_boot with Magisk and flash it. `patched` skips the app step.
pub fn install(
    rep: &dyn Reporter,
    settings: &Settings,
    backup_dir: &Path,
    patched: Option<&Path>,
    dry_run: bool,
) -> Result<()> {
    if dry_run {
        print_plan(rep, backup_dir, patched, "<slot>");
        return Ok(());
    }

    let manifest = backup::load_backup(backup_dir)?;
    let summary = device::require_model(rep)?;
    if summary.locked != "0" {
        anyhow::bail!("bootloader is locked; unlock first");
    }
    let slot = active_slot(rep)?;
    let stock = backup_dir.join(format!("init_boot{slot}.img"));
    if !stock.exists() {
        anyhow::bail!(
            "the backup has no init_boot{slot}.img; Magisk patches the stock init_boot for the \
             active slot"
        );
    }
    if let Some(info) = manifest.partitions.get(&format!("init_boot{slot}")) {
        if info.sha256 != download::sha256_file(&stock)? {
            anyhow::bail!("init_boot{slot}.img does not match the backup manifest");
        }
        rep.log(&format!("stock init_boot{slot}.img verified"));
    }

    let image = match patched {
        Some(path) => {
            check_boot_image(path)?;
            rep.log(&format!("using patched image {}", path.display()));
            path.to_path_buf()
        }
        None => patch_with_app(rep, settings, &stock, &slot)?,
    };

    let image_str = image
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("non-UTF8 patched image path"))?;

    rep.log("rebooting to fastbootd");
    device::ensure_adb(rep)?;
    proc::adb_ok(rep, &["reboot", "fastboot"])?;
    device::wait_for_fastboot(rep, 180)?;
    if device::fastboot_getvar(rep, "is-userspace")?.to_lowercase() != "yes" {
        anyhow::bail!("not in fastbootd (is-userspace != yes); try `adb reboot fastboot` again");
    }
    rep.log("rebooting to LK fastboot");
    proc::fastboot_ok(rep, &["reboot", "bootloader"])?;
    device::wait_for_fastboot(rep, 180)?;
    if device::fastboot_getvar(rep, "is-userspace")?.to_lowercase() != "no" {
        anyhow::bail!("need LK fastboot to write init_boot{slot}");
    }

    rep.log(&format!("flashing init_boot{slot}"));
    proc::fastboot_ok(rep, &["flash", &format!("init_boot{slot}"), image_str])?;
    proc::fastboot_ok(rep, &["reboot"])?;
    rep.log("waiting for Android");
    device::wait_for_adb(rep, 600, true)?;

    if device::getprop(rep, "ro.boot.slot_suffix")? != slot {
        rep.log("WARNING: the active slot changed during the flash");
    }
    if adb_quiet(&["shell", "which", "magisk"]).is_some_and(|p| !p.is_empty()) {
        rep.log("Magisk is running (magisk binary in PATH).");
    } else {
        rep.log("WARNING: could not confirm Magisk; open the Magisk app and check its status.");
    }
    if adb_quiet(&["shell", "pm", "list", "packages", MAGISK_PACKAGE])
        .is_some_and(|p| p.contains(MAGISK_PACKAGE))
    {
        rep.log("Magisk app installed.");
    }
    rep.log("");
    rep.log("Next:");
    rep.log("  - Open Magisk; finish any setup it asks for (it reboots).");
    rep.log("  - Superuser tab -> allow Shell (needed by playfix/playid).");
    rep.log("  - For Play Integrity: Settings -> Zygisk, then run `playfix`.");
    rep.log("  - For Play Store certification: run `playid` and register the GSF ID.");
    rep.log("  - `restore` from the stock backup removes Magisk again.");
    Ok(())
}

fn patch_with_app(
    rep: &dyn Reporter,
    settings: &Settings,
    stock: &Path,
    slot: &str,
) -> Result<PathBuf> {
    let apk = ensure_magisk_apk(rep, settings)?;
    let apk_str = apk
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("non-UTF8 APK path"))?;
    rep.log("installing the Magisk app");
    proc::adb_ok(rep, &["install", "-r", apk_str])?;

    let name = format!("init_boot{slot}.img");
    let stock_str = stock
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("non-UTF8 init_boot path"))?;
    rep.log(&format!("pushing the stock {name} to Download"));
    proc::adb_ok(
        rep,
        &["push", stock_str, &format!("/sdcard/Download/{name}")],
    )?;

    let before: HashSet<String> = patched_files().into_iter().collect();
    rep.log("");
    rep.log("On the tablet, patch the stock image with Magisk:");
    rep.log("  1. Open Magisk -> Install -> Patch firmware / image");
    rep.log(&format!("  2. Select from storage -> Download/{name}"));
    rep.log("  3. Wait for 'All done!' and return here (do not reboot).");
    rep.log("Watching /sdcard/Download for magisk_patched-*.img (up to 30 minutes)...");

    let device_name = wait_for_new_patch(rep, &before, PATCH_TIMEOUT_SECS)?;
    if !device_name.ends_with(".img") || device_name.contains('/') {
        anyhow::bail!("unexpected patched file name {device_name:?}");
    }
    let dest = settings.patched_init_boot();
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let dest_str = dest
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("non-UTF8 workdir path"))?;
    rep.log(&format!("pulling {device_name}"));
    proc::adb_ok(
        rep,
        &["pull", &format!("/sdcard/Download/{device_name}"), dest_str],
    )?;
    check_boot_image(&dest)?;
    rep.log(&format!("patched image saved to {}", dest.display()));
    Ok(dest)
}

/// Install PlayIntegrityFork. Needs Zygisk; the module ships no fingerprint.
pub fn install_pif(rep: &dyn Reporter, settings: &Settings, dry_run: bool) -> Result<()> {
    let remote = format!("/sdcard/Download/{}", PLAY_INTEGRITY_FIX.file_name);
    if dry_run {
        rep.log("dry run; would run:");
        rep.log(&format!(
            "  adb push <workdir>/magisk/{} {remote}",
            PLAY_INTEGRITY_FIX.file_name
        ));
        rep.log(&format!(
            "  adb shell su -c \"magisk --install-module {remote}\"   # if Shell is granted"
        ));
        rep.log("  # otherwise: Magisk -> Modules -> Install from storage");
        rep.log("  # Magisk -> Settings -> enable Zygisk; reboot");
        rep.log(
            "PIF ships no fingerprint: run its Action button (autopif4) or add custom.pif.prop.",
        );
        return Ok(());
    }

    device::require_model(rep)?;
    let zip = ensure_pif(rep, settings)?;
    let zip_str = zip
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("non-UTF8 module path"))?;
    rep.log("pushing the module");
    proc::adb_ok(rep, &["push", zip_str, &remote])?;

    if has_shell_root() {
        let cmd = format!("su -c 'magisk --install-module {remote}'");
        match proc::adb(rep, &["shell", &cmd]) {
            Ok(out) => {
                if !out.trim().is_empty() {
                    rep.log(out.trim());
                }
                rep.log("rebooting to load the module");
                proc::adb_ok(rep, &["reboot"])?;
                device::wait_for_adb(rep, 600, true)?;
                rep.log("PlayIntegrityFork installed.");
            }
            Err(err) => {
                rep.log(&format!("automatic module install failed: {err:#}"));
                pif_instructions(rep);
                return Ok(());
            }
        }
    } else {
        rep.log("Shell root is not granted yet; install the module in the Magisk app:");
        pif_instructions(rep);
        return Ok(());
    }

    pif_instructions(rep);
    Ok(())
}

fn pif_instructions(rep: &dyn Reporter) {
    rep.log("");
    rep.log("PlayIntegrityFork:");
    rep.log("  - Magisk -> Settings -> enable Zygisk, then reboot.");
    rep.log(&format!(
        "  - Magisk -> Modules -> Install from storage -> Download/{}",
        PLAY_INTEGRITY_FIX.file_name
    ));
    rep.log("  - v18 ships no fingerprint. Run the module's Action button (autopif4) or add a");
    rep.log("    custom.pif.prop; without one it will not fix DEVICE integrity.");
    rep.log("  - Play Store certification itself is fixed by registering the GSF ID (`playid`).");
}

fn gsf_value(raw: &str) -> Option<&str> {
    let line = raw.lines().map(str::trim).find(|l| !l.is_empty())?;
    let mut value = line;
    if let Some((_, after)) = value.rsplit_once("value=") {
        value = after;
    }
    if let Some((_, after)) = value.rsplit_once('|') {
        value = after;
    }
    let value = value.trim().trim_matches('"');
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn parse_gsf_value(value: &str) -> Option<(u64, String)> {
    if let Ok(number) = value.parse::<u64>() {
        return Some((number, format!("{number:x}")));
    }
    // gservices stores what Java wrote: a signed long, negative about half
    // the time. The hex form is the same two's-complement bits.
    if let Ok(number) = value.parse::<i64>() {
        let bits = number as u64;
        return Some((bits, format!("{bits:x}")));
    }
    if value.len() >= 8 && value.len() <= 16 && value.chars().all(|c| c.is_ascii_hexdigit()) {
        let number = u64::from_str_radix(value, 16).ok()?;
        return Some((number, value.to_ascii_lowercase()));
    }
    None
}

fn parse_gsf(raw: &str) -> Option<(u64, String)> {
    parse_gsf_value(gsf_value(raw)?)
}

/// The command printed on Google's uncertified-device page. The device shell
/// expands the current user and the package/database wildcard.
const GOOGLE_GSF_COMMAND: &str =
    "sqlite3 /data/user/$(cmd activity get-current-user)/*/*/gservices.db \
     \"select * from main where name = \\\"android_id\\\";\"";

fn shell_uid() -> Option<String> {
    adb_quiet(&["shell", "id", "-u"])
}

/// Run a command as root: directly when adbd itself is root (rooted USB
/// debugging), otherwise through Magisk's `su`. The command must not contain
/// single quotes because it is wrapped in them for `su -c`.
fn run_as_root(cmd: &str) -> Option<String> {
    if shell_uid().as_deref() == Some("0") {
        if let Some(out) = adb_quiet(&["shell", cmd]) {
            if !out.is_empty() {
                return Some(out);
            }
        }
    }
    if has_shell_root() {
        return adb_quiet(&["shell", &format!("su -c '{cmd}'")]);
    }
    None
}

fn gsf_from_google_command() -> Option<String> {
    run_as_root(GOOGLE_GSF_COMMAND)
}

fn gsf_from_sqlite_path() -> Option<String> {
    run_as_root(
        "sqlite3 /data/data/com.google.android.gsf/databases/gservices.db \
         \"select value from main where name = \\\"android_id\\\"\"",
    )
}

fn gsf_from_content_provider() -> Option<String> {
    let cmd = "content query --uri content://com.google.android.gsf.gservices \
               --projection value --where \"name='android_id'\"";
    adb_quiet(&["shell", cmd])
}

/// Read the Android ID and, if the shell can, the GSF ID used for Play
/// certification.
pub fn play_ids(rep: &dyn Reporter) -> Result<PlayIds> {
    device::wait_for_adb(rep, 300, true)?;
    let android_id = proc::adb(rep, &["shell", "settings", "get", "secure", "android_id"])?
        .trim()
        .to_string();

    type GsfSource = (fn() -> Option<String>, &'static str);
    let mut via = "unavailable";
    let mut raw = None;
    let mut parsed = None;
    let mut gsf_value_text = None;
    let sources: [GsfSource; 3] = [
        (
            gsf_from_google_command,
            "sqlite3 (Google uncertified command)",
        ),
        (gsf_from_sqlite_path, "gservices.db (sqlite)"),
        (
            gsf_from_content_provider,
            "com.google.android.gsf content provider",
        ),
    ];
    for (fetch, name) in sources {
        let Some(output) = fetch() else { continue };
        if let Some(ids) = parse_gsf(&output) {
            gsf_value_text = gsf_value(&output).map(str::to_string);
            parsed = Some(ids);
            raw = Some(output);
            via = name;
            break;
        }
        if raw.is_none() {
            raw = Some(output);
        }
    }

    let (gsf_decimal, gsf_hex) = match parsed {
        Some((number, hex)) => (Some(number), Some(hex)),
        None => (None, None),
    };
    Ok(PlayIds {
        android_id,
        gsf_value: gsf_value_text,
        gsf_hex,
        gsf_decimal,
        raw,
        via,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_decimal() {
        assert_eq!(
            parse_gsf("1234567890123456789"),
            Some((1234567890123456789, "112210f47de98115".to_string()))
        );
    }

    #[test]
    fn parses_hex() {
        assert_eq!(
            parse_gsf("1234567890ABCDEF"),
            Some((0x1234567890abcdef, "1234567890abcdef".to_string()))
        );
    }

    #[test]
    fn parses_provider_and_sqlite_shapes() {
        assert_eq!(
            parse_gsf("Row: 0 value=1234567890ABCDEF"),
            Some((0x1234567890abcdef, "1234567890abcdef".to_string()))
        );
        assert_eq!(
            parse_gsf("android_id|1234567890ABCDEF"),
            Some((0x1234567890abcdef, "1234567890abcdef".to_string()))
        );
    }

    #[test]
    fn google_command_matches_the_page() {
        assert_eq!(
            GOOGLE_GSF_COMMAND,
            "sqlite3 /data/user/$(cmd activity get-current-user)/*/*/gservices.db \
             \"select * from main where name = \\\"android_id\\\";\""
        );
    }

    #[test]
    fn extracts_the_paste_value() {
        assert_eq!(
            gsf_value("android_id|1234567890ABCDEF"),
            Some("1234567890ABCDEF")
        );
        assert_eq!(gsf_value("Row: 0 value=123"), Some("123"));
    }

    #[test]
    fn parses_negative_signed_long() {
        assert_eq!(
            parse_gsf("-1"),
            Some((u64::MAX, "ffffffffffffffff".to_string()))
        );
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_gsf(""), None);
        assert_eq!(parse_gsf("not-an-id"), None);
    }
}
