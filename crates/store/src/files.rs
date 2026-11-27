//! The database file on disk: owner-only modes, the state that must be refused
//! before SQLite opens it, and the truncation of the write-ahead log.

use rusqlite::Connection;
use std::io;
use std::path::{Path, PathBuf};

/// Mode of a database directory that the store creates: owner only.
#[cfg(unix)]
const PRIVATE_DIR_MODE: u32 = 0o700;
/// Mode of a database file and its `-wal` and `-shm` files: owner only.
#[cfg(unix)]
const PRIVATE_FILE_MODE: u32 = 0o600;

fn beside(db: &Path, suffix: &str) -> PathBuf {
    let mut name = db.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// Size of a file in bytes. A missing file has size 0.
fn size_of(path: &Path) -> io::Result<u64> {
    match std::fs::metadata(path) {
        Ok(found) => Ok(found.len()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(e),
    }
}

/// True when the database file is missing or holds at most 1 byte while a log
/// beside it holds data. SQLite reads such a file as new and removes the log.
pub(crate) fn is_empty_with_log(db: &Path) -> io::Result<bool> {
    // The log is read first. SQLite writes the file header before it creates a
    // log, so an open that runs at the same moment cannot look like this state.
    Ok(size_of(&beside(db, "-wal"))? > 0 && size_of(db)? <= 1)
}

/// Create the parent directory of a database file (mode 0700 when this call
/// creates it) and the file itself (mode 0600), before SQLite opens it. Files
/// that exist are tightened to 0600. SQLite gives new `-wal` and `-shm` files
/// the mode of the main file. Directories that already exist are left as they
/// are: they may be shared (for example the temp directory). On other platforms
/// this only creates the directory and the file.
pub(crate) fn prepare_private_db(db: &Path) -> io::Result<()> {
    if let Some(parent) = db.parent().filter(|p| !p.as_os_str().is_empty()) {
        if !parent.exists() {
            create_private_dir(parent)?;
        }
    }
    create_private_file(db)?;
    tighten_sidecars(db)
}

#[cfg(unix)]
fn create_private_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(PRIVATE_DIR_MODE)
        .create(dir)
}

#[cfg(not(unix))]
fn create_private_dir(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(PRIVATE_FILE_MODE)
        .open(path)?;
    // `mode` above does not change a file that already existed.
    tighten(path)
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> io::Result<()> {
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map(|_| ())
}

/// Set an existing file to 0600. A missing file is skipped.
#[cfg(unix)]
fn tighten(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::set_permissions(path, std::fs::Permissions::from_mode(PRIVATE_FILE_MODE)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(not(unix))]
fn tighten(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// Set the `-wal` and `-shm` files that exist beside `db` to 0600.
pub(crate) fn tighten_sidecars(db: &Path) -> io::Result<()> {
    tighten(&beside(db, "-wal"))?;
    tighten(&beside(db, "-shm"))
}

/// Set the database file and its sidecars to 0600 and create nothing.
pub(crate) fn tighten_existing(db: &Path) -> io::Result<()> {
    tighten(db)?;
    tighten_sidecars(db)
}

/// Checkpoint the write-ahead log and truncate it to zero bytes, so old copies
/// of deleted text do not stay in it (ADR 0008). Returns `Ok(true)` when the log
/// is empty afterwards (also for a database without a log) and `Ok(false)` when
/// another connection's read lock stopped the checkpoint. The truncate releases
/// the bytes of the log; it does not overwrite the freed disk blocks.
pub(crate) fn truncate_wal(conn: &Connection) -> rusqlite::Result<bool> {
    // The row is (busy, log frames, checkpointed frames); busy is 1 when the
    // checkpoint could not finish.
    let busy: i64 = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))?;
    Ok(busy == 0)
}
