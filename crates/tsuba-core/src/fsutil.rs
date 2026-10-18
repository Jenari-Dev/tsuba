//! Small file-system helpers shared by everything that writes Tsuba's files.

use std::io::{Read, Write};
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};

/// Random hex for temporary file names.
pub fn random_suffix() -> String {
    let mut b = [0u8; 8];
    if std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).is_err() {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u64;
        b = (t ^ (u64::from(std::process::id()) << 32) ^ N.fetch_add(1, std::sync::atomic::Ordering::Relaxed).wrapping_mul(0x9e37_79b9_7f4a_7c15)).to_le_bytes();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Write via a new temporary file + fsync + rename: the destination's directory entry is
/// replaced, never written through (a symlink at the destination is never followed). The
/// temporary file gets a random name and is created exclusively without following links
/// (`O_CREAT | O_EXCL | O_NOFOLLOW`), so nothing planted at a temporary name is ever written
/// through or reused either.
pub fn write_atomic(dest: &Path, data: &[u8]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let dir = dest.parent().ok_or_else(|| anyhow!("no parent folder"))?;
    let name = dest.file_name().unwrap_or_default().to_string_lossy().into_owned();
    // Leftover of an interrupted write by older versions (fixed name): unlink, which removes a
    // link itself rather than its target.
    let _ = std::fs::remove_file(dir.join(format!(".{name}.tsuba-tmp")));
    for _ in 0..16 {
        let tmp = dir.join(format!(".{name}.{}.tsuba-tmp", random_suffix()));
        let mut f = match std::fs::OpenOptions::new().write(true).create_new(true).mode(0o644).custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC).open(&tmp) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e).with_context(|| format!("writing {}", tmp.display())),
        };
        let written = f.write_all(data).and_then(|()| f.sync_all());
        drop(f);
        return written
            .with_context(|| format!("writing {}", tmp.display()))
            .and_then(|()| std::fs::rename(&tmp, dest).with_context(|| format!("replacing {}", dest.display())))
            .inspect_err(|_| {
                let _ = std::fs::remove_file(&tmp);
            });
    }
    bail!("could not create a temporary file next to {}", dest.display())
}
