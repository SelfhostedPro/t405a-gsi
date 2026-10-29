# t405a-gsi

[![build](https://github.com/SelfhostedPro/t405a-gsi/actions/workflows/build.yml/badge.svg)](https://github.com/SelfhostedPro/t405a-gsi/actions/workflows/build.yml)

A step-by-step desktop installer for the **onn 11" Core (2026)** tablet: unlock
the bootloader, take a restorable stock backup, then flash **LineageOS 23.2**
(vanilla or with Google apps) or any raw arm64 GSI you like.

Device: onn 36018341, `onn11Core` / board `T405A`, MediaTek MT8781, Android 16,
A/B with virtual A/B and dynamic `super`.

> **Device-specific and destructive.** Only for the onn 11" Core 2026
> (`onn11Core` / `T405A`). The unlock is verified for two bootloader builds and
> refuses everything else. Read the safety section before running it.

## What it does

| Step | UI | CLI | Effect |
| --- | --- | --- | --- |
| Connect | *Check device* | `check` | Shows model, build, slot, lock state, supported bootloader |
| Unlock | *Unlock bootloader* | `unlock` | Sends the verified 38-byte LK payload once; wipes userdata; leaves orange/unlocked |
| Back up | *Back up stock* | `backup` | Boots a userdebug GSI via DSU, gets `adb root`, dumps every partition except userdata/cache/whole disks, with SHA-256SUMS |
| Flash | *Flash GSI* | `flash --backup DIR` | Resizes `system_a` if needed, flashes the image (add `--gapps` for Google apps), patches `vbmeta_a`, walks through the factory reset. `--force` re-flashes over an existing GSI |
| Restore | *Restore stock* | `restore --backup DIR` | Flashes stock boot/vbmeta/super back |

LineageOS 23.2 (MisterZtr, Android 16 QPR2, `VANILLA-EXT4`) is the default
image. The Flash step also accepts **any raw ext4 arm64 GSI** via the *Use a
custom raw ext4 GSI image* checkbox (CLI: `--image PATH`).

Use *Allow re-flash* (CLI: `--force`) to replace an existing LineageOS/GSI
install, for example to switch from vanilla to GApps.

For Google apps, tick *Include Google apps (GApps GSI)* (CLI: `--gapps`). That
uses the LineageOS 23.2 `GAPPS-EXT4` build. The official
[MindTheGapps](https://wiki.lineageos.org/gapps/) zip is installed from a
custom recovery, and this tablet only has stock recovery (signed OTAs only), so
the GApps-built GSI is the supported route here.

## Build

Requirements: Rust 1.90+ and Android platform-tools (`adb`, `fastboot`) on
`PATH`. The Linux build also needs a C compiler and `make` (libusb and liblzma
are built from source).

```sh
# Linux
./build/linux.sh
#   dist/linux/t405a-gsi-gui      # desktop app
#   dist/linux/t405a-gsi          # CLI

# macOS (from the repo root)
./build/macos-app.sh
#   dist/macos/t405a-gsi.app      # double-clickable app
#   dist/macos/t405a-gsi-cli

# Windows (PowerShell)
./build/windows.ps1
#   dist/windows/t405a-gsi-gui.exe
#   dist/windows/t405a-gsi.exe
```

Every push to `main` builds Linux, macOS and Windows binaries and publishes a
GitHub Release under the next patch tag (`v0.1.0`, `v0.1.1`, ...). Push a `v*`
tag manually to release that exact version. Assets are
`t405a-gsi-linux-x86_64.tar.gz`, `t405a-gsi-macos-arm64.zip` and
`t405a-gsi-windows-x86_64.zip` on the
[Releases page](https://github.com/SelfhostedPro/t405a-gsi/releases). The macOS
app is unsigned; right-click Open or run `xattr -dr com.apple.quarantine` if
Gatekeeper objects.

Development:

```sh
cargo run -p t405a-gui
cargo run -p t405a-gsi -- check
# Headless UI captures for review: writes 01-connect.png ... 05-restore.png
cargo run -p t405a-gui -- --screenshots /tmp/t405a-ui
```

Linux was built and tested first; macOS uses the same core (build on a Mac).
Windows builds with the same source; the raw-USB unlock is the only
driver-sensitive step (see below).

## Using the app

1. **Connect & check** — enable Developer options and USB debugging; press
   *Check device*. Optionally *Download / verify images*.
2. **Unlock bootloader** — read the warning, tick the acknowledgement, type
   `UNLOCK T405A`, press *Unlock*. When the tablet reaches recovery:
   Power + Volume Up, *Wipe data/factory reset*, *Reboot system now*.
3. **Back up stock** — needs about 12 GB free; the app installs a userdebug
   GSI through DSU, roots it, dumps the partitions to the work directory.
4. **Flash GSI** — pick the stock backup (needed for `vbmeta_a.img`), optionally
   a custom `.img`, press *Flash GSI*, then do the recovery factory reset.
5. **Restore stock** — flashes the backup's boot/vbmeta/super images and asks
   for a recovery factory reset.

The work directory defaults to the platform data directory
(`~/.local/share/t405a-gsi` on Linux).

## CLI

```sh
t405a-gsi check
t405a-gsi download all
t405a-gsi download lineage-gapps  # optional GApps image
t405a-gsi unlock                 # type UNLOCK T405A
t405a-gsi backup --out ~/stock-backup/t405a
t405a-gsi flash --backup ~/stock-backup/t405a
t405a-gsi flash --backup ~/stock-backup/t405a --gapps          # LineageOS + Google apps
t405a-gsi flash --backup ~/stock-backup/t405a --gapps --force  # replace an existing GSI
t405a-gsi flash --backup ~/stock-backup/t405a --image /path/to/other-gsi.img
t405a-gsi flash --backup ~/stock-backup/t405a --dry-run
t405a-gsi restore --backup ~/stock-backup/t405a --dry-run
```

## Google apps (GApps)

The tool's GApps option flashes the LineageOS 23.2 GSI built with Google apps;
nothing is patched after boot and no custom recovery is needed. The official
LineageOS route ([MindTheGapps](https://wiki.lineageos.org/gapps/)) installs a
zip from a custom recovery, which this device does not have. The GApps build is
still an unofficial LineageOS GSI: Play certification and some Google features
may need a certification/spoofing fix, and Google apps are not part of
LineageOS itself.

## Custom GSIs

Any **raw ext4 arm64** GSI that is compatible with an Android 16 vendor
(API 202504) should work: the flow uses the stock kernel/vendor and only
replaces the `system` logical partition. Sparse images must be converted with
`simg2img` first. The tool cannot validate GSI/vendor compatibility; choose an
Android 16 or newer GSI for a 202504 vendor.

## Pinned assets

- TrebleDroid `system-td-arm64-vanilla.img.xz` (DSU root GSI), SHA-256
  `d06c66e45c6d1e8d61b45d1261a65cb7ccf2e7946739602011e500a12ae55ad6`
- MisterZtr LineageOS 23.2 `VANILLA-EXT4-GSI.7z`, SHA-1
  `300f0dc37d21339cc6deff6b5aead53ac7ccba9c`, MD5
  `341fd678e507d8bfe790c4910d443bea`
- MisterZtr LineageOS 23.2 `GAPPS-EXT4-GSI.7z`, 1,167,859,370 bytes, SHA-1
  `11e782a9207bcb9a6e163577eb5ac7ec577cfd99`, MD5
  `8d5a503a0793c1498e23fde4528db62f`

## Safety

- **The unlock is irreversible in practice.** It writes lock state 3 into
  `seccfg`; BROM is V6/patched and DA authentication is enforced, so a bad
  LK/seccfg/GPT write bricks the tablet. There is no EDL/BROM recovery path.
- The payload is sent **once**; the tool refuses to resend it.
- Never `fastboot flashing lock` / `oem lock` with modified images. The tool
  never re-locks.
- The tool never writes `lk`, preloader, `tee`, `gz`, `seccfg`, `protect*`,
  `nvram`, `nvdata`, `nvcfg`, `persist`, `proinfo` or the GPT. Restore is
  limited to boot/init_boot/vendor_boot/dtbo/vbmeta*/super.
- `flash` refuses without a backup containing a verified `vbmeta_a.img`.
- Userdata is wiped by the unlock reset and again after the GSI flash. Back up
  anything important first.
- Keep the stock backup out of git and off the network; it contains
  calibration/serial data.

## Host USB notes

- **Linux** is the tested path (Bazzite and the dev VM).
- **Windows:** the raw-USB unlock needs libusb/WinUSB and a driver bound to
  `0e8d:201c`; binding a driver can break `fastboot.exe`. Easiest is **WSL2 +
  usbipd-win**, then run the Linux binary.
- **macOS:** the app builds and runs, but this MediaTek LK is known to fail
  `fastboot`/bulk transfers on some xHCI controllers
  (`usb_write failed e00002c0/e00002d8`). Use a Linux host or a USB 2.0 hub if
  you hit that.
- adb/fastboot must be on `PATH`; on Windows they are `adb.exe`/`fastboot.exe`.

## Verified result (2026-10-28)

The procedure was run end to end on an April-2026 unit
(`BP2A.250605.031.A3`, LK `T405A_a16db94_202604232118`):

- The unlock payload returned the expected `FAILoem erase is not allow! …`;
  recovery showed `init_user0_failed`; the factory reset completed the unlock.
- A DSU userdebug TrebleDroid Android 16 GSI gave `adb root`; all 63 by-name
  partitions (except userdata/cache/whole disks) were dumped and SHA-256
  verified.
- LineageOS 23.2 booted after resizing `system_a` to 3 GB and patching
  `vbmeta_a`.
- The GApps archive was SHA-1/MD5-verified; its raw ext4 image is 3,446,353,920
  bytes and contains `GmsCore`. It was then flashed over the vanilla install
  with `--force`: `ro.lineage.version=23.2-20260524-GAPPS-EXT4-GSI`, with
  Google Play services, GSF and Play Store (`com.android.vending`) present;
  Wi-Fi, both cameras, speaker and battery reporting checked. Use `--force`
  to re-flash an existing LineageOS/GSI install.
- Working: boot, touch, display 1280x1840 (60/90 Hz), rotation, Wi-Fi
  (5 GHz/WPA3), Bluetooth, speakers/mic, both cameras, charging/battery,
  sleep/wake, brightness, sensors, USB adb. No SD card slot.

The Rust CLI then passed `check`, `flash --dry-run`, `restore --dry-run`,
plus the unlock-on-unlocked and backup-on-LineageOS refusals against the same
unit and a real backup. The GUI was compiled for macOS and Linux; the
destructive commands issue the same verified core paths.

## Layout

```
crates/core   device checks, adb/fastboot, raw USB, downloads, hashing,
              DSU backup, flash/restore
crates/cli    headless CLI (clap)
crates/gui    egui/eframe desktop app
build/        per-OS release scripts
docs/         device and bootloader technical notes
```

## Credits

- Unlock vulnerability, addresses and payload: drakeerv's
  [Nura Wiki](https://wiki.nura.eco/wiki/Onn_11_Core_Tablet_%28onn-t405a%29),
  packaged by [phildabill](https://github.com/phildabill/onn-t405a-Bootloader-unlock).
- Stock partition reference: [el-vertedero/onn_onn11core_dump](https://github.com/el-vertedero/onn_onn11core_dump).
- GSIs: [TrebleDroid](https://github.com/TrebleDroid/treble_experimentations),
  [MisterZtr LineageOS_gsi](https://github.com/MisterZtr/LineageOS_gsi).

## License

MIT; see [LICENSE](LICENSE). Documentation and GSI images remain under their
authors' licenses.
