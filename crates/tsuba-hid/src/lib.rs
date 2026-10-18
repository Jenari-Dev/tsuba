//! hidraw transport for Razer protocol 2.5: device discovery via sysfs and
//! feature-report exchange via HIDIOCSFEATURE / HIDIOCGFEATURE.

use std::fs::{self, File, OpenOptions};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

use tsuba_proto::{Command, ProtoError, Report, Status, REPORT_LEN};
use thiserror::Error;

pub const RAZER_VID: u16 = 0x1532;

pub mod db;

pub use db::{Features, Kind, Spec};

/// A device model from the database. `iface` is the USB interface whose hidraw node accepts
/// protocol-2.5 feature reports; `event_iface` carries hardware-event input reports (record
/// types 2–5); `txid` is the transaction id the firmware expects in requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Model {
    pub pid: u16,
    pub name: &'static str,
    pub iface: u8,
    pub event_iface: u8,
    pub kind: Kind,
    pub txid: u8,
    /// Everything else the database knows (features, matrix, LEDs, effects…).
    pub spec: &'static Spec,
}

impl Model {
    fn from_spec(s: &'static Spec) -> Self {
        Self { pid: s.pid, name: &s.name, iface: s.iface, event_iface: s.event_iface, kind: s.kind, txid: s.txid, spec: s }
    }

    /// Verified with Tsuba on real hardware (writes allowed by default).
    pub fn tested(&self) -> bool {
        self.spec.tested
    }
}

/// Every model Tsuba can talk to (database entries with `supported = true`).
pub fn models() -> &'static [Model] {
    static MODELS: std::sync::OnceLock<Vec<Model>> = std::sync::OnceLock::new();
    MODELS.get_or_init(|| db::all().iter().filter(|s| s.supported).map(Model::from_spec).collect())
}

/// The model for `pid`, if Tsuba supports it.
pub fn model(pid: u16) -> Option<Model> {
    models().iter().find(|m| m.pid == pid).copied()
}

#[derive(Debug, Error)]
pub enum HidError {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error(transparent)]
    Proto(#[from] ProtoError),
    #[error("refusing to send Set command 0x{class:02x}/0x{id:02x} without write access enabled")]
    WriteBlocked { class: u8, id: u8 },
    #[error("refusing to send Set command 0x{class:02x}/0x{id:02x}: this device model has not been tested with Tsuba, so writes are off until you enable them for it (allow_untested_writes = true in its [[device]] config section, the daemon's SetAllowUntestedWrites, or `tsuba --allow-untested`)")]
    Untested { class: u8, id: u8 },
    #[error("device stayed busy after {0} polls")]
    Busy(u32),
}

/// A discovered device node.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub model: Model,
    pub hidraw: PathBuf,
}

/// Find supported devices' control nodes by scanning /sys/class/hidraw.
pub fn discover() -> Vec<DeviceInfo> {
    discover_iface(|m| m.iface)
}

/// Find supported devices' hardware-event nodes.
pub fn discover_events() -> Vec<DeviceInfo> {
    discover_iface(|m| m.event_iface)
}

fn discover_iface(pick: impl Fn(&Model) -> u8) -> Vec<DeviceInfo> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/hidraw") else { return out };
    for e in entries.flatten() {
        let dev = e.path().join("device");
        let Some((vid, pid)) = hid_ids(&dev) else { continue };
        if vid != RAZER_VID {
            continue;
        }
        let Some(model) = model(pid) else { continue };
        if usb_interface(&dev) == Some(pick(&model)) {
            out.push(DeviceInfo { model, hidraw: Path::new("/dev").join(e.file_name()) });
        }
    }
    out.sort_by_key(|d| d.model.pid);
    out
}

