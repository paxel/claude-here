//! Summaries and session files are written inside the sandbox, so the host
//! reads them as untrusted JSON: any bytes must be accepted or rejected
//! without a panic.
#![no_main]

use claude_here::net::Summary;
use claude_here::session::SessionInfo;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<Summary>(data);
    let _ = serde_json::from_slice::<SessionInfo>(data);
});
