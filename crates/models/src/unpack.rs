//! `tar.bz2` extraction with the top-level folder stripped and path-traversal protection.

use std::path::{Component, Path, PathBuf};

use crate::ModelError;

/// Extract `archive` into `dest` (created). The first path component of every entry is dropped
/// (`vits-piper-x/model.onnx` -> `dest/model.onnx`).
pub fn unpack_tar_bz2(archive: &Path, dest: &Path) -> Result<(), ModelError> {
    let f = std::fs::File::open(archive)?;
    let mut tar = tar::Archive::new(bzip2::read::BzDecoder::new(f));
    std::fs::create_dir_all(dest)?;
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let mut out = PathBuf::new();
        for (i, c) in path.components().enumerate() {
            match c {
                Component::Normal(p) if i > 0 => out.push(p),
                Component::Normal(_) | Component::CurDir => {}
                _ => return Err(ModelError::UnsafePath(path.display().to_string())),
            }
        }
        if out.as_os_str().is_empty() {
            continue;
        }
        let kind = entry.header().entry_type();
        if !(kind.is_file() || kind.is_dir()) {
            continue; // links and specials are ignored
        }
        let target = dest.join(&out);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&target)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    pub fn make_archive(path: &Path, files: &[(&str, &[u8])]) {
        let enc = bzip2::write::BzEncoder::new(
            std::fs::File::create(path).expect("create"),
            bzip2::Compression::fast(),
        );
        let mut b = tar::Builder::new(enc);
        for (name, data) in files {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, name, *data).expect("append");
        }
        b.into_inner()
            .expect("finish")
            .finish()
            .expect("bz")
            .flush()
            .expect("flush");
    }

    #[test]
    fn strips_the_top_folder() {
        let dir = tempfile::tempdir().expect("tmp");
        let a = dir.path().join("a.tar.bz2");
        make_archive(
            &a,
            &[
                ("top/model.onnx", b"M"),
                ("top/espeak-ng-data/phondata", b"P"),
            ],
        );
        let out = dir.path().join("out");
        unpack_tar_bz2(&a, &out).expect("unpack");
        assert_eq!(std::fs::read(out.join("model.onnx")).expect("m"), b"M");
        assert_eq!(
            std::fs::read(out.join("espeak-ng-data/phondata")).expect("p"),
            b"P"
        );
    }

    #[test]
    fn rejects_path_traversal() {
        let dir = tempfile::tempdir().expect("tmp");
        let a = dir.path().join("evil.tar.bz2");
        // tar::Builder refuses '..' itself, so write the header name bytes directly.
        let enc = bzip2::write::BzEncoder::new(
            std::fs::File::create(&a).expect("create"),
            bzip2::Compression::fast(),
        );
        let mut b = tar::Builder::new(enc);
        let mut h = tar::Header::new_gnu();
        h.set_size(1);
        h.set_mode(0o644);
        let name = b"top/../../escape.txt";
        h.as_old_mut().name[..name.len()].copy_from_slice(name);
        h.set_cksum();
        b.append(&h, &b"x"[..]).expect("append");
        b.into_inner().expect("finish").finish().expect("bz");
        let out = dir.path().join("out");
        let r = unpack_tar_bz2(&a, &out);
        assert!(matches!(r, Err(ModelError::UnsafePath(_))), "{r:?}");
        assert!(!dir.path().join("escape.txt").exists());
    }
}