/// `HID_ID=0003:00001532:0000028D` from the HID device's uevent.
fn hid_ids(dev: &Path) -> Option<(u16, u16)> {
    let uevent = fs::read_to_string(dev.join("uevent")).ok()?;
    let id = uevent.lines().find_map(|l| l.strip_prefix("HID_ID="))?;
    let mut parts = id.split(':').skip(1);
    let vid = u32::from_str_radix(parts.next()?, 16).ok()?;
    let pid = u32::from_str_radix(parts.next()?, 16).ok()?;
    Some((vid as u16, pid as u16))
}

/// The parent USB interface directory is named like `3-2.1:1.3`; the number after the dot.
fn usb_interface(dev: &Path) -> Option<u8> {
    let real = fs::canonicalize(dev).ok()?;
    let name = real.parent()?.file_name()?.to_str()?;
    name.rsplit_once('.')?.1.parse().ok()
}

const fn ioc_rw(nr: u64, size: usize) -> u64 {
    (3 << 30) | ((size as u64) << 16) | ((b'H' as u64) << 8) | nr
}
const HIDIOCSFEATURE: u64 = ioc_rw(0x06, REPORT_LEN + 1);
const HIDIOCGFEATURE: u64 = ioc_rw(0x07, REPORT_LEN + 1);

/// Which kind of stored data a Set command changes (see [`WriteCounters`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Domain {
    /// Key/button assignment tables.
    Assign = 0,
    /// Profile list, DPI, polling, scroll and other per-profile settings.
    Settings = 1,
    /// On-board macro memory.
    Macros = 2,
    /// Battery / sleep / power settings.
    Power = 3,
}

/// Per-device counters bumped by every Set command, by what it changes. They let callers keep
/// caches of device reads that are exact: a cached value is current while the counter of its
/// [`Domain`] is unchanged. Lighting (class 0x0f) writes change none of them. Shared via `Arc`
/// so a cache can be checked without taking the lock around the [`Device`].
#[derive(Debug)]
pub struct WriteCounters {
    id: u64,
    gens: [AtomicU64; 4],
}

impl WriteCounters {
    fn new() -> Arc<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Arc::new(Self { id: NEXT.fetch_add(1, Ordering::Relaxed), gens: Default::default() })
    }

    /// Unique per opened device (a re-attached device gets a new one).
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn get(&self, d: Domain) -> u64 {
        self.gens[d as usize].load(Ordering::SeqCst)
    }

    /// Mark everything of `d` as changed (also used for outside events, e.g. a hardware notification).
    pub fn touch(&self, d: Domain) {
        self.gens[d as usize].fetch_add(1, Ordering::SeqCst);
    }

    pub fn touch_all(&self) {
        for d in [Domain::Assign, Domain::Settings, Domain::Macros, Domain::Power] {
            self.touch(d);
        }
    }

    /// Record a Set command that is about to be sent.
    fn record(&self, cmd: Command) {
        match (cmd.class, cmd.id) {
            (0x0f, _) => {}
            (0x07, _) => self.touch(Domain::Power),
            (0x06, _) => self.touch(Domain::Macros),
            (0x02, 0x0c | 0x0d) => self.touch(Domain::Assign),
            // Switching the active on-board profile or naming one leaves every stored table as it
            // was (tables are kept per profile; the live profile 0 is never cached).
            (0x05, 0x04 | 0x08) => self.touch(Domain::Settings),
            _ => {
                self.touch(Domain::Assign);
                self.touch(Domain::Settings);
            }
        }
    }
}

/// An open device. Set commands are refused unless `allow_writes` is called, and for models
/// not yet tested with Tsuba also until `allow_untested` is called.
pub struct Device {
    file: File,
    path: PathBuf,
    txid: u8,
    writes: bool,
    untested_lock: bool,
    counters: Arc<WriteCounters>,
    /// In-process stand-in for the hardware (tests and simulations); see [`Device::simulated`].
    responder: Option<Responder>,
    pub poll_interval: Duration,
    pub max_polls: u32,
}

