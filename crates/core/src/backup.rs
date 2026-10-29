//! Stock backup through a DSU userdebug GSI.

use crate::consts::{DSU_USERDATA, SKIP_PARTITIONS, TREBLEDROID};
use crate::download;
use crate::proc;
use crate::report::Reporter;
use crate::{device, Result, Settings};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartitionInfo {
    pub size: u64,
    pub sha256: String,
    pub file: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackupManifest {
    pub created: String,
    pub fingerprint: String,
    pub incremental: String,
    pub patch: String,
    pub slot: String,
    pub gsi_fingerprint: String,
    pub partitions: BTreeMap<String, PartitionInfo>,
}

pub fn backup(rep: &dyn Reporter, settings: &Settings, out: Option<&Path>) -> Result<PathBuf> {
    crate::ensure_workdir(settings)?;
    let stock = device::require_model(rep)?;
    if stock.locked != "0" {
        anyhow::bail!("bootloader is locked; run unlock first");
    }
    if !stock.lineage.is_empty() {
        anyhow::bail!("this is already a LineageOS/GSI system; stock backup is impossible now");
    }

    let outdir = match out {
        Some(p) => p.to_path_buf(),
        None => settings.workdir.join(format!(
            "backup-{}",
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        )),
    };
    if outdir.exists() {
        anyhow::bail!("{} already exists; choose another --out", outdir.display());
    }

    let gz = download::ensure_trebledroid_gz(rep, settings)?;
    install_dsu(rep, &gz)?;
    wait_for_dsu(rep)?;

    proc::adb_ok(rep, &["root"])?;
    device::wait_for_adb(rep, 120, true)?;
    let uid = proc::adb(rep, &["shell", "id", "-u"])?;
    if uid.trim() != "0" {
        anyhow::bail!("adb root failed on the DSU GSI");
    }

    dump_partitions(rep, &outdir, &stock)?;

    rep.log("removing the DSU and returning to stock");
    proc::adb_ok(rep, &["shell", "gsi_tool", "wipe"])?;
    proc::adb_ok(rep, &["reboot"])?;
    device::wait_for_adb(rep, 600, true)?;
    let deadline = Instant::now() + Duration::from_secs(180);
    while Instant::now() < deadline {
        if device::getprop(rep, "ro.build.fingerprint")? == stock.fingerprint {
            break;
        }
        std::thread::sleep(Duration::from_secs(3));
    }
    let now = device::getprop(rep, "ro.build.fingerprint")?;
    if now != stock.fingerprint {
        rep.log(&format!(
            "WARNING: expected stock fingerprint {:?}, got {now:?}",
            stock.fingerprint
        ));
    }
    rep.log(&format!("stock backup done: {}", outdir.display()));
    Ok(outdir)
}

fn install_dsu(rep: &dyn Reporter, image_gz: &Path) -> Result<()> {
    rep.log("pushing the DSU image to /sdcard/Download");
    let gz = image_gz
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("non-UTF8 image path"))?;
    proc::adb_ok(rep, &["push", gz, "/sdcard/Download/"])?;

    let uri = format!(
        "file:///storage/emulated/0/Download/{}",
        image_gz
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("image.gz")
    );
    rep.log("starting the DSU install");
    proc::adb_ok(
        rep,
        &[
            "shell",
            "am",
            "start-activity",
            "-n",
            "com.android.dynsystem/com.android.dynsystem.VerificationActivity",
            "-a",
            "android.os.image.action.START_INSTALL",
            "-d",
            &uri,
            "--el",
            "KEY_SYSTEM_SIZE",
            &TREBLEDROID.raw_size.to_string(),
            "--el",
            "KEY_USERDATA_SIZE",
            &DSU_USERDATA.to_string(),
        ],
    )?;

    let deadline = Instant::now() + Duration::from_secs(1200);
    let mut last = String::new();
    while Instant::now() < deadline {
        let status = proc::adb(rep, &["shell", "gsi_tool", "status"])?;
        if status != last {
            rep.log(&format!("gsi_tool status: {}", status.trim()));
            last = status.clone();
        }
        if status.contains("installed") || status.contains("enabled") {
            break;
        }
        std::thread::sleep(Duration::from_secs(5));
    }
    if !(last.contains("installed") || last.contains("enabled")) {
        anyhow::bail!("timed out waiting for the DSU install");
    }

    proc::adb_ok(rep, &["shell", "gsi_tool", "enable"])?;
    rep.log("rebooting into the DSU");
    proc::adb_ok(rep, &["reboot"])?;
    Ok(())
}

fn wait_for_dsu(rep: &dyn Reporter) -> Result<()> {
    device::wait_for_adb(rep, 300, true)?;
    let deadline = Instant::now() + Duration::from_secs(300);
    while Instant::now() < deadline {
        let fp = device::getprop(rep, "ro.build.fingerprint")?;
        if fp.contains("tdgsi") || fp.contains("treble_arm64") {
            rep.log(&format!("booted DSU: {fp}"));
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(3));
    }
    anyhow::bail!(
        "DSU did not boot (fingerprint {:?})",
        device::getprop(rep, "ro.build.fingerprint")?
    )
}

