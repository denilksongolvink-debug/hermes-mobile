use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use tar::Archive;
use xz2::read::XzDecoder;

/// Extract a .tar.gz archive into dest, preserving symlinks and permissions.
pub fn extract_tar_gz<R: Read>(reader: R, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest).context("create dest")?;
    let gz = GzDecoder::new(reader);
    let mut archive = Archive::new(gz);
    archive.unpack(dest).context("unpack tar.gz")?;
    Ok(())
}

/// Extract a .tar.xz archive into dest.
pub fn extract_tar_xz<R: Read>(reader: R, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest).context("create dest")?;
    let xz = XzDecoder::new(reader);
    let mut archive = Archive::new(xz);
    archive.unpack(dest).context("unpack tar.xz")?;
    Ok(())
}

/// Detect by filename and extract.
pub fn extract_auto<R: Read>(reader: R, dest: &Path, filename: &str) -> Result<()> {
    if filename.ends_with(".tar.xz") || filename.ends_with(".txz") {
        extract_tar_xz(reader, dest)
    } else if filename.ends_with(".tar.gz") || filename.ends_with(".tgz") {
        extract_tar_gz(reader, dest)
    } else {
        // try gz
        extract_tar_gz(reader, dest)
    }
}

/// Download a URL to a file with blocking reqwest.
pub fn download(url: &str, dest: &Path) -> Result<()> {
    let resp = reqwest::blocking::get(url).context("http get")?;
    if !resp.status().is_success() {
        anyhow::bail!("bad status {}", resp.status());
    }
    let bytes = resp.bytes().context("read bytes")?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(dest, &bytes).context("write file")?;
    Ok(())
}

/// Verify sha256 of a file.
pub fn sha256_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    // sha2 crate may not be dep; implement via? We'll add dep.
    let data = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tempfile::tempdir;

    fn make_tar_gz() -> Vec<u8> {
        // build a small tar.gz in memory
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("hello.txt"), "hello world").unwrap();
        // symlink
        #[cfg(unix)]
        std::os::unix::fs::symlink("hello.txt", src.join("link.txt")).unwrap();

        let tar_path = dir.path().join("out.tar.gz");
        let file = File::create(&tar_path).unwrap();
        let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut ar = tar::Builder::new(enc);
        ar.append_dir_all(".", &src).unwrap();
        let enc = ar.into_inner().unwrap();
        enc.finish().unwrap();
        fs::read(&tar_path).unwrap()
    }

    #[test]
    fn extract_gz_roundtrip() {
        let data = make_tar_gz();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("dest");
        extract_tar_gz(Cursor::new(data), &dest).unwrap();
        let hello = fs::read_to_string(dest.join("hello.txt")).unwrap();
        assert_eq!(hello, "hello world");
    }

    #[test]
    fn extract_xz_roundtrip() {
        // build tar.xz
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.txt"), "abc").unwrap();
        let tar_path = dir.path().join("out.tar.xz");
        let file = File::create(&tar_path).unwrap();
        let enc = xz2::write::XzEncoder::new(file, 6);
        let mut ar = tar::Builder::new(enc);
        ar.append_dir_all(".", &src).unwrap();
        let enc = ar.into_inner().unwrap();
        enc.finish().unwrap();
        let data = fs::read(&tar_path).unwrap();

        let dest = dir.path().join("dest2");
        extract_tar_xz(Cursor::new(data), &dest).unwrap();
        assert_eq!(fs::read_to_string(dest.join("a.txt")).unwrap(), "abc");
    }
}