/// Answers commands in place of a real device: gets the command and its arguments, returns the
/// reply payload (or an error, e.g. a protocol error for an unsupported command).
pub type Responder = Arc<dyn Fn(Command, &[u8]) -> Result<Vec<u8>, HidError> + Send + Sync>;

impl Device {
    /// Open a discovered device's control node, using its model's transaction id.
    pub fn open(info: &DeviceInfo) -> Result<Self, HidError> {
        let mut d = Self::open_path(&info.hidraw, info.model.txid)?;
        d.untested_lock = !info.model.tested();
        Ok(d)
    }

    /// Open a hidraw node directly with an explicit transaction id (no untested-model lock).
    pub fn open_path(path: impl AsRef<Path>, txid: u8) -> Result<Self, HidError> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|source| HidError::Io { path: path.clone(), source })?;
        Ok(Self { file, path, txid, writes: false, untested_lock: false, counters: WriteCounters::new(), responder: None, poll_interval: Duration::from_millis(2), max_polls: 150 })
    }

    /// A device whose replies come from `responder` instead of hardware (tests, simulations).
    /// Write gates, the untested-model lock and the write counters behave as for a real device.
    pub fn simulated(responder: Responder) -> Result<Self, HidError> {
        let mut d = Self::open_path("/dev/null", 0x1f)?;
        d.responder = Some(responder);
        Ok(d)
    }

    pub fn allow_writes(&mut self, on: bool) {
        self.writes = on;
    }

    /// Lift (or restore) the lock on Set commands for a model not yet tested with Tsuba.
    pub fn allow_untested(&mut self, on: bool) {
        self.untested_lock = !on;
    }

    /// Whether Set commands are currently refused because the model is untested.
    pub fn untested_locked(&self) -> bool {
        self.untested_lock
    }

    /// Write counters for cache invalidation; see [`WriteCounters`].
    pub fn counters(&self) -> &Arc<WriteCounters> {
        &self.counters
    }

    pub fn txid(&self) -> u8 {
        self.txid
    }

    fn check_write(&self, cmd: Command) -> Result<(), HidError> {
        if cmd.is_get() {
            return Ok(());
        }
        if !self.writes {
            return Err(HidError::WriteBlocked { class: cmd.class, id: cmd.id });
        }
        if self.untested_lock {
            return Err(HidError::Untested { class: cmd.class, id: cmd.id });
        }
        Ok(())
    }

    /// Send `cmd` with `args` and return the successful reply's payload. If the reply belongs to
    /// another command (something else talked to the device in between), the request is re-sent.
    pub fn query(&self, cmd: Command, args: &[u8]) -> Result<Vec<u8>, HidError> {
        self.check_write(cmd)?;
        if !cmd.is_get() {
            self.counters.record(cmd);
        }
        let mut attempt = 0;
        loop {
            match self.query_once(cmd, args) {
                Err(HidError::Proto(ProtoError::Mismatch { .. })) if attempt < 3 => {
                    attempt += 1;
                    sleep(Duration::from_millis(5));
                }
                r => return r,
            }
        }
    }

    fn query_once(&self, cmd: Command, args: &[u8]) -> Result<Vec<u8>, HidError> {
        if let Some(r) = &self.responder {
            return r(cmd, args);
        }
        let req = Report::request(cmd, self.txid, args)?;
        let mut wire = [0u8; REPORT_LEN + 1];
        wire[1..].copy_from_slice(&req.0);
        self.ioctl(HIDIOCSFEATURE, &mut wire)?;
        for _ in 0..self.max_polls {
            sleep(self.poll_interval);
            let mut buf = [0u8; REPORT_LEN + 1];
            self.ioctl(HIDIOCGFEATURE, &mut buf)?;
            let reply = Report::parse(&buf[1..])?;
            if reply.status() == Status::Busy {
                continue;
            }
            return Ok(reply.expect_ok(cmd)?.to_vec());
        }
        Err(HidError::Busy(self.max_polls))
    }

    /// Send a Set command without waiting for the reply (frame streaming).
    pub fn send(&self, cmd: Command, args: &[u8]) -> Result<(), HidError> {
        self.check_write(cmd)?;
        if let Some(r) = &self.responder {
            return r(cmd, args).map(|_| ());
        }
        let req = Report::request(cmd, self.txid, args)?;
        let mut wire = [0u8; REPORT_LEN + 1];
        wire[1..].copy_from_slice(&req.0);
        self.ioctl(HIDIOCSFEATURE, &mut wire)
    }

    fn ioctl(&self, req: u64, buf: &mut [u8; REPORT_LEN + 1]) -> Result<(), HidError> {
        // SAFETY: buf is a valid, writable buffer of exactly the size encoded in `req`.
        let r = unsafe { libc::ioctl(self.file.as_raw_fd(), req as _, buf.as_mut_ptr()) };
        if r < 0 {
            return Err(HidError::Io { path: self.path.clone(), source: std::io::Error::last_os_error() });
        }
        Ok(())
    }
}

