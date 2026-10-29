//! Downloading, verifying and unpacking the GSI images.

use crate::consts::{Asset, LINEAGE, TREBLEDROID};
use crate::report::Reporter;
use crate::{Result, Settings};
use flate2::write::GzEncoder;
use flate2::Compression;
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use xz2::read::XzDecoder;

const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/153.0.0.0 Safari/537.36";

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut h = Sha256::new();
    let mut f = BufReader::new(File::open(path)?);
    let mut buf = [0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

fn sha1_file(path: &Path) -> Result<String> {
    let mut h = Sha1::new();
    let mut f = BufReader::new(File::open(path)?);
    let mut buf = [0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

fn md5_file(path: &Path) -> Result<String> {
    let mut h = Md5::new();
    let mut f = BufReader::new(File::open(path)?);
    let mut buf = [0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

pub fn verify_asset(path: &Path, asset: &Asset) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    if fs::metadata(path)?.len() != asset.size {
        return Ok(false);
    }
    if let Some(expected) = asset.sha256 {
        if sha256_file(path)? != expected {
            return Ok(false);
        }
    }
    if let Some(expected) = asset.sha1 {
        if sha1_file(path)? != expected {
            return Ok(false);
        }
    }
    if let Some(expected) = asset.md5 {
        if md5_file(path)? != expected {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn is_ext4(path: &Path) -> Result<bool> {
    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(0x438))?;
    let mut magic = [0u8; 2];
    f.read_exact(&mut magic)?;
    Ok(magic == [0x53, 0xef])
}

pub fn check_ext4(path: &Path) -> Result<()> {
    if !is_ext4(path)? {
        anyhow::bail!(
            "{} is not a raw ext4 system image. If it is sparse, convert it with simg2img first.",
            path.display()
        );
    }
    Ok(())
}

pub fn download_asset(rep: &dyn Reporter, asset: &Asset, dest: &Path) -> Result<PathBuf> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    if verify_asset(dest, asset)? {
        rep.log(&format!("using cached {}", dest.display()));
        return Ok(dest.to_path_buf());
    }
    let tmp = dest.with_extension(format!(
        "{}.part",
        dest.extension().and_then(|e| e.to_str()).unwrap_or("bin")
    ));
    let _ = fs::remove_file(&tmp);
    rep.log(&format!("downloading {}", asset.url));
    let agent = ureq::AgentBuilder::new()
        .redirects(10)
        .user_agent(UA)
        .build();
    let resp = agent
        .get(asset.url)
        .call()
        .map_err(|e| anyhow::anyhow!("download failed: {e}"))?;
    let total = resp
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok());
    let mut reader = resp.into_reader();
    let mut out = BufWriter::new(File::create(&tmp)?);
    let mut done: u64 = 0;
    let mut last_pct = 0u64;
    let mut buf = [0u8; 1024 * 1024];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| anyhow::anyhow!("download read failed: {e}"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        done += n as u64;
        rep.progress(done, total);
        if let Some(total) = total {
            if total > 0 {
                let pct = done * 100 / total;
                if pct >= last_pct + 10 {
                    rep.log(&format!("  {pct:3}%  {:.1} MiB", done as f64 / 1048576.0));
                    last_pct = pct;
                }
            }
        }
    }
    out.flush()?;
    drop(out);
    if !verify_asset(&tmp, asset)? {
        let _ = fs::remove_file(&tmp);
        anyhow::bail!("downloaded file failed verification: {}", dest.display());
    }
    fs::rename(&tmp, dest)?;
    rep.log(&format!("verified {}", dest.display()));
    Ok(dest.to_path_buf())
}

pub fn decompress_xz(rep: &dyn Reporter, src: &Path, dst: &Path) -> Result<()> {
    rep.log(&format!("decompressing {}", src.display()));
    let mut reader = XzDecoder::new(BufReader::new(File::open(src)?));
    let mut out = BufWriter::new(File::create(dst)?);
    std::io::copy(&mut reader, &mut out)?;
    out.flush()?;
    Ok(())
}

pub fn gzip_image(rep: &dyn Reporter, src: &Path, dst: &Path) -> Result<()> {
    rep.log(&format!("gzipping {}", src.display()));
    let mut encoder = GzEncoder::new(BufWriter::new(File::create(dst)?), Compression::default());
    std::io::copy(&mut BufReader::new(File::open(src)?), &mut encoder)?;
    let mut out = encoder.finish()?;
    out.flush()?;
    Ok(())
}

pub fn extract_7z(rep: &dyn Reporter, archive: &Path, outdir: &Path) -> Result<()> {
    rep.log(&format!("extracting {}", archive.display()));
    fs::create_dir_all(outdir)?;
    sevenz_rust::decompress_file(archive, outdir)
        .map_err(|e| anyhow::anyhow!("7z extraction failed: {e}"))?;
    Ok(())
}

pub fn ensure_trebledroid(rep: &dyn Reporter, settings: &Settings) -> Result<PathBuf> {
    let xz = download_asset(rep, &TREBLEDROID.asset, &settings.trebledroid_xz())?;
    let img = settings.trebledroid_img();
    if img.exists() && fs::metadata(&img)?.len() == TREBLEDROID.raw_size && is_ext4(&img)? {
        return Ok(img);
    }
    decompress_xz(rep, &xz, &img)?;
    if fs::metadata(&img)?.len() != TREBLEDROID.raw_size {
        anyhow::bail!("TrebleDroid image has an unexpected size");
    }
    check_ext4(&img)?;
    Ok(img)
}

pub fn ensure_trebledroid_gz(rep: &dyn Reporter, settings: &Settings) -> Result<PathBuf> {
    let img = ensure_trebledroid(rep, settings)?;
    let gz = settings.trebledroid_gz();
    if gz.exists() && fs::metadata(&gz)?.len() > 0 {
        return Ok(gz);
    }
    gzip_image(rep, &img, &gz)?;
    Ok(gz)
}

pub fn ensure_lineage(rep: &dyn Reporter, settings: &Settings, gapps: bool) -> Result<PathBuf> {
    let spec = if gapps {
        &crate::consts::LINEAGE_GAPPS
    } else {
        &LINEAGE
    };
    let img = settings.lineage_img(gapps);
    let size_ok =
        spec.img_size == 0 || fs::metadata(&img).map(|m| m.len()).unwrap_or(0) == spec.img_size;
    if img.exists() && size_ok && is_ext4(&img)? {
        return Ok(img);
    }
    let archive = download_asset(rep, &spec.asset, &settings.lineage_archive(gapps))?;
    let dir = settings.lineage_dir();
    extract_7z(rep, &archive, &dir)?;
    if img.exists() && is_ext4(&img)? {
        return Ok(img);
    }
    let mut found = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("img")
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n == spec.img_name)
                .unwrap_or(false)
        {
            found.push(path);
        }
    }
    if found.len() != 1 {
        anyhow::bail!("expected {} in the archive, found {found:?}", spec.img_name);
    }
    check_ext4(&found[0])?;
    Ok(found.remove(0))
}

pub fn ensure_custom_image(rep: &dyn Reporter, path: &Path) -> Result<PathBuf> {
    let path = path
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("cannot open {}: {e}", path.display()))?;
    check_ext4(&path)?;
    rep.log(&format!(
        "using custom GSI {} ({} bytes)",
        path.display(),
        fs::metadata(&path)?.len()
    ));
    Ok(path)
}
