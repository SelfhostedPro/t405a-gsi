# Technical notes: onn 11" Core (T405A)

Everything here was read from the device, from the public stock dump, or
verified by disassembling the stock bootloader. Dates are 2026-10-27/28.

## Device

| Item | Value |
| --- | --- |
| Retail model | onn 36018341 |
| Device / board | `onn11Core` / `T405A` |
| SoC | MediaTek MT8781V (Helio G99 tablet), platform MT6789 |
| Stock Android | 16, `BP2A.250605.031.A3`, patch 2026-04-05 (`1777011005`) |
| Kernel | 6.12.38-android16-5 GKI, 4K pages |
| Partitions | A/B, virtual A/B, dynamic `super`, `init_boot`, `vendor_boot`, `dtbo`, `vbmeta*`, UFS |
| Display | 1280x1840, 220 dpi, 60/90 Hz |
| RAM / storage | 6 GB / 128 GB, no SD slot (no mmc block device) |

Stock firmware is not published by Walmart/onn. The public
[el-vertedero/onn_onn11core_dump](https://github.com/el-vertedero/onn_onn11core_dump)
has file trees for the August build plus `lk_a`, `lk_b`, preloader and tee,
but no boot/super/vbmeta/nvram images. The tool's backup is therefore the
only complete restore source.

## Unlock: the LK `oem erase` overflow

`fastboot flashing unlock` / `oem unlock` are compiled out. `oem erase` is
gated by an RSA challenge, but its handler copies the partition argument into
a 16-byte stack buffer without a bounds check, before the permission check.

Payload (38 bytes, sent as a raw USB bulk transfer to `0e8d:201c`):

```
"oem erase " + 24 * "A" + f8 83 f8 50
```

The handler's frame:

```
file 0xcb68  (runtime VA 0x50f0c968)
  sub sp, sp, #0x40
  stp x29, x30, [sp, #0x10]   ; saved x29 at +0x10, saved x30 at +0x18
  ...
  buffer at sp (16 bytes), copy loop with no bound
```

The command dispatcher calls the handler through a function pointer
(`blr x8` at file 0x875c); the return address is file 0x8760 = runtime
`0xffff000050f08560`, exactly the "original LR" documented by the exploit
author. The 24 filler bytes overwrite the buffer and saved x29, and the final
four bytes partially overwrite the saved LR's low 32 bits:

```
0xffff000050f08560  ->  0xffff000050f883f8
```

The target is the lock-state path:

```
VA 0x50f883f8 (file 0x885f8)
  mov w0, #3
  bl  0x50f3455c          ; veneer -> 0x50f88228
```

`0x50f88228` stores w19 (=3) to `[0x5109248c]` and prints the lock state
(`secure`, `unlocked`, `yes`/`no`). That is the `seccfg` lock-state store.

### LK container and load address

The `lk` partition is an MTK container: a 0x200-byte header, then the LK
payload (size 0xd2780). The payload is linked at `0xffff000050f00000`:

```
runtime VA = 0x50f00000 + file_offset - 0x200
```

Check: the "oem erase is not allow" string at file 0xa2f07 is referenced from
the handler at file 0xcc0c with `adrp`+`add #0xd07` -> `0x50fa2d07`, which is
`0x50f00000 + 0xa2f07 - 0x200`. (The 0x200 offset is easy to miss; do not
compare file offsets directly with runtime addresses.)

### Verified builds

| Image | Version string | SHA-256 |
| --- | --- | --- |
| `lk_a.img` (April, untouched slot a) | `a16db94_202604232118` | `5274c2c5b137cb02ea0d2ccbcc0b3536996af885488bd9bca934f832f1487b42` |
| `lk_b.img` (August, OTA-updated slot b) | `a16db94_202608121002` | `4b33aa8820113a549da251375e73b9fd06653ed98b802bebf9bf700a6a0d3aec` |

Both have the same handler frame and the same target instruction
(`mov w0,#3; bl ...`); only nearby string addresses differ (5 bytes in the
handler, 1 byte at the target). The April unit was also unlocked on hardware.

**Do not add a new LK build without repeating the disassembly.** A wrong
partial overwrite jumps to arbitrary LK code; with no BROM/DA recovery a bad
write to LK/seccfg/GPT is unrecoverable.

### BROM / recovery

MediaTek BROM is V6/patched; DA authentication is enforced. mtkclient reaches
the preloader but DA upload fails with `DAA_SIG_VERIFY_FAILED (0x7024)`.
Holding both volume keys on a cold boot reaches LK fastboot, not BROM. A bad
write to preloader, lk, gz, seccfg or the GPT bricks the tablet.

The exploit writes `seccfg` directly, skipping the normal userdata wipe. The
FBE key becomes invalid, so the first boot lands in recovery with
`init_user0_failed`; a recovery factory reset completes the unlock. After any
GSI flash, do the same recovery reset; `fastboot -w` alone leaves MediaTek
metadata encryption uninitialized.

## Backup via DSU

The stock build is a `user` build (`ro.debuggable=0`), so it cannot `adb root`.
After unlock, DSU boots a userdebug GSI without touching `super`:

1. TrebleDroid Android 16 arm64 vanilla (`system-td-arm64-vanilla.img.xz`,
   userdebug, `ro.debuggable=1`), gzipped.
2. Push to `/storage/emulated/0/Download/` and start
   `com.android.dynsystem/.VerificationActivity` with
   `android.os.image.action.START_INSTALL`, `KEY_SYSTEM_SIZE` = uncompressed
   image size, `KEY_USERDATA_SIZE` = 8 GiB.
3. `gsi_tool enable` + reboot; on the unlocked bootloader the test-key GSI
   boots (orange).
4. `adb root`, then dump every `/dev/block/by-name` entry except `userdata`,
   `cache` and the whole-disk nodes `sda`/`sdb`/`sdc` (UFS LUNs, not
   partitions), checking each image against `blockdev --getsize64` and
   writing SHA-256 sums.
5. `gsi_tool wipe` (as root) + reboot returns to stock; the stock userdata is
   untouched.

The dump includes `super` (9,663,676,416 bytes, both slots' logical
partitions), boot/init_boot/vendor_boot/dtbo/vbmeta*, lk_a/b, nvram, nvdata,
nvcfg, persist, protect1/2, seccfg (post-unlock state), proinfo, frp, misc,
metadata and the rest. It does not include the original *locked* `seccfg`;
re-locking is out of scope.

## Flash geometry

- `super` is 9,663,676,416 bytes; groups `main_a`/`main_b` each have max
  9,661,579,264 bytes.
- Stock `system_a` was 1,222,750,208 bytes (0x48D27000). The LineageOS image is
  2,733,260,800 bytes, so `system_a` was resized to 3,000,000,000 bytes with
  `fastboot resize-logical-partition system_a 3000000000` in fastbootd.
- `fastboot flash system_a <img>` (fastboot splits it into sparse chunks).
- Physical partitions only flash from LK fastboot; logical partitions only
  from fastbootd (`adb reboot fastboot`).
- `fastboot --disable-verity --disable-verification flash vbmeta_a
  <stock-backup>/vbmeta_a.img` patches the stock vbmeta flags; the device is
  unlocked, so the invalidated signature is accepted.
- A recovery factory reset follows; `fastboot -w` alone is not enough.

## Return to stock

From LK fastboot, flash `boot_a`, `init_boot_a`, `vendor_boot_a`, `dtbo_a`,
`vbmeta_a`, `vbmeta_system_a`, `vbmeta_vendor_a` and `super` from the backup,
then a recovery factory reset. The bootloader stays unlocked/orange. Never
flash `lk`, preloader, `tee`, `gz`, `seccfg`, `protect*`, `nvram`, `nvdata`,
`nvcfg`, `persist`, `proinfo` or the GPT; those are the unrecoverable set.

## Quirks seen

- macOS `fastboot` fails on this LK (`usb_write failed e00002c0/e00002d8`).
  Linux (Bazzite) works; start the command before the device enumerates if
  the host xHCI controller stalls.
- `fastboot getvar all` under-reports some partitions (no seccfg/preloader
  entries); use `/dev/block/by-name` from root for the real list.
- The GSI reports the stock fingerprint (`onn/onn11Core/...`) even though
  `ro.lineage.version=23.2-...`; that is normal for this GSI/vendor mix.
- The stock OTA was available on 2026-10-28 and was deliberately not installed
  before unlocking; the April LK was the verified target. Updating later may
  change `version-bootloader` - re-check before any future unlock.
- No SD card slot (no mmc block device).

## Tool implementation (2026-10-28)

- The repo is a Rust workspace: `crates/core` (all device logic), `crates/cli`
  (headless), `crates/gui` (egui/eframe desktop wizard). The core mirrors the
  validated hardware flow, including the LK whitelist, backup verification and
  the safe restore set.
- The flash step takes **any raw ext4 arm64 GSI** (`--image` / the GUI
  checkbox); LineageOS 23.2 is only the default asset. The tool cannot check
  GSI/vendor compatibility; an Android 16 (202504 vendor) or newer GSI is the
  expected target.
- Build status: Linux release built on Bazzite with Rust 1.90 (CLI and GUI);
  macOS release built on the Mac with Rust 1.99; a Windows PowerShell build
  script is included.
- The Rust CLI passed `check`, `flash --dry-run`, `restore --dry-run`, and the
  unlock-on-unlocked / backup-on-LineageOS refusal guards against the real
  unit and a real stock backup. The destructive commands use the same core
  paths but were not re-run through the Rust app on hardware.
- The Rust core is the only implementation; its CLI passed the check/dry-run/
  refusal tests against the real unit and backup.

### GApps on a GSI device

LineageOS's official GApps package (MindTheGapps, <https://wiki.lineageos.org/gapps/>)
is installed from a custom recovery. This tablet has only stock recovery, which
accepts signed OTAs and not a GApps zip, so the tool offers the LineageOS 23.2
`GAPPS-EXT4` GSI instead (`--gapps` / the GUI checkbox). It is a normal raw
ext4 system image and uses the same resize/flash/vbmeta flow. Archive:
`LineageOS-23.2-20260524-GAPPS-EXT4-GSI.7z`, 1,167,859,370 bytes, SHA-1
`11e782a9207bcb9a6e163577eb5ac7ec577cfd99`, MD5
`8d5a503a0793c1498e23fde4528db62f`. The extracted image is a raw ext4
3,446,353,920-byte GSI.