/// Read-only reader for a hardware-event node.
pub struct EventReader {
    file: File,
    pub path: PathBuf,
    pub model: Model,
}

impl EventReader {
    pub fn open(info: &DeviceInfo) -> Result<Self, HidError> {
        let file = OpenOptions::new()
            .read(true)
            .open(&info.hidraw)
            .map_err(|source| HidError::Io { path: info.hidraw.clone(), source })?;
        Ok(Self { file, path: info.hidraw.clone(), model: info.model })
    }

    pub fn fd(&self) -> std::os::fd::RawFd {
        self.file.as_raw_fd()
    }

    /// Read one input report (blocking). The first byte is the report id.
    pub fn read_report(&mut self, buf: &mut [u8]) -> Result<usize, HidError> {
        use std::io::Read;
        self.file.read(buf).map_err(|source| HidError::Io { path: self.path.clone(), source })
    }
}

/// Wait until any of `fds` is readable or `timeout_ms` passes; returns readable indices.
pub fn poll_readable(fds: &[std::os::fd::RawFd], timeout_ms: i32) -> Vec<usize> {
    let mut p: Vec<libc::pollfd> = fds.iter().map(|&fd| libc::pollfd { fd, events: libc::POLLIN, revents: 0 }).collect();
    // SAFETY: p is a valid array of pollfd of the given length.
    let n = unsafe { libc::poll(p.as_mut_ptr(), p.len() as libc::nfds_t, timeout_ms) };
    if n <= 0 {
        return Vec::new();
    }
    p.iter().enumerate().filter(|(_, x)| x.revents & libc::POLLIN != 0).map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ioctl_numbers_match_linux_headers() {
        // _IOC(_IOC_WRITE|_IOC_READ, 'H', 0x06/0x07, 91)
        assert_eq!(HIDIOCSFEATURE, 0xC05B_4806);
        assert_eq!(HIDIOCGFEATURE, 0xC05B_4807);
    }

    #[test]
    fn verified_models_keep_their_interfaces() {
        let kb = model(0x028d).unwrap();
        assert_eq!((kb.iface, kb.event_iface, kb.kind, kb.txid, kb.name), (3, 1, Kind::Keyboard, 0x1f, "BlackWidow V4 Pro"));
        let m = model(0x00aa).unwrap();
        assert_eq!((m.iface, m.event_iface, m.kind, m.txid, m.name), (0, 1, Kind::Mouse, 0x1f, "Basilisk V3 Pro (wired)"));
        assert!(kb.tested() && m.tested());
    }

    #[test]
    fn discovery_table_covers_database_but_not_headsets() {
        assert!(models().len() > 250);
        assert!(models().iter().all(|m| m.kind != Kind::Headset));
        assert!(model(0x0501).is_none(), "Kraken headset is listed but not driven");
        assert!(model(0x0084).is_some_and(|m| !m.tested()));
    }
}
