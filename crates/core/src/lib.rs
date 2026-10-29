//! Core device logic for the onn 11" Core (T405A) LineageOS tool.
//!
//! The GUI and CLI are thin front-ends: every step is a function here that
//! reports progress through [`Reporter`] and returns an error instead of
//! exiting the process.

pub mod backup;
pub mod consts;
pub mod device;
pub mod download;
pub mod flash;
pub mod proc;
pub mod report;
pub mod unlock;

use std::path::PathBuf;

pub use consts::*;
pub use report::{Reporter, StdoutReporter};

pub type Result<T> = anyhow::Result<T>;

/// Where images, logs and backups live.
#[derive(Clone, Debug)]
pub struct Settings {
    pub workdir: PathBuf,
}

impl Settings {
    pub fn new(workdir: PathBuf) -> Self {
        Self { workdir }
    }

    pub fn images_dir(&self) -> PathBuf {
        self.workdir.join("images")
    }

    pub fn trebledroid_xz(&self) -> PathBuf {
        self.images_dir()
            .join("trebledroid")
            .join(consts::TREBLEDROID.xz_name)
    }

    pub fn trebledroid_img(&self) -> PathBuf {
        self.images_dir()
            .join("trebledroid")
            .join(consts::TREBLEDROID.img_name)
    }

    pub fn trebledroid_gz(&self) -> PathBuf {
        self.images_dir()
            .join("trebledroid")
            .join(format!("{}.gz", consts::TREBLEDROID.img_name))
    }

    pub fn lineage_dir(&self) -> PathBuf {
        self.images_dir().join("lineage")
    }

    pub fn lineage_archive(&self, gapps: bool) -> PathBuf {
        let spec = if gapps {
            &consts::LINEAGE_GAPPS
        } else {
            &consts::LINEAGE
        };
        self.lineage_dir().join(spec.archive_name)
    }

    pub fn lineage_img(&self, gapps: bool) -> PathBuf {
        let spec = if gapps {
            &consts::LINEAGE_GAPPS
        } else {
            &consts::LINEAGE
        };
        self.lineage_dir().join(spec.img_name)
    }
}

/// The one-line device summary shown by check and the GUI.
#[derive(Clone, Debug, Default)]
pub struct DeviceSummary {
    pub model: String,
    pub board: String,
    pub fingerprint: String,
    pub incremental: String,
    pub patch: String,
    pub slot: String,
    pub locked: String,
    pub verified_boot: String,
    pub lineage: String,
    pub bootloader: String,
    pub supported_lk: bool,
}

impl DeviceSummary {
    pub fn is_t405a(&self) -> bool {
        self.model == consts::MODEL && self.board == consts::BOARD
    }
}

pub fn ensure_workdir(settings: &Settings) -> Result<()> {
    std::fs::create_dir_all(&settings.workdir)
        .map_err(|e| anyhow::anyhow!("could not create {}: {e}", settings.workdir.display()))?;
    Ok(())
}

/// Print tools and device state. Works with no device attached.
pub fn check(rep: &dyn Reporter) -> Result<Option<DeviceSummary>> {
    for (tool, arg) in [("adb", "version"), ("fastboot", "--version")] {
        let ok = std::process::Command::new(tool)
            .arg(arg)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        rep.log(&format!("{tool}: {}", if ok { "ok" } else { "MISSING" }));
    }

    if device::adb_present() {
        let mut s = device::summary(rep)?;
        rep.log("");
        rep.log("android:");
        rep.log(&format!("  model / board: {} / {}", s.model, s.board));
        rep.log(&format!("  fingerprint: {}", s.fingerprint));
        rep.log(&format!("  build / patch: {} / {}", s.incremental, s.patch));
        rep.log(&format!("  slot: {}", s.slot));
        rep.log(&format!(
            "  bootloader: {} ({})",
            if s.locked == "0" {
                "UNLOCKED"
            } else {
                "locked"
            },
            s.verified_boot
        ));
        if !s.lineage.is_empty() {
            rep.log(&format!("  running: {}", s.lineage));
        }
        if !s.is_t405a() {
            rep.log("WARNING: this is not an onn 11\" Core (T405A)");
        }
        s.bootloader = "n/a (android)".to_string();
        s.supported_lk = true;
        return Ok(Some(s));
    }

    if device::fastboot_serial().is_some() {
        let lk = device::fastboot_getvar(rep, "version-bootloader")?;
        let supported = device::supported_lk(&lk);
        rep.log("");
        rep.log("fastboot:");
        rep.log(&format!(
            "  product: {}",
            device::fastboot_getvar(rep, "product")?
        ));
        rep.log(&format!("  version-bootloader: {lk}"));
        rep.log(&format!(
            "  unlocked: {}",
            device::fastboot_getvar(rep, "unlocked")?
        ));
        rep.log(&format!(
            "  is-userspace: {}",
            device::fastboot_getvar(rep, "is-userspace")?
        ));
        rep.log(&format!(
            "  current-slot: {}",
            device::fastboot_getvar(rep, "current-slot")?
        ));
        rep.log(&format!(
            "  supported LK: {}",
            if supported {
                "yes"
            } else {
                "NO - do not unlock"
            }
        ));
        let s = DeviceSummary {
            bootloader: lk,
            supported_lk: supported,
            ..Default::default()
        };
        return Ok(Some(s));
    }

    rep.log("");
    rep.log(
        "no device via adb or fastboot. Connect USB and enable Developer options + USB debugging.",
    );
    rep.log("Supported bootloaders:");
    for (lk, desc) in SUPPORTED_LK {
        rep.log(&format!("  {lk}  ({desc})"));
    }
    Ok(None)
}

pub fn download_all(rep: &dyn Reporter, settings: &Settings) -> Result<()> {
    download::ensure_trebledroid(rep, settings)?;
    download::ensure_lineage(rep, settings, false)?;
    Ok(())
}
