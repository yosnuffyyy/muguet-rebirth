//! Bounded process-outcome evidence, independent of rotated stdout logs.
//! No signal handlers, per-frame work, or inference about OS termination causes.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

const LIMIT: usize = 8192;
const NAME: &str = "rebirth-session.json";
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Launcher,
    Running,
    UserStop,
    EngineExit,
    Fatal,
    Panic,
}
impl Outcome {
    fn terminal(self) -> bool {
        !matches!(self, Self::Launcher | Self::Running)
    }
    fn failure(self) -> bool {
        matches!(self, Self::Fatal | Self::Panic)
    }
}
#[derive(Debug, Serialize, Deserialize)]
struct Record {
    schema: u32,
    session: String,
    started_unix_ms: u64,
    updated_unix_ms: u64,
    identity: String,
    outcome: Outcome,
    detail: String,
}
struct Journal {
    root: PathBuf,
    record: Record,
}
static CURRENT: OnceLock<Mutex<Journal>> = OnceLock::new();
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
fn bounded(text: &str, chars: usize) -> String {
    text.chars()
        .take(chars)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
impl Journal {
    fn open(root: &Path, session: u128, at: u64) -> Result<(Self, Option<String>)> {
        fs::create_dir_all(root)?;
        let previous = match OpenOptions::new().read(true).open(root.join(NAME)) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(LIMIT as u64 + 1).read_to_end(&mut bytes)?;
                if bytes.len() > LIMIT {
                    Some("previous session record exceeds bounds; outcome UNKNOWN".into())
                } else {
                    match serde_json::from_slice::<Record>(&bytes) {
                        Ok(record) if record.schema == 1 => {
                            let status = match record.outcome {
                                Outcome::Launcher | Outcome::Running => "no recorded clean exit; cause unconfirmed (including user force-close, crash or OS termination)",
                                Outcome::UserStop => "explicit user stop recorded; not proof of completed shutdown",
                                Outcome::EngineExit => "engine exit recorded; see exit code, not proof of gameplay",
                                Outcome::Fatal => "fatal error recorded",
                                Outcome::Panic => "Rust panic recorded",
                            };
                            Some(format!("previous session {} started_unix_ms={} updated_unix_ms={}: {status}; installed app={}; detail={}", bounded(&record.session, 40), record.started_unix_ms, record.updated_unix_ms, bounded(&record.identity, 512), bounded(&record.detail, 512)))
                        }
                        _ => Some(
                            "previous session record unreadable or unsupported; outcome UNKNOWN"
                                .into(),
                        ),
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        let journal = Self {
            root: root.into(),
            record: Record {
                schema: 1,
                session: session.to_string(),
                started_unix_ms: at,
                updated_unix_ms: at,
                identity: "not yet supplied".into(),
                outcome: Outcome::Launcher,
                detail: "launcher started; PLAY not yet requested".into(),
            },
        };
        journal.persist()?;
        Ok((journal, previous))
    }
    fn persist(&self) -> Result<()> {
        let bytes = serde_json::to_vec(&self.record)?;
        ensure!(bytes.len() <= LIMIT, "session record exceeds bound");
        let temp = self.root.join("rebirth-session.tmp");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        // Same-directory rename publishes a complete record. A failed write or
        // abrupt exit before rename leaves the last complete record intact.
        fs::rename(temp, self.root.join(NAME))?;
        Ok(())
    }
    fn update(&mut self, outcome: Outcome, detail: &str, at: u64) -> Result<()> {
        // Preserve the first fatal/panic and do not turn an error into a clean
        // exit when another thread or the UI subsequently requests shutdown.
        if self.record.outcome.failure() || (self.record.outcome.terminal() && !outcome.failure()) {
            return Ok(());
        }
        self.record.outcome = outcome;
        self.record.detail = bounded(detail, 512);
        self.record.updated_unix_ms = at;
        self.persist()
    }
}
pub fn start(root: &Path, session: u128) -> Result<Option<String>> {
    if CURRENT.get().is_some() {
        return Ok(None);
    }
    let (journal, previous) = Journal::open(root, session, now())?;
    ensure!(
        CURRENT.set(Mutex::new(journal)).is_ok(),
        "session journal already started"
    );
    Ok(previous)
}
pub fn identify(identity: &str) {
    if let Some(journal) = CURRENT.get() {
        let mut journal = journal.lock().unwrap_or_else(|e| e.into_inner());
        journal.record.identity = bounded(identity, 512);
        journal.record.updated_unix_ms = now();
        if let Err(e) = journal.persist() {
            eprintln!("[rebirth] session identity persistence failed: {e}");
        }
    }
}
fn update(outcome: Outcome, detail: &str) {
    if let Some(journal) = CURRENT.get() {
        let mut journal = journal.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = journal.update(outcome, detail, now()) {
            eprintln!("[rebirth] session outcome persistence failed: {e}");
        }
    }
}
pub fn running(mode: u32) {
    update(
        Outcome::Running,
        &format!("engine launch requested, mode={mode}; gameplay unverified"),
    );
}
pub fn user_stop(detail: &str) {
    update(Outcome::UserStop, detail);
}
pub fn engine_exit(code: i32) {
    update(Outcome::EngineExit, &format!("engine exit code={code}"));
}
pub fn fatal(detail: &str) {
    update(Outcome::Fatal, detail);
}
pub fn panic(detail: &str) {
    // A panic while writing a journal must never recursively block on its lock.
    if let Some(journal) = CURRENT.get() {
        if let Ok(mut journal) = journal.try_lock() {
            let _ = journal.update(Outcome::Panic, detail, now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    fn root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "rebirth-session-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
    }
    #[test]
    fn unfinished_sessions_are_unknown_and_preserve_prior_identity() {
        let root = root();
        let (mut journal, previous) = Journal::open(&root, 17, 100).unwrap();
        assert!(previous.is_none());
        journal.record.identity = "version=0.46.5 build=51 source=old".into();
        journal.update(Outcome::Running, "mode=2", 200).unwrap();
        drop(journal);
        let (_, prior) = Journal::open(&root, 18, 300).unwrap();
        let prior = prior.unwrap();
        assert!(prior.contains("no recorded clean exit; cause unconfirmed"));
        assert!(prior.contains("version=0.46.5 build=51 source=old"));
        assert!(prior.contains("started_unix_ms=100 updated_unix_ms=200"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn fatal_survives_exit_and_unicode_records_remain_bounded() {
        let root = root();
        let (mut journal, _) = Journal::open(&root, 19, 100).unwrap();
        journal.record.identity = bounded(&"🦀".repeat(5000), 512);
        journal
            .update(
                Outcome::Fatal,
                &format!("original\n{}", "🦀".repeat(5000)),
                200,
            )
            .unwrap();
        journal.update(Outcome::EngineExit, "exit=0", 300).unwrap();
        journal.update(Outcome::UserStop, "stop", 400).unwrap();
        assert!(fs::metadata(root.join(NAME)).unwrap().len() <= LIMIT as u64);
        assert!(!root.join("rebirth-session.tmp").exists());
        drop(journal);
        let (_, prior) = Journal::open(&root, 20, 500).unwrap();
        let prior = prior.unwrap();
        assert!(prior.contains("fatal error recorded"));
        assert!(prior.contains("detail=original "));
        assert!(!prior.contains("exit=0"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn partial_temporary_file_cannot_replace_previous_record() {
        let root = root();
        let (mut journal, _) = Journal::open(&root, 21, 100).unwrap();
        journal
            .update(Outcome::UserStop, "window close", 200)
            .unwrap();
        drop(journal);
        fs::write(root.join("rebirth-session.tmp"), b"{partial").unwrap();
        let (_, prior) = Journal::open(&root, 22, 300).unwrap();
        assert!(prior.unwrap().contains("explicit user stop recorded"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn corrupt_or_oversized_records_are_reported_without_blocking_startup() {
        let root = root();
        fs::create_dir_all(&root).unwrap();
        for bytes in [b"{broken".to_vec(), vec![b'x'; LIMIT + 1]] {
            fs::write(root.join(NAME), bytes).unwrap();
            let (_, prior) = Journal::open(&root, 23, 100).unwrap();
            assert!(prior.unwrap().contains("outcome UNKNOWN"));
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn concurrent_exit_cannot_hide_fatal_and_engine_exit_keeps_code() {
        let root = root();
        let (mut journal, _) = Journal::open(&root, 24, 100).unwrap();
        journal
            .update(Outcome::EngineExit, "engine exit code=17", 200)
            .unwrap();
        drop(journal);
        let (journal, prior) = Journal::open(&root, 25, 300).unwrap();
        assert!(prior.unwrap().contains("engine exit code=17"));
        let journal = std::sync::Arc::new(Mutex::new(journal));
        let exit = journal.clone();
        let failure = journal.clone();
        let a = std::thread::spawn(move || {
            exit.lock()
                .unwrap()
                .update(Outcome::UserStop, "stop", 400)
                .unwrap()
        });
        let b = std::thread::spawn(move || {
            failure
                .lock()
                .unwrap()
                .update(Outcome::Fatal, "original failure", 401)
                .unwrap()
        });
        a.join().unwrap();
        b.join().unwrap();
        assert_eq!(journal.lock().unwrap().record.outcome, Outcome::Fatal);
        drop(journal);
        let (_, prior) = Journal::open(&root, 26, 500).unwrap();
        assert!(prior.unwrap().contains("detail=original failure"));
        fs::remove_dir_all(root).unwrap();
    }
}
