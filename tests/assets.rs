#![cfg(unix)]
use rebirth_contract_tests::rebirth::assets::{Reader, OPEN_FILES, SCRATCH_BYTES};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
static SEQ: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "gta-muguet-fixture-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        fs::create_dir(p.join("data")).unwrap();
        Self(p)
    }
    fn manifest(&self, rows: serde_json::Value) {
        fs::write(
            self.0.join("data/manifest.json"),
            serde_json::to_vec(&serde_json::json!({"mount":"/game/","files":rows})).unwrap(),
        )
        .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn exact_bytes_bounded_reads_cache_eof_and_cancellation() {
    let f = Fixture::new();
    let data: Vec<u8> = (0..SCRATCH_BYTES * 3 + 17)
        .map(|i| (i % 251) as u8)
        .collect();
    let mut rows = Vec::new();
    for id in 0..12 {
        let name = format!("{id}.rpf");
        fs::write(f.0.join("data").join(&name), &data).unwrap();
        rows.push(serde_json::json!([name, data.len(), 0]));
    }
    f.manifest(serde_json::json!(rows));
    let stop = AtomicBool::new(false);
    let mut reader = Reader::open(&f.0).unwrap();
    let metadata = fs::metadata(f.0.join("data/0.rpf")).unwrap();
    let stat = gta_muguet_runtime::abi::Stat64::from_metadata(&metadata);
    assert_eq!(stat.size, data.len() as u64);
    assert_eq!(stat.mode & 0o170000, 0o100000);
    assert_eq!(
        reader.manifest_bytes(),
        fs::read(f.0.join("data/manifest.json")).unwrap()
    );
    for id in 0..12 {
        let mut result = Vec::new();
        assert_eq!(
            reader
                .read(id, 11, data.len(), &stop, |b| {
                    assert!(b.len() <= SCRATCH_BYTES);
                    result.extend_from_slice(b);
                    Ok(())
                })
                .unwrap(),
            data.len() - 11
        );
        assert_eq!(result, data[11..]);
        assert!(reader.cached_files() <= OPEN_FILES);
    }
    assert_eq!(reader.cached_files(), 8);
    assert_eq!(reader.opens, 12);
    reader
        .read(11, 0, 17, &stop, |b| {
            assert_eq!(b, &data[..17]);
            Ok(())
        })
        .unwrap();
    assert_eq!(reader.cache_hits, 1);
    assert_eq!(
        reader
            .read(11, data.len() as u64, 17, &stop, |_| panic!("EOF sink"))
            .unwrap(),
        0
    );
    assert!(reader.read(12, 0, 1, &stop, |_| Ok(())).is_err());
    assert!(reader.read(11, u64::MAX, 1, &stop, |_| Ok(())).is_err());
    assert!(reader
        .read(11, 0, data.len(), &stop, |_| {
            stop.store(true, Ordering::Release);
            Ok(())
        })
        .is_err());
    assert!(reader.read(11, 0, 0, &stop, |_| panic!()).is_err());
    stop.store(false, Ordering::Release);
    assert!(reader
        .read(11, 0, 1, &stop, |_| anyhow::bail!("destination failure"))
        .is_err());
    assert_eq!(fs::read(f.0.join("data/11.rpf")).unwrap(), data);
}
#[test]
fn missing_truncated_symlink_and_replacement() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    f.manifest(serde_json::json!([
        ["missing", 3, 0],
        ["short", 4, 0],
        ["link", 3, 0],
        ["dirlink/file", 3, 0],
        ["live", 3, 0]
    ]));
    fs::write(f.0.join("data/short"), b"abc").unwrap();
    fs::write(f.0.join("outside"), b"bad").unwrap();
    symlink(f.0.join("outside"), f.0.join("data/link")).unwrap();
    fs::create_dir(f.0.join("other")).unwrap();
    fs::write(f.0.join("other/file"), b"bad").unwrap();
    symlink(f.0.join("other"), f.0.join("data/dirlink")).unwrap();
    fs::write(f.0.join("data/live"), b"old").unwrap();
    let stop = AtomicBool::new(false);
    let mut r = Reader::open(&f.0).unwrap();
    for id in 0..4 {
        assert!(r
            .read(id, 0, 3, &stop, |_| panic!("invalid read delivered bytes"))
            .is_err());
    }
    r.read(4, 0, 3, &stop, |b| {
        assert_eq!(b, b"old");
        Ok(())
    })
    .unwrap();
    fs::rename(f.0.join("data/live"), f.0.join("data/old-inode")).unwrap();
    fs::write(f.0.join("data/live"), b"new").unwrap();
    // An open archive's identity remains stable across pathname replacement.
    r.read(4, 0, 3, &stop, |b| {
        assert_eq!(b, b"old");
        Ok(())
    })
    .unwrap();
    drop(r);
    Reader::open(&f.0)
        .unwrap()
        .read(4, 0, 3, &stop, |b| {
            assert_eq!(b, b"new");
            Ok(())
        })
        .unwrap();
    fs::write(f.0.join("data/live"), b"x").unwrap();
    assert!(Reader::open(&f.0)
        .unwrap()
        .read(4, 0, 3, &stop, |_| Ok(()))
        .is_err());
}
