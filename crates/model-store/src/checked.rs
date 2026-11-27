//! The check of the files of one pack on disk, and the SHA-256 hasher.
//!
//! A pack lives in `<model folder>/<pack id>/`. The model folder itself may be
//! reached through a symlink; nothing below it may be one. The check reads each
//! file through one handle, so the owner, the length and the digest belong to
//! the same file. It does not keep the handle: a caller that opens the path
//! again later is not protected against a change made after this check. The
//! walk to a file is not atomic either: a process that replaces a folder below
//! the model folder with a link while the check runs can pass it. M1-42 owns
//! both limits.

use crate::{FileDigest, Hasher, ModelFile, Pack};
use sha2::{Digest, Sha256};
use std::fs::{File, Metadata, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

// The macOS values of two open flags, written here so that no dependency is
// added.
/// Do not follow a link at the last part of the path.
#[cfg(target_os = "macos")]
const O_NOFOLLOW: i32 = 0x0100;
/// Do not wait for the other end of a pipe.
#[cfg(target_os = "macos")]
const O_NONBLOCK: i32 = 0x0004;
/// Flags of the open of a model file.
#[cfg(target_os = "macos")]
const OPEN_FLAGS: i32 = O_NOFOLLOW | O_NONBLOCK;
/// Not set on other systems: the app runs on macOS only.
#[cfg(not(target_os = "macos"))]
const OPEN_FLAGS: i32 = 0;

/// What a check found about the files of one pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackCheck {
    /// Every file is there and matches the table.
    Ok,
    /// At least one file is absent and no file failed.
    Missing,
    /// A file is wrong, cannot be read, is behind a symlink or belongs to
    /// another user.
    Failed,
}

/// The SHA-256 of an open model file.
#[derive(Debug, Clone, Copy, Default)]
pub struct Sha256Hasher;

impl Hasher for Sha256Hasher {
    fn digest(&self, file: &mut File, cap_bytes: u64) -> io::Result<FileDigest> {
        let len = file.metadata()?.len();
        if len > cap_bytes {
            return Ok(FileDigest::OverCap { len });
        }
        file.seek(SeekFrom::Start(0))?;
        let mut sha = Sha256::new();
        let hashed = io::copy(&mut file.take(cap_bytes), &mut sha)?;
        let sha256_hex = sha.finalize().iter().map(|b| format!("{b:02x}")).collect();
        Ok(FileDigest::Hashed {
            len: hashed,
            sha256_hex,
        })
    }
}

/// Open `path` and require that the handle is the regular file that `before`
/// describes, so a link, a pipe or another file put there after the look is
/// refused, and the open does not wait.
fn open_same(path: &Path, before: &Metadata) -> io::Result<File> {
    let handle = OpenOptions::new()
        .read(true)
        .custom_flags(OPEN_FLAGS)
        .open(path)?;
    let now = handle.metadata()?;
    if !now.is_file() || (now.dev(), now.ino()) != (before.dev(), before.ino()) {
        return Err(io::Error::other("the file changed before it was opened"));
    }
    Ok(handle)
}

/// Walk from `root` to `relative`, one part at a time, without following a link.
/// Returns the full path and what the last part is.
fn look(root: &Path, relative: &Path) -> Result<(PathBuf, Metadata), PackCheck> {
    let mut path = root.to_path_buf();
    let mut last = None;
    for part in relative.components() {
        let Component::Normal(name) = part else {
            return Err(PackCheck::Failed);
        };
        path.push(name);
        let seen = std::fs::symlink_metadata(&path).map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => PackCheck::Missing,
            _ => PackCheck::Failed,
        })?;
        if seen.file_type().is_symlink() {
            return Err(PackCheck::Failed);
        }
        last = Some(seen);
    }
    match last {
        // Only a regular file goes on to the open.
        Some(seen) if seen.is_file() => Ok((path, seen)),
        _ => Err(PackCheck::Failed),
    }
}

