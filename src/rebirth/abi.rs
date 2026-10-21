//! Exact guest records, independent of the host's libc structure layout.
//! Contract: original Emscripten reference and native-ios/native_host.c.
pub const STAT64_BYTES: usize = 104;

#[derive(Default, Debug)]
pub struct Stat64 {
    pub device: u32,
    pub mode: u32,
    pub links: u64,
    pub uid: u32,
    pub gid: u32,
    pub rdev: u32,
    pub size: u64,
    pub block_size: u32,
    pub blocks: u32,
    pub times: [(i64, i64); 3],
    pub inode: u64,
}
impl Stat64 {
    /// Writes exactly the guest ABI size. The destination must be exclusively
    /// owned; this is not permission to borrow concurrent shared Wasm memory.
    /// An invalid output range leaves every byte untouched.
    pub fn write(&self, destination: &mut [u8]) -> anyhow::Result<()> {
        anyhow::ensure!(
            destination.len() >= STAT64_BYTES,
            "short stat64 destination"
        );
        let b = &mut destination[..STAT64_BYTES];
        b.fill(0);
        for (at, value) in [
            (0, self.device),
            (4, self.mode),
            (16, self.uid),
            (20, self.gid),
            (24, self.rdev),
            (40, self.block_size),
            (44, self.blocks),
        ] {
            b[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        for (at, value) in [(8, self.links), (32, self.size), (96, self.inode)] {
            b[at..at + 8].copy_from_slice(&value.to_le_bytes());
        }
        for (i, (seconds, nanos)) in self.times.iter().enumerate() {
            let at = 48 + i * 16;
            b[at..at + 8].copy_from_slice(&seconds.to_le_bytes());
            b[at + 8..at + 16].copy_from_slice(&nanos.to_le_bytes());
        }
        Ok(())
    }
    #[cfg(unix)]
    pub fn from_metadata(m: &std::fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;
        // Explicit narrowing matches the guest's 32-bit fields, not host ABI.
        Self {
            device: m.dev() as u32,
            mode: m.mode(),
            links: m.nlink(),
            uid: m.uid(),
            gid: m.gid(),
            rdev: m.rdev() as u32,
            size: m.size(),
            block_size: m.blksize() as u32,
            blocks: m.blocks() as u32,
            times: [
                (m.atime(), m.atime_nsec()),
                (m.mtime(), m.mtime_nsec()),
                (m.ctime(), m.ctime_nsec()),
            ],
            inode: m.ino(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_layout_and_canaries() {
        let stat = Stat64 {
            device: 17,
            mode: 0o100644,
            links: 2,
            uid: 3,
            gid: 4,
            rdev: 5,
            size: 0x100000007,
            block_size: 4096,
            blocks: 9,
            times: [(-7, 999999999), (11, 22), (33, 44)],
            inode: 0x10000000b,
        };
        let mut bytes = [0xa5; 128];
        stat.write(&mut bytes[8..]).unwrap();
        assert_eq!(&bytes[..8], &[0xa5; 8]);
        assert_eq!(&bytes[112..], &[0xa5; 16]);
        let b = &bytes[8..112];
        for (at, n) in [
            (0, 17u32),
            (4, 0o100644),
            (16, 3),
            (20, 4),
            (24, 5),
            (28, 0),
            (40, 4096),
            (44, 9),
        ] {
            assert_eq!(u32::from_le_bytes(b[at..at + 4].try_into().unwrap()), n);
        }
        for (at, n) in [
            (8, 2u64),
            (32, 0x100000007),
            (48, (-7i64) as u64),
            (56, 999999999),
            (64, 11),
            (72, 22),
            (80, 33),
            (88, 44),
            (96, 0x10000000b),
        ] {
            assert_eq!(u64::from_le_bytes(b[at..at + 8].try_into().unwrap()), n);
        }
        let before = bytes;
        assert!(stat.write(&mut bytes[8..111]).is_err());
        assert_eq!(bytes, before);
    }
}
