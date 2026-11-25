use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::fs::{self, File};
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use tar::Archive;
use xz2::read::XzDecoder;

fn detect_root_prefix_gz(data: &[u8]) -> Option<String> {
    let gz = GzDecoder::new(Cursor::new(data));
    let mut archive = Archive::new(gz);
    detect_root_prefix_archive(&mut archive)
}

fn detect_root_prefix_xz(data: &[u8]) -> Option<String> {
    let xz = XzDecoder::new(Cursor::new(data));
    let mut archive = Archive::new(xz);
    detect_root_prefix_archive(&mut archive)
}

fn detect_root_prefix_archive<R: Read>(archive: &mut Archive<R>) -> Option<String> {
    let mut root: Option<String> = None;
    let mut all_under_root = true;
    let mut seen = false;
    for entry in archive.entries().ok()? {
        let entry = entry.ok()?;
        let path = entry.path().ok()?;
        let mut comps = path.components();
        match comps.next() {
            Some(Component::Normal(first)) => {
                let first = first.to_string_lossy().to_string();
                if !seen {
                    root = Some(first);
                    seen = true;
                } else if root.as_deref() != Some(first.as_str()) {
                    all_under_root = false;
                }
            }
            _ => {
                all_under_root = false;
            }
        }
        if !all_under_root {
            break;
        }
    }
    if all_under_root && seen {
        root
    } else {
        None
    }
}

fn extract_to_temp_and_normalize<R: Read, F>(reader: R, dest: &Path, make_archive: F) -> Result<()>
where
    F: FnOnce(R) -> Archive<R>,
{
    fs::create_dir_all(dest).context("create dest")?;
    let tmp = dest.join(".extract_tmp");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).context("create tmp")?;
    let mut archive = make_archive(reader);
    archive.unpack(&tmp).context("unpack")?;
    // normalize
    let entries: Vec<_> = fs::read_dir(&tmp)?.filter_map(|e| e.ok()).collect();
    let standard = ["bin","boot","dev","etc","home","lib","lib64","media","mnt","opt","proc","root","run","sbin","srv","sys","tmp","usr","var"];
    let strip = entries.len() == 1 && entries[0].path().is_dir() && {
        let name = entries[0].file_name().to_string_lossy().to_string();
        !standard.contains(&name.as_str())
    };
    if strip {
        let single = entries[0].path();
        for child in fs::read_dir(&single)? {
            let child = child?;
            let name = child.file_name();
            let target = dest.join(&name);
            if target.exists() {
                let _ = fs::remove_file(&target);
                let _ = fs::remove_dir_all(&target);
            }
            fs::rename(child.path(), &target).context("rename")?;
        }
    } else {
        for child in fs::read_dir(&tmp)? {
            let child = child?;
            let name = child.file_name();
            let target = dest.join(&name);
            if target.exists() {
                let _ = fs::remove_file(&target);
                let _ = fs::remove_dir_all(&target);
            }
            fs::rename(child.path(), &target).context("rename")?;
        }
    }
    let _ = fs::remove_dir_all(&tmp);
    // remove empty single dir if left
    Ok(())
}

/// Extract a .tar.gz archive into dest, normalizing a single top-level dir.
pub fn extract_tar_gz<R: Read>(mut reader: R, dest: &Path) -> Result<()> {
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).context("read")?;
    let gz = GzDecoder::new(Cursor::new(&buf));
    extract_to_temp_and_normalize(gz, dest, |r| Archive::new(r))
}

/// Extract a .tar.xz archive into dest, normalizing a single top-level dir.
pub fn extract_tar_xz<R: Read>(mut reader: R, dest: &Path) -> Result<()> {
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).context("read")?;
    let xz = XzDecoder::new(Cursor::new(&buf));
    extract_to_temp_and_normalize(xz, dest, |r| Archive::new(r))
}

pub fn extract_auto<R: Read>(mut reader: R, dest: &Path, filename: &str) -> Result<()> {
    // buffer reader to detect; but reader may be consumed. Read all.
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).context("read")?;
    if filename.ends_with(".tar.xz") || filename.ends_with(".txz") {
        let xz = XzDecoder::new(Cursor::new(&buf));
        extract_to_temp_and_normalize(xz, dest, |r| Archive::new(r))
    } else {
        let gz = GzDecoder::new(Cursor::new(&buf));
        extract_to_temp_and_normalize(gz, dest, |r| Archive::new(r))
    }
}

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

pub fn sha256_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let data = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use std::io::Cursor;
    use tempfile::tempdir;
    use xz2::write::XzEncoder;

    fn make_tar_gz(rooted: bool) -> Vec<u8> {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(src.join("bin")).unwrap();
        fs::write(src.join("bin").join("sh"), "shell").unwrap();
        let tar_path = dir.path().join("out.tar.gz");
        let file = File::create(&tar_path).unwrap();
        let enc = GzEncoder::new(file, flate2::Compression::default());
        let mut ar = tar::Builder::new(enc);
        if rooted {
            // wrap in debian-arm64/
            let root = dir.path().join("root");
            fs::create_dir_all(root.join("bin")).unwrap();
            fs::write(root.join("bin").join("sh"), "shell").unwrap();
            ar.append_dir_all("debian-arm64", &root).unwrap();
        } else {
            ar.append_dir_all(".", &src).unwrap();
        }
        let enc = ar.into_inner().unwrap();
        enc.finish().unwrap();
        fs::read(&tar_path).unwrap()
    }

    #[test]
    fn extract_gz_flat() {
        let data = make_tar_gz(false);
        let dir = tempdir().unwrap();
        let dest = dir.path().join("dest");
        extract_tar_gz(Cursor::new(data), &dest).unwrap();
        assert!(dest.join("bin").join("sh").exists());
    }

    #[test]
    fn extract_gz_rooted_normalizes() {
        let data = make_tar_gz(true);
        let dir = tempdir().unwrap();
        let dest = dir.path().join("dest");
        extract_tar_gz(Cursor::new(data), &dest).unwrap();
        assert!(dest.join("bin").join("sh").exists());
        assert!(!dest.join("debian-arm64").exists());
    }

    #[test]
    fn extract_xz_roundtrip() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(src.join("bin")).unwrap();
        fs::write(src.join("bin").join("sh"), "abc").unwrap();
        let tar_path = dir.path().join("out.tar.xz");
        let file = File::create(&tar_path).unwrap();
        let enc = XzEncoder::new(file, 6);
        let mut ar = tar::Builder::new(enc);
        ar.append_dir_all(".", &src).unwrap();
        let enc = ar.into_inner().unwrap();
        enc.finish().unwrap();
        let data = fs::read(&tar_path).unwrap();

        let dest = dir.path().join("dest2");
        extract_tar_xz(Cursor::new(data), &dest).unwrap();
        assert_eq!(fs::read_to_string(dest.join("bin").join("sh")).unwrap(), "abc");
    }
}