fn check_file(
    root: &Path,
    pack_id: &str,
    file: &ModelFile,
    owner_uid: u32,
    hasher: &dyn Hasher,
) -> PackCheck {
    let (path, before) = match look(root, &Path::new(pack_id).join(file.rel_path)) {
        Ok(found) => found,
        Err(result) => return result,
    };
    let Ok(mut handle) = open_same(&path, &before) else {
        return PackCheck::Failed;
    };
    let owned_and_sized = handle
        .metadata()
        .is_ok_and(|now| now.uid() == owner_uid && now.len() == file.size_bytes);
    if !owned_and_sized {
        return PackCheck::Failed;
    }
    match hasher.digest(&mut handle, file.size_bytes) {
        Ok(FileDigest::Hashed { len, sha256_hex })
            if len == file.size_bytes && sha256_hex.eq_ignore_ascii_case(file.sha256_hex) =>
        {
            PackCheck::Ok
        }
        _ => PackCheck::Failed,
    }
}

/// Check every file of `pack` under `root/<pack id>/`. `owner_uid` is the user
/// that must own each file.
pub fn check_pack(root: &Path, pack: &Pack, owner_uid: u32, hasher: &dyn Hasher) -> PackCheck {
    let mut missing = false;
    for file in pack.files {
        match check_file(root, pack.id, file, owner_uid, hasher) {
            PackCheck::Ok => {}
            PackCheck::Missing => missing = true,
            PackCheck::Failed => return PackCheck::Failed,
        }
    }
    if missing {
        PackCheck::Missing
    } else {
        PackCheck::Ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// SHA-256 of the bytes `abc` (FIPS 180-2 example).
    const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    /// SHA-256 of the bytes `abd`.
    const ABD_SHA256: &str = "a52d159f262b2c6ddb724a61840befc36eb30c88877a4030b65cbe86298449c9";

    static FILES: [ModelFile; 2] = [
        ModelFile {
            rel_path: "a.bin",
            url: "https://models.example.com/a.bin",
            sha256_hex: ABC_SHA256,
            size_bytes: 3,
        },
        ModelFile {
            rel_path: "sub/b.bin",
            url: "https://models.example.com/b.bin",
            sha256_hex: ABD_SHA256,
            size_bytes: 3,
        },
    ];
    static PACK: Pack = Pack {
        id: "demo",
        files: &FILES,
    };

    /// A model folder with the two files of `PACK`, removed on drop.
    struct Folder(PathBuf);

    impl Folder {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let root =
                std::env::temp_dir().join(format!("model-store-pack-{}-{n}", std::process::id()));
            std::fs::create_dir_all(root.join("demo/sub")).unwrap();
            std::fs::write(root.join("demo/a.bin"), b"abc").unwrap();
            std::fs::write(root.join("demo/sub/b.bin"), b"abd").unwrap();
            Self(root)
        }

        fn at(&self, relative: &str) -> PathBuf {
            self.0.join(relative)
        }

        /// The owner of the files this test made.
        fn uid(&self) -> u32 {
            std::fs::metadata(&self.0).unwrap().uid()
        }

        fn check(&self) -> PackCheck {
            check_pack(&self.0, &PACK, self.uid(), &Sha256Hasher)
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Counts the files that were hashed.
    struct Counting(Cell<u32>);

    impl Hasher for Counting {
        fn digest(&self, file: &mut File, cap_bytes: u64) -> io::Result<FileDigest> {
            self.0.set(self.0.get() + 1);
            Sha256Hasher.digest(file, cap_bytes)
        }
    }

    fn digest_of(content: &[u8], cap_bytes: u64) -> FileDigest {
        let folder = Folder::new();
        std::fs::write(folder.at("demo/a.bin"), content).unwrap();
        let mut file = File::open(folder.at("demo/a.bin")).unwrap();
        Sha256Hasher.digest(&mut file, cap_bytes).unwrap()
    }

    #[test]
    fn the_hasher_gives_the_sha256_and_the_length_of_the_file() {
        assert_eq!(
            digest_of(b"abc", 3),
            FileDigest::Hashed {
                len: 3,
                sha256_hex: ABC_SHA256.to_string()
            }
        );
    }

    #[test]
    fn the_hasher_reads_from_the_start_of_an_already_read_handle() {
        let folder = Folder::new();
        let mut file = File::open(folder.at("demo/a.bin")).unwrap();
        file.read_exact(&mut [0u8; 2]).unwrap();
        assert_eq!(
            Sha256Hasher.digest(&mut file, 3).unwrap(),
            FileDigest::Hashed {
                len: 3,
                sha256_hex: ABC_SHA256.to_string()
            }
        );
    }

    #[test]
    fn the_hasher_reports_a_file_over_the_cap_without_a_digest() {
        assert_eq!(digest_of(b"abcd", 3), FileDigest::OverCap { len: 4 });
    }

    #[test]
    fn matching_files_pass_and_each_file_is_hashed_once() {
        let folder = Folder::new();
        let hasher = Counting(Cell::new(0));
        let result = check_pack(&folder.0, &PACK, folder.uid(), &hasher);
        assert_eq!(result, PackCheck::Ok);
        assert_eq!(hasher.0.get(), 2);
    }

    #[test]
    fn a_changed_byte_of_the_same_size_fails_the_pack() {
        let folder = Folder::new();
        std::fs::write(folder.at("demo/sub/b.bin"), b"abe").unwrap();
        assert_eq!(folder.check(), PackCheck::Failed);
    }

    #[test]
    fn a_longer_and_a_shorter_file_fail_the_pack() {
        for content in [&b"abcd"[..], &b"ab"[..]] {
            let folder = Folder::new();
            std::fs::write(folder.at("demo/a.bin"), content).unwrap();
            let hasher = Counting(Cell::new(0));
            let result = check_pack(&folder.0, &PACK, folder.uid(), &hasher);
            assert_eq!(result, PackCheck::Failed, "{} bytes", content.len());
            // The length alone refuses it: nothing is read.
            assert_eq!(hasher.0.get(), 0, "{} bytes", content.len());
        }
    }

    /// A hasher that answers the pinned digest for any content, so only the
    /// length check can refuse the file.
    struct AlwaysPinned(u64);

    impl Hasher for AlwaysPinned {
        fn digest(&self, _file: &mut File, _cap_bytes: u64) -> io::Result<FileDigest> {
            Ok(FileDigest::Hashed {
                len: self.0,
                sha256_hex: ABC_SHA256.to_string(),
            })
        }
    }

    #[test]
    fn a_length_other_than_the_table_length_fails_even_with_the_pinned_digest() {
        static ONE: [ModelFile; 1] = [FILES[0]];
        let pack = Pack {
            id: "demo",
            files: &ONE,
        };
        let folder = Folder::new();
        for len in [2_u64, 4] {
            assert_eq!(
                check_pack(&folder.0, &pack, folder.uid(), &AlwaysPinned(len)),
                PackCheck::Failed,
                "{len} bytes"
            );
        }
        assert_eq!(
            check_pack(&folder.0, &pack, folder.uid(), &AlwaysPinned(3)),
            PackCheck::Ok
        );
    }

    #[test]
    fn an_absent_file_folder_or_pack_is_missing() {
        for gone in ["demo/sub/b.bin", "demo/sub", "demo"] {
            let folder = Folder::new();
            let path = folder.at(gone);
            if path.is_dir() {
                std::fs::remove_dir_all(path).unwrap();
            } else {
                std::fs::remove_file(path).unwrap();
            }
            assert_eq!(folder.check(), PackCheck::Missing, "{gone}");
        }
    }

    #[test]
    fn a_failed_file_wins_over_a_missing_file_in_either_order() {
        for (changed, removed) in [
            ("demo/a.bin", "demo/sub/b.bin"),
            ("demo/sub/b.bin", "demo/a.bin"),
        ] {
            let folder = Folder::new();
            std::fs::write(folder.at(changed), b"zzz").unwrap();
            std::fs::remove_file(folder.at(removed)).unwrap();
            assert_eq!(folder.check(), PackCheck::Failed, "changed {changed}");
        }
    }

    #[test]
    fn an_unreadable_file_fails_the_pack() {
        let folder = Folder::new();
        let path = folder.at("demo/a.bin");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        assert_eq!(folder.check(), PackCheck::Failed);
    }

    #[test]
    fn a_folder_where_a_file_belongs_fails_the_pack() {
        let folder = Folder::new();
        std::fs::remove_file(folder.at("demo/a.bin")).unwrap();
        std::fs::create_dir(folder.at("demo/a.bin")).unwrap();
        assert_eq!(folder.check(), PackCheck::Failed);
    }

    #[test]
    fn a_symlink_below_the_model_folder_is_refused() {
        for linked in ["demo/a.bin", "demo/sub", "demo"] {
            let folder = Folder::new();
            let real = folder.at("moved");
            std::fs::rename(folder.at(linked), &real).unwrap();
            symlink(&real, folder.at(linked)).unwrap();
            // The content behind the link matches: only the link is wrong.
            assert_eq!(folder.check(), PackCheck::Failed, "{linked}");
        }
    }

    #[test]
    fn a_model_folder_reached_through_a_symlink_is_accepted() {
        let folder = Folder::new();
        let holder = Folder::new();
        let link = holder.at("link-to-models");
        symlink(&folder.0, &link).unwrap();
        assert_eq!(
            check_pack(&link, &PACK, folder.uid(), &Sha256Hasher),
            PackCheck::Ok
        );
    }

    #[test]
    fn a_file_of_another_user_fails_the_pack_before_it_is_hashed() {
        let folder = Folder::new();
        let hasher = Counting(Cell::new(0));
        let other = folder.uid().wrapping_add(1);
        assert_eq!(
            check_pack(&folder.0, &PACK, other, &hasher),
            PackCheck::Failed
        );
        assert_eq!(hasher.0.get(), 0);
    }

    #[test]
    fn a_path_that_leaves_the_pack_folder_fails_the_pack() {
        static OUT: [ModelFile; 1] = [ModelFile {
            rel_path: "../a.bin",
            url: "https://models.example.com/a.bin",
            sha256_hex: ABC_SHA256,
            size_bytes: 3,
        }];
        let pack = Pack {
            id: "demo",
            files: &OUT,
        };
        let folder = Folder::new();
        std::fs::write(folder.at("a.bin"), b"abc").unwrap();
        assert_eq!(
            check_pack(&folder.0, &pack, folder.uid(), &Sha256Hasher),
            PackCheck::Failed
        );
    }

    /// `open_same` on its own thread: an open that waits fails the test and does
    /// not stop the run.
    #[cfg(target_os = "macos")]
    fn open_in_time(path: PathBuf, before: Metadata) -> io::Result<File> {
        let (done, opened) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = done.send(open_same(&path, &before));
        });
        opened
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the open waits")
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_pipe_put_at_the_path_is_refused_without_waiting() {
        let folder = Folder::new();
        let path = folder.at("demo/a.bin");
        let looked_at = std::fs::symlink_metadata(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        let made = std::process::Command::new("/usr/bin/mkfifo")
            .arg(&path)
            .status()
            .unwrap();
        assert!(made.success());
        // Put there after the look: it is another file.
        assert!(open_in_time(path.clone(), looked_at).is_err());
        // The pipe as the file that was looked at: it is not a regular file.
        let pipe = std::fs::symlink_metadata(&path).unwrap();
        assert!(open_in_time(path, pipe).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_link_to_the_same_file_put_after_the_look_is_refused() {
        let folder = Folder::new();
        let path = folder.at("demo/a.bin");
        let looked_at = std::fs::symlink_metadata(&path).unwrap();
        let moved = folder.at("moved");
        std::fs::rename(&path, &moved).unwrap();
        symlink(&moved, &path).unwrap();
        assert!(open_in_time(path, looked_at).is_err());
    }

    #[test]
    fn a_handle_of_another_file_than_the_one_looked_at_is_refused() {
        let folder = Folder::new();
        let looked_at = std::fs::symlink_metadata(folder.at("demo/a.bin")).unwrap();
        assert!(open_same(&folder.at("demo/a.bin"), &looked_at).is_ok());
        assert!(open_same(&folder.at("demo/sub/b.bin"), &looked_at).is_err());
    }
}
