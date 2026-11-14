//! A config file is hand-written TOML: any text must be accepted or rejected
//! without a panic, and an accepted config must survive a write and a read.
#![no_main]

use claude_here::config::ConfigFile;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    let Ok(config) = toml::from_str::<ConfigFile>(text) else {
        return;
    };
    let Ok(written) = toml::to_string_pretty(&config) else {
        return;
    };
    let read: ConfigFile = toml::from_str(&written).unwrap_or_else(|e| panic!("{e}\n{written}"));
    assert_eq!(read, config);
});
