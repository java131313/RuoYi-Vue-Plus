use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Snowflake-lite id generator: single process, no worker id.
/// Layout: (millis - EPOCH) << 12 | sequence
pub struct IdGen {
    last_ms: i64,
    seq: i64,
}

const EPOCH: i64 = 1704067200000; // 2024-01-01T00:00:00Z

impl IdGen {
    pub fn new() -> Self {
        IdGen { last_ms: -1, seq: 0 }
    }

    pub fn next(&mut self) -> i64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        if now == self.last_ms {
            self.seq = (self.seq + 1) & 0xFFF;
            if self.seq == 0 {
                // sequence exhausted in the same millisecond; spin
                let mut n = now;
                while n <= self.last_ms {
                    n = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0);
                }
                self.last_ms = n;
            }
        } else {
            self.last_ms = now;
            self.seq = 0;
        }
        (self.last_ms - EPOCH) << 12 | self.seq
    }
}

pub struct Ids {
    inner: Mutex<IdGen>,
}

impl Ids {
    pub fn new() -> Self {
        Ids { inner: Mutex::new(IdGen::new()) }
    }
    pub fn next(&self) -> i64 {
        self.inner.lock().unwrap().next()
    }
}