fn dump_partitions(rep: &dyn Reporter, outdir: &Path, stock: &crate::DeviceSummary) -> Result<()> {
    fs::create_dir_all(outdir)?;
    let free = fs2::available_space(outdir)?;
    if free < 16 * 1024 * 1024 * 1024 {
        anyhow::bail!(
            "need at least 16 GB free on {}; have {:.1} GB",
            outdir.display(),
            free as f64 / 1073741824.0
        );
    }

    let listing = proc::adb(rep, &["shell", "ls", "/dev/block/by-name"])?;
    let mut names = listing
        .split_whitespace()
        .filter(|n| !SKIP_PARTITIONS.contains(n))
        .map(str::to_string)
        .collect::<Vec<_>>();
    names.sort();
    if names.is_empty() {
        anyhow::bail!("could not list /dev/block/by-name");
    }

    let mut manifest = BackupManifest {
        created: chrono::Local::now().to_rfc3339(),
        fingerprint: stock.fingerprint.clone(),
        incremental: stock.incremental.clone(),
        patch: stock.patch.clone(),
        slot: stock.slot.clone(),
        gsi_fingerprint: device::getprop(rep, "ro.build.fingerprint")?,
        partitions: BTreeMap::new(),
    };
    let mut sums = Vec::new();

    for name in names {
        let dev = format!("/dev/block/by-name/{name}");
        let size_text = proc::adb(rep, &["shell", "blockdev", "--getsize64", &dev])?;
        let size: u64 = match size_text.trim().parse() {
            Ok(v) if v > 0 => v,
            _ => {
                rep.log(&format!("skip {name} (size {size_text:?})"));
                continue;
            }
        };
        let dest = outdir.join(format!("{name}.img"));
        rep.log(&format!("dumping {name} ({size} bytes)"));
        let stdout = File::create(&dest)?;
        let status = Command::new("adb")
            .args(["exec-out", &format!("dd if={dev} bs=4M 2>/dev/null")])
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::null())
            .status()
            .map_err(|e| anyhow::anyhow!("could not run adb: {e}"))?;
        if !status.success() {
            anyhow::bail!("dd failed for {name}");
        }
        let got = fs::metadata(&dest)?.len();
        if got != size {
            anyhow::bail!("{name}: {got} bytes, expected {size}");
        }
        let digest = download::sha256_file(&dest)?;
        let file = format!("{name}.img");
        sums.push(format!("{digest}  {file}"));
        manifest.partitions.insert(
            name,
            PartitionInfo {
                size,
                sha256: digest,
                file,
            },
        );
    }

    fs::write(outdir.join("SHA256SUMS"), sums.join("\n") + "\n")?;
    fs::write(
        outdir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)? + "\n",
    )?;
    let mut rows = String::new();
    for (name, info) in &manifest.partitions {
        rows.push_str(&format!("{name} {} {}\n", info.size, info.file));
    }
    fs::write(outdir.join("partitions.txt"), rows)?;
    rep.log(&format!(
        "backup complete: {} images, SHA256SUMS and manifest.json in {}",
        manifest.partitions.len(),
        outdir.display()
    ));
    Ok(())
}

pub fn load_backup(backup: &Path) -> Result<BackupManifest> {
    if !backup.is_dir() {
        anyhow::bail!("backup directory not found: {}", backup.display());
    }
    let manifest_path = backup.join("manifest.json");
    let sums_path = backup.join("SHA256SUMS");
    if !sums_path.exists() {
        anyhow::bail!(
            "{} has no SHA256SUMS; not a backup made by this tool",
            backup.display()
        );
    }
    let manifest = if manifest_path.exists() {
        serde_json::from_str(&fs::read_to_string(&manifest_path)?)?
    } else {
        let mut partitions = BTreeMap::new();
        for line in fs::read_to_string(&sums_path)?.lines() {
            let mut parts = line.split_whitespace();
            let (Some(digest), Some(file)) = (parts.next(), parts.next_back()) else {
                continue;
            };
            let name = file.trim_end_matches(".img").to_string();
            partitions.insert(
                name,
                PartitionInfo {
                    size: 0,
                    sha256: digest.to_string(),
                    file: file.to_string(),
                },
            );
        }
        BackupManifest {
            created: String::new(),
            fingerprint: String::new(),
            incremental: String::new(),
            patch: String::new(),
            slot: String::new(),
            gsi_fingerprint: String::new(),
            partitions,
        }
    };
    if !backup.join("vbmeta_a.img").exists() {
        anyhow::bail!(
            "{} has no vbmeta_a.img; cannot patch verification safely",
            backup.display()
        );
    }
    Ok(manifest)
}

pub fn verify_backup(rep: &dyn Reporter, backup: &Path, manifest: &BackupManifest) -> Result<()> {
    rep.log("verifying backup hashes");
    for (name, info) in &manifest.partitions {
        let path = backup.join(&info.file);
        if !path.exists() {
            anyhow::bail!("backup is missing {}", path.display());
        }
        if info.size > 0 && fs::metadata(&path)?.len() != info.size {
            anyhow::bail!("backup size mismatch: {}", path.display());
        }
        let digest = download::sha256_file(&path)?;
        if digest != info.sha256 {
            anyhow::bail!("backup hash mismatch: {}", path.display());
        }
        rep.log(&format!("  ok {name}"));
    }
    rep.log("backup verified");
    Ok(())
}
