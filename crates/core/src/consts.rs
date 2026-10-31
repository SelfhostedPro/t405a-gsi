//! Device, asset and payload constants.

pub const MODEL: &str = "onn11Core";
pub const BOARD: &str = "T405A";
pub const STOCK_BUILD_ID: &str = "BP2A.250605.031.A3";

/// LK builds the unlock payload was verified against.
pub const SUPPORTED_LK: &[(&str, &str)] = &[
    ("T405A_a16db94_202604232118", "April 2026 stock"),
    ("T405A_a16db94_202608121002", "August 2026 stock"),
];

/// "oem erase " + 24 filler + low 4 bytes of 0xffff000050f883f8.
pub const PAYLOAD: [u8; 38] = make_payload();

const fn make_payload() -> [u8; 38] {
    let mut p = [0u8; 38];
    let prefix = b"oem erase ";
    let mut i = 0;
    while i < prefix.len() {
        p[i] = prefix[i];
        i += 1;
    }
    while i < 34 {
        p[i] = b'A';
        i += 1;
    }
    p[34] = 0xf8;
    p[35] = 0x83;
    p[36] = 0xf8;
    p[37] = 0x50;
    p
}

pub struct Asset {
    pub url: &'static str,
    pub file_name: &'static str,
    pub sha256: Option<&'static str>,
    pub sha1: Option<&'static str>,
    pub md5: Option<&'static str>,
    pub size: u64,
}

pub struct TrebleDroid {
    pub asset: Asset,
    pub xz_name: &'static str,
    pub img_name: &'static str,
    pub raw_size: u64,
}

pub const TREBLEDROID: TrebleDroid = TrebleDroid {
    asset: Asset {
        url: "https://github.com/TrebleDroid/treble_experimentations/releases/download/ci-20250617/system-td-arm64-vanilla.img.xz",
        file_name: "system-td-arm64-vanilla.img.xz",
        sha256: Some("d06c66e45c6d1e8d61b45d1261a65cb7ccf2e7946739602011e500a12ae55ad6"),
        sha1: None,
        md5: None,
        size: 774_420_196,
    },
    xz_name: "system-td-arm64-vanilla.img.xz",
    img_name: "system-td-arm64-vanilla.img",
    raw_size: 2_211_401_728,
};

pub struct Lineage {
    pub asset: Asset,
    pub archive_name: &'static str,
    pub img_name: &'static str,
    pub img_size: u64,
}

pub const LINEAGE: Lineage = Lineage {
    asset: Asset {
        url: "https://sourceforge.net/projects/misterztr-gsi/files/LineageOS/Android%2016/LineageOS-23.2-20260524-VANILLA-EXT4-GSI.7z/download",
        file_name: "LineageOS-23.2-20260524-VANILLA-EXT4-GSI.7z",
        sha256: None,
        sha1: Some("300f0dc37d21339cc6deff6b5aead53ac7ccba9c"),
        md5: Some("341fd678e507d8bfe790c4910d443bea"),
        size: 922_022_276,
    },
    archive_name: "LineageOS-23.2-20260524-VANILLA-EXT4-GSI.7z",
    img_name: "LineageOS-23.2-20260524-VANILLA-EXT4-GSI.img",
    img_size: 2_733_260_800,
};

/// Same LineageOS 23.2 GSI but built with Google apps (GApps).
///
/// This is the practical "add GApps" route for a GSI device: the official
/// MindTheGapps zip is installed from a custom recovery, which this tablet
/// does not have (stock recovery only accepts signed OTAs).
pub const LINEAGE_GAPPS: Lineage = Lineage {
    asset: Asset {
        url: "https://sourceforge.net/projects/misterztr-gsi/files/LineageOS/Android%2016/LineageOS-23.2-20260524-GAPPS-EXT4-GSI.7z/download",
        file_name: "LineageOS-23.2-20260524-GAPPS-EXT4-GSI.7z",
        sha256: None,
        sha1: Some("11e782a9207bcb9a6e163577eb5ac7ec577cfd99"),
        md5: Some("8d5a503a0793c1498e23fde4528db62f"),
        size: 1_167_859_370,
    },
    archive_name: "LineageOS-23.2-20260524-GAPPS-EXT4-GSI.7z",
    img_name: "LineageOS-23.2-20260524-GAPPS-EXT4-GSI.img",
    img_size: 3_446_353_920,
};

pub const DSU_USERDATA: u64 = 8 * 1024 * 1024 * 1024;

pub const SKIP_PARTITIONS: &[&str] = &["userdata", "cache", "sda", "sdb", "sdc"];

pub const RESTORE_PHYSICAL_A: &[&str] = &[
    "boot_a",
    "init_boot_a",
    "vendor_boot_a",
    "dtbo_a",
    "vbmeta_a",
    "vbmeta_system_a",
    "vbmeta_vendor_a",
];

/// Official Magisk release used for the root flow.
pub const MAGISK: Asset = Asset {
    url: "https://github.com/topjohnwu/Magisk/releases/download/v31.1/Magisk-v31.1.apk",
    file_name: "Magisk-v31.1.apk",
    sha256: Some("89cefac0a318afa39fb3880c45c87b036c05bc1ebf02a7a715b7f8d7c26ac6e7"),
    sha1: None,
    md5: None,
    size: 10_679_283,
};

/// PlayIntegrityFork (osm0sis) module for Play Integrity / certified fingerprints.
pub const PLAY_INTEGRITY_FIX: Asset = Asset {
    url: "https://github.com/osm0sis/PlayIntegrityFork/releases/download/v18/PlayIntegrityFork-v18.zip",
    file_name: "PlayIntegrityFork-v18.zip",
    sha256: Some("625ebe65631f761e1521009f10a205d74255adf2e35732923d65106643cad9a6"),
    sha1: None,
    md5: None,
    size: 227_895,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_is_the_verified_38_bytes() {
        assert_eq!(PAYLOAD.len(), 38);
        assert_eq!(&PAYLOAD[..10], b"oem erase ");
        assert_eq!(&PAYLOAD[10..34], &[b'A'; 24]);
        assert_eq!(&PAYLOAD[34..], &[0xf8, 0x83, 0xf8, 0x50]);
    }

    #[test]
    fn supported_lk_list_is_exact() {
        assert_eq!(SUPPORTED_LK.len(), 2);
        assert_eq!(SUPPORTED_LK[0].0, "T405A_a16db94_202604232118");
        assert_eq!(SUPPORTED_LK[1].0, "T405A_a16db94_202608121002");
    }
}
