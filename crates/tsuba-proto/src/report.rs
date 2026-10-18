use thiserror::Error;

/// Report length without the leading report-id byte.
pub const REPORT_LEN: usize = 90;
/// Argument bytes available in one report.
pub const ARGS_LEN: usize = 80;

const OFF_STATUS: usize = 0;
const OFF_TXID: usize = 1;
const OFF_SIZE: usize = 5;
const OFF_CLASS: usize = 6;
const OFF_ID: usize = 7;
const OFF_ARGS: usize = 8;
const OFF_CRC: usize = 88;

/// A command triplet `[data size, class, id]`. A Get is the Set id with bit 7 set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Command {
    pub size: u8,
    pub class: u8,
    pub id: u8,
}

impl Command {
    pub const fn new(size: u8, class: u8, id: u8) -> Self {
        Self { size, class, id }
    }

    pub const fn is_get(&self) -> bool {
        self.id & 0x80 != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    New,
    Busy,
    Ok,
    Fail,
    Timeout,
    Unsupported,
    Other(u8),
}

impl From<u8> for Status {
    fn from(b: u8) -> Self {
        match b {
            0x00 => Status::New,
            0x01 => Status::Busy,
            0x02 => Status::Ok,
            0x03 => Status::Fail,
            0x04 => Status::Timeout,
            0x05 => Status::Unsupported,
            x => Status::Other(x),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtoError {
    #[error("report is {0} bytes, expected {REPORT_LEN}")]
    Length(usize),
    #[error("{0} argument bytes exceed the {ARGS_LEN}-byte payload")]
    ArgsTooLong(usize),
    #[error("reply is for class 0x{got_class:02x} id 0x{got_id:02x}, expected 0x{want_class:02x}/0x{want_id:02x}")]
    Mismatch { want_class: u8, want_id: u8, got_class: u8, got_id: u8 },
    #[error("device returned {0:?}")]
    Status(Status),
}

/// One 90-byte protocol-2.5 report (the wire adds report id 0 in front).
#[derive(Clone, PartialEq, Eq)]
pub struct Report(pub [u8; REPORT_LEN]);

impl Report {
    /// Build a request. Checksum = XOR of bytes 2..88.
    pub fn request(cmd: Command, txid: u8, args: &[u8]) -> Result<Self, ProtoError> {
        if args.len() > ARGS_LEN {
            return Err(ProtoError::ArgsTooLong(args.len()));
        }
        let mut b = [0u8; REPORT_LEN];
        b[OFF_TXID] = txid;
        b[OFF_SIZE] = cmd.size;
        b[OFF_CLASS] = cmd.class;
        b[OFF_ID] = cmd.id;
        b[OFF_ARGS..OFF_ARGS + args.len()].copy_from_slice(args);
        b[OFF_CRC] = checksum(&b);
        Ok(Self(b))
    }

    pub fn parse(buf: &[u8]) -> Result<Self, ProtoError> {
        let b: [u8; REPORT_LEN] = buf.try_into().map_err(|_| ProtoError::Length(buf.len()))?;
        Ok(Self(b))
    }

    pub fn status(&self) -> Status {
        self.0[OFF_STATUS].into()
    }
    pub fn txid(&self) -> u8 {
        self.0[OFF_TXID]
    }
    pub fn size(&self) -> u8 {
        self.0[OFF_SIZE]
    }
    pub fn class(&self) -> u8 {
        self.0[OFF_CLASS]
    }
    pub fn id(&self) -> u8 {
        self.0[OFF_ID]
    }

    /// Payload bytes, bounded by the reported data size.
    pub fn args(&self) -> &[u8] {
        let n = (self.size() as usize).min(ARGS_LEN);
        &self.0[OFF_ARGS..OFF_ARGS + n]
    }

    pub fn checksum_ok(&self) -> bool {
        self.0[OFF_CRC] == checksum(&self.0)
    }

    /// Check that this reply answers `cmd` and succeeded.
    pub fn expect_ok(&self, cmd: Command) -> Result<&[u8], ProtoError> {
        if self.class() != cmd.class || self.id() != cmd.id {
            return Err(ProtoError::Mismatch {
                want_class: cmd.class,
                want_id: cmd.id,
                got_class: self.class(),
                got_id: self.id(),
            });
        }
        match self.status() {
            Status::Ok => Ok(self.args()),
            s => Err(ProtoError::Status(s)),
        }
    }
}

impl std::fmt::Debug for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Report {{ status: {:?}, txid: 0x{:02x}, cmd: [{}, 0x{:02x}, 0x{:02x}], args: {:02x?} }}",
            self.status(),
            self.txid(),
            self.size(),
            self.class(),
            self.id(),
            self.args()
        )
    }
}

fn checksum(b: &[u8; REPORT_LEN]) -> u8 {
    b[2..OFF_CRC].iter().fold(0, |acc, x| acc ^ x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_layout_and_checksum() {
        // Set Device Mode [2, 0x00, 0x04] -> driver mode (0x03, 0x00), as logged by Synapse.
        let r = Report::request(Command::new(2, 0x00, 0x04), 0x04, &[0x03, 0x00]).unwrap();
        assert_eq!(&r.0[..10], &[0x00, 0x04, 0, 0, 0, 0x02, 0x00, 0x04, 0x03, 0x00]);
        assert_eq!(r.0[88], 0x02 ^ 0x04 ^ 0x03);
        assert!(r.checksum_ok());
    }

    #[test]
    fn rejects_oversized_args() {
        assert_eq!(
            Report::request(Command::new(80, 2, 0x0d), 0x1f, &[0; 81]),
            Err(ProtoError::ArgsTooLong(81))
        );
    }

    #[test]
    fn reply_checks() {
        // Live reply to Get Firmware Version from a Basilisk V3 Pro.
        let mut b = [0u8; REPORT_LEN];
        b[..10].copy_from_slice(&[0x02, 0x1f, 0, 0, 0, 0x02, 0x00, 0x81, 0x02, 0x01]);
        let r = Report::parse(&b).unwrap();
        assert_eq!(r.expect_ok(Command::new(2, 0x00, 0x81)).unwrap(), &[0x02, 0x01]);
        assert!(matches!(
            r.expect_ok(Command::new(2, 0x00, 0x84)),
            Err(ProtoError::Mismatch { .. })
        ));
    }
}
