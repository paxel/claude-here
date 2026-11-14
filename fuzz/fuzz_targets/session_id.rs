//! Session ids come from file names in the log directory: any string must
//! parse to a time or to nothing, and a parsed time must print back to the
//! same date and time.
#![no_main]

use claude_here::session::{Civil, session_start_secs};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|id: &str| {
    if let Some(secs) = session_start_secs(id) {
        let compact = Civil::from_unix(secs).compact();
        if let Some(again) = session_start_secs(&format!("{compact}-0000")) {
            assert_eq!(again, secs, "{id} -> {compact}");
        }
    }
});
