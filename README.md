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
| Root & Play | *Install Magisk (root)* | `root --backup DIR`, `playfix`, `playid` | Patches the stock `init_boot_<slot>` with Magisk and flashes it, installs PlayIntegrityFork, reads the GSF ID for Play Store certification |
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
# Headless UI captures for review: writes 01-connect.png ... 06-restore.png
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
5. **Root & Play** (optional) — install Magisk by patching the stock
   `init_boot_<slot>` from the backup, install PlayIntegrityFork, and read the
   GSF ID for Play Store certification.
6. **Restore stock** — flashes the backup's boot/vbmeta/super images and asks
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
t405a-gsi root --backup ~/stock-backup/t405a                 # Magisk root
t405a-gsi root --backup ~/stock-backup/t405a --patched IMG   # image patched earlier
t405a-gsi playfix                                            # PlayIntegrityFork
t405a-gsi playid                                             # GSF ID for certification
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

## Play certification and Play Integrity

After a GApps GSI boots, the Play Store may say **Device is not certified**.
That is an ID registration issue, not a ROM bug:

1. Run `t405a-gsi playid` (or *Read GSF ID* in the app). It runs Google's
   uncertified-page command —
   `sqlite3 /data/user/$(cmd activity get-current-user)/*/*/gservices.db "select * from main where name = \"android_id\";"`
   — as root through Magisk's Shell and shows the raw output plus the ID in
   hex and decimal, with copy buttons. If the shell cannot read
   `gservices.db`, install root first and grant Shell in Magisk's Superuser
   tab.
2. Register the ID at <https://www.google.com/android/uncertified/> with the
   Google account used on the tablet. The page accepts either form.
3. Clear Play Store data (*Settings → Apps → Play Store → Storage → Clear
   data*), reboot, and allow Google some time to sync.

Magisk root and Play Integrity (Wallet, some banking apps) are separate:

```sh
t405a-gsi root --backup ~/stock-backup/t405a
t405a-gsi playfix
```

`root` downloads the official Magisk v31.1 APK, installs it, pushes the stock
`init_boot_<slot>.img` from the backup, and waits while you patch it in the app
(*Magisk → Install → Patch firmware / image → Select from storage*). It then
flashes `init_boot_<slot>` in LK fastboot and checks that Magisk is running.
Use `--patched PATH` for an image patched earlier.

Magisk v31.1 removed root prompts and app hiding: grant root to **Shell** from
the Superuser tab, and there is nothing to hide. PlayIntegrityFork v18 is a
Zygisk module and ships **no fingerprint**; run its Action button
(`autopif4`) or add a `custom.pif.prop`, or it will not fix DEVICE integrity.

Do not use `adb root` on this GSI: it restarts adbd and the device then stops
answering adb until it is rebooted (seen on the 2026-10-30 GApps build).
Magisk is the supported root path. `restore` puts the stock `init_boot_<slot>`
back and removes Magisk.

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
- Magisk `v31.1` APK, 10,679,283 bytes, SHA-256
  `89cefac0a318afa39fb3880c45c87b036c05bc1ebf02a7a715b7f8d7c26ac6e7`
- PlayIntegrityFork `v18` zip, 227,895 bytes, SHA-256
  `625ebe65631f761e1521009f10a205d74255adf2e35732923d65106643cad9a6`

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
- The root step writes `init_boot_<slot>` only, using the stock image from
  the backup; `restore` puts the unpatched one back.
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
