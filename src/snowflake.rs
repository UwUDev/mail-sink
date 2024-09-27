use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::Mutex;
use lazy_static::lazy_static;

const EPOCH: u128 = 1704067200000; // January 1, 2024, 00:00:00 UTC in milliseconds
const SEQUENCE_BITS: u32 = 12;
const SEQUENCE_MASK: u128 = (1 << SEQUENCE_BITS) - 1;

lazy_static! {
    static ref SNOWFLAKE: Mutex<Snowflake> = Mutex::new(Snowflake::new());
}

struct Snowflake {
    sequence: u128,
    last_timestamp: u128,
}

impl Snowflake {
    fn new() -> Self {
        Snowflake {
            sequence: 0,
            last_timestamp: 0,
        }
    }

    fn next_id(&mut self) -> u128 {
        let timestamp = Self::current_timestamp();
        if timestamp != self.last_timestamp {
            self.sequence = 0;
            self.last_timestamp = timestamp;
        } else {
            self.sequence += 1;
            if self.sequence > SEQUENCE_MASK {
                // sequence overflow, wait for the next timestamp
                while Self::current_timestamp() == self.last_timestamp {
                    // busy wait, not ideal, but simple
                }
                self.sequence = 0;
                self.last_timestamp = Self::current_timestamp();
            }
        }
        (self.last_timestamp - EPOCH) << SEQUENCE_BITS | self.sequence
    }

    fn current_timestamp() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
    }
}

pub fn next() -> u128 {
    SNOWFLAKE.lock().unwrap().next_id()
}

pub fn to_timestamp(snowflake: u128) -> u128 {
    (snowflake >> SEQUENCE_BITS) + EPOCH
}