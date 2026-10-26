#![forbid(unsafe_code)]
// Shipped code may not panic: no unwrap, expect or panic, no unreachable, no
// indexing or slicing that can go out of bounds, no arithmetic that can overflow.
// Unit tests are exempt via `cfg(test)`; integration tests are separate crates.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented,
        clippy::unreachable,
        clippy::indexing_slicing,
        clippy::string_slice,
        clippy::arithmetic_side_effects
    )
)]

fn main() -> std::process::ExitCode {
    claude_here::main_entry(false)
}
