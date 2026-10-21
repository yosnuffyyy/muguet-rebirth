// muguet - io.rs
// Constantin Clerc (c22dev) - 0.1

use crate::mem_base_len;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::Duration;
use wasmtime::SharedMemory;

static STARTED: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);

pub fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var("MUGUET_DATA").unwrap_or_default())
}

fn atomic_i32(mem: &SharedMemory, addr: u64) -> &AtomicI32 {
    let (base, len) = mem_base_len(mem);
    assert!(addr.checked_add(4).is_some_and(|end|end <= len as u64) && addr % 4 == 0);
    unsafe { &*(base.add(addr as usize) as *const AtomicI32) }
}

pub fn start(mem: &SharedMemory, table: u64) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let mem = mem.clone();
    std::thread::Builder::new()
        .name("io".into())
        .spawn(move || serve_loop(mem, table))
        .expect("spawn io thread");
    eprintln!("[io] native I/O thread started (table=0x{table:x})");
}

fn serve_loop(mem: SharedMemory, table: u64) {
    let (_, memory_bytes)=mem_base_len(&mem);
    if table==0 || table%4!=0 || table.checked_add(64).is_none_or(|end|end>memory_bytes as u64) {
        crate::host::report_fatal("invalid asset request table header".into());return;
    }
    let mut reader = match crate::rebirth::assets::Reader::open_data(&data_dir()) {
        Ok(r) => r,
        Err(e) => { crate::host::report_fatal(format!("asset reader: {e:#}")); return; }
    };
    let mut served: u64 = 0;
    let mut bytes: u64 = 0;
    loop {
        let seen = atomic_i32(&mem, table).load(Ordering::SeqCst);
        let raw_n = atomic_i32(&mem, table + 4).load(Ordering::SeqCst);
        let n = raw_n.max(0) as u64;
        let (_, memory_bytes) = mem_base_len(&mem);
        if raw_n < 0 || table.checked_add(64 + n * 64).is_none_or(|end| end > memory_bytes as u64) {
            crate::host::report_fatal("asset request table outside guest memory".into()); return;
        }
        for slot in 0..n {
            let w = table + 64 + 64 * slot;
            if atomic_i32(&mem, w).compare_exchange(1, 3, Ordering::SeqCst, Ordering::SeqCst).is_err() {
                continue;
            }
            let word = |k: u64| atomic_i32(&mem, w + 4 * k).load(Ordering::SeqCst) as u32 as u64;
            let id = word(1) as i32;
            let off = word(2) | (word(3) << 32);
            let dst = word(4) | (word(5) << 32);
            let len = word(6);
            let (_, mlen) = mem_base_len(&mem);
            let result = if dst == 0 || dst.checked_add(len).is_none_or(|end| end > mlen as u64) {
                eprintln!("[io] invalid destination id={id} offset={off} dst={dst} length={len}");
                -1
            } else {
                let mut cursor = dst;
                match reader.read(id as usize, off, len as usize, &STOP, |chunk| {
                    anyhow::ensure!(crate::write_bytes(&mem, cursor, chunk), "asset destination out of bounds");
                    cursor += chunk.len() as u64;
                    Ok(())
                }) {
                    Ok(n) => n as i64,
                    Err(e) => { eprintln!("[io] read id={id}@{off}+{len}: {e:#}"); -1 }
                }
            };
            served += 1;
            if result > 0 { bytes += result as u64; }
            if served % 2000 == 0 {
                eprintln!("[io] {served} reads, {} MB", bytes >> 20);
            }
            atomic_i32(&mem, w + 28).store(result as i32, Ordering::SeqCst);
            atomic_i32(&mem, w).store(2, Ordering::SeqCst);
            let _ = mem.atomic_notify(w, 1);
        }

        let _ = mem.atomic_wait32(table, seen as u32, Some(Duration::from_millis(20)));
    }
}
