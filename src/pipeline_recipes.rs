//! Stream optional pipeline warmup recipes instead of retaining an expanded JSON queue.
//!
//! The readable length is frozen when opened: recipes recorded by this launch must
//! not extend its own warmup. Required draws still compile through the normal path.
use std::{
    fs::File,
    io::{self, BufRead, BufReader, Read, Take},
    path::Path,
};

pub(crate) const FILE_LIMIT: u64 = 32 << 20;
const READ_BUFFER: usize = 16 << 10;

pub(crate) struct Recipes<R: Read = File> {
    reader: Option<BufReader<Take<R>>>,
    line: Vec<u8>,
    bytes: u64,
    malformed: u64,
    peak_line: usize,
}

impl Recipes<File> {
    pub(crate) fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let bytes = file.metadata()?.len();
        Ok(Self::new(file, bytes))
    }
}

impl<R: Read> Recipes<R> {
    fn new(reader: R, bytes: u64) -> Self {
        Self {
            reader: (bytes != 0).then(|| BufReader::with_capacity(READ_BUFFER, reader.take(bytes))),
            line: Vec::new(),
            bytes,
            malformed: 0,
            peak_line: 0,
        }
    }

    pub(crate) fn bytes(&self) -> u64 { self.bytes }
    pub(crate) fn is_empty(&self) -> bool { self.reader.is_none() }
    pub(crate) fn malformed(&self) -> u64 { self.malformed }
    pub(crate) fn peak_line(&self) -> usize { self.peak_line }

    /// Consume one line, including a final valid record without a newline.
    /// Malformed records return None, preserving the original filter_map policy.
    /// Callers check is_empty separately and retain their existing work deadline.
    pub(crate) fn next_recipe(&mut self) -> io::Result<Option<serde_json::Value>> {
        self.line.clear();
        let Some(reader) = self.reader.as_mut() else { return Ok(None) };
        loop {
            let available = reader.fill_buf()?;
            if available.is_empty() { self.reader = None; break; }
            let end = available.iter().position(|&b| b == b'\n').map(|n| n + 1);
            let take = end.unwrap_or(available.len());
            if self.line.len() as u64 + take as u64 > FILE_LIMIT {
                // This is optional persisted cache data, never a graphics command.
                // Do not allocate beyond the entire normal cache write budget.
                self.reader = None;
                self.line = Vec::new();
                return Err(io::Error::new(io::ErrorKind::InvalidData, "pipeline recipe exceeds 32 MiB; remaining prewarm deferred to required draws"));
            }
            self.line.extend_from_slice(&available[..take]);
            reader.consume(take);
            if end.is_some() { break; }
        }
        self.peak_line = self.peak_line.max(self.line.len());
        if self.line.is_empty() { return Ok(None); }
        match serde_json::from_slice(&self.line) {
            Ok(recipe) => Ok(Some(recipe)),
            Err(_) => { self.malformed += 1; Ok(None) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, io::Cursor, rc::Rc};

    #[test]
    fn preserves_eager_recipe_values_order_and_final_line() {
        let text = "{\"vs\":\"a\",\"samples\":1}\r\nnot-json\n\n{\"ps\":\"b\",\"strides\":[0,16],\"rod\":false}";
        let eager: Vec<serde_json::Value> = text.lines().filter_map(|line| serde_json::from_str(line).ok()).collect();
        let mut reader = Recipes::new(Cursor::new(text.as_bytes()), text.len() as u64);
        let mut streamed = Vec::new();
        while !reader.is_empty() {
            if let Some(recipe) = reader.next_recipe().unwrap() { streamed.push(recipe); }
        }
        assert_eq!(streamed, eager);
        assert_eq!(reader.malformed(), 2);
        assert_eq!(reader.bytes(), text.len() as u64);
    }

    #[test]
    fn frozen_length_does_not_read_new_session_appends() {
        let first = "{\"vs\":\"old\"}\n";
        let all = format!("{first}{{\"vs\":\"new\"}}\n");
        let mut reader = Recipes::new(Cursor::new(all.into_bytes()), first.len() as u64);
        assert_eq!(reader.next_recipe().unwrap().unwrap()["vs"], "old");
        assert!(reader.next_recipe().unwrap().is_none());
        assert!(reader.is_empty());
    }

    #[test]
    fn empty_truncated_and_io_error_are_distinguished() {
        let mut empty = Recipes::new(Cursor::new(Vec::<u8>::new()), 0);
        assert!(empty.is_empty());
        assert!(empty.next_recipe().unwrap().is_none());
        let mut partial = Recipes::new(Cursor::new(b"{\"vs\":"), 7);
        assert!(partial.next_recipe().unwrap().is_none());
        assert_eq!(partial.malformed(), 1);
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> { Err(io::ErrorKind::Other.into()) }
        }
        assert!(Recipes::new(Broken, 1).next_recipe().is_err());
    }

    struct Repeated {
        line: Vec<u8>,
        position: usize,
        count: usize,
        read: Rc<Cell<usize>>,
    }
    impl Read for Repeated {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            let take = out.len().min(self.line.len() * self.count - self.position);
            for (k, byte) in out[..take].iter_mut().enumerate() { *byte = self.line[(self.position + k) % self.line.len()]; }
            self.position += take;
            self.read.set(self.position);
            Ok(take)
        }
    }

    #[test]
    fn large_cache_streams_without_eager_parse_or_resident_queue() {
        let line = format!("{{\"vs\":\"a\",\"payload\":\"{}\"}}\n", "x".repeat(1000));
        let count = (FILE_LIMIT as usize) / line.len();
        let bytes = count * line.len();
        let read = Rc::new(Cell::new(0));
        let input = Repeated { line: line.as_bytes().to_vec(), position: 0, count, read: read.clone() };
        let mut reader = Recipes::new(input, bytes as u64);
        assert_eq!(read.get(), 0, "opening must not parse the full cache");
        assert!(reader.next_recipe().unwrap().is_some());
        assert!(read.get() <= READ_BUFFER);
        let mut parsed = 1;
        while !reader.is_empty() { if reader.next_recipe().unwrap().is_some() { parsed += 1; } }
        assert_eq!(parsed, count);
        assert_eq!(read.get(), bytes);
        assert_eq!(reader.peak_line(), line.len());
        assert!(reader.line.capacity() <= READ_BUFFER);
        println!("pipeline recipe synthetic cache: {bytes} file bytes; {count} recipes; eager queue {count} JSON values versus streaming <=1; read buffer {READ_BUFFER}, peak line {} bytes; excludes parsed-value allocation and driver memory", reader.peak_line());
    }

    #[test]
    fn accepts_large_existing_records_and_rejects_over_budget_line() {
        // The guard equals the existing total file write budget, not a new small
        // record limit that would discard legitimate older descriptors.
        let valid = format!("{{\"payload\":\"{}\"}}", "x".repeat(256 << 10));
        let mut reader = Recipes::new(Cursor::new(valid.as_bytes()), valid.len() as u64);
        assert_eq!(reader.next_recipe().unwrap().unwrap()["payload"].as_str().unwrap().len(), 256 << 10);
        let input = std::io::repeat(b'x');
        let mut oversized = Recipes::new(input, FILE_LIMIT + 1);
        assert_eq!(oversized.next_recipe().unwrap_err().kind(), io::ErrorKind::InvalidData);
        assert!(oversized.is_empty());
        assert_eq!(oversized.line.capacity(), 0);
    }
}
