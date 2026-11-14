//! The command line is split into claude_here's and claude's arguments:
//! both sides keep the order of the command line, and an argument that is on
//! neither side is a help or version flag or the `--` separator.
#![no_main]

use claude_here::cli::split_args;
use libfuzzer_sys::fuzz_target;

/// `part` appears in `whole` in the same order, gaps allowed.
fn is_subsequence(part: &[String], whole: &[String]) -> bool {
    let mut rest = whole.iter();
    part.iter().all(|p| rest.any(|w| w == p))
}

fuzz_target!(|args: Vec<String>| {
    let split = split_args(args.clone());
    assert!(is_subsequence(&split.tool, &args));
    assert!(is_subsequence(&split.claude, &args));
    let kept = split.tool.len() + split.claude.len();
    assert!(kept <= args.len());
    let dropped = args.len() - kept;
    let droppable = args
        .iter()
        .filter(|a| matches!(a.as_str(), "-h" | "--help" | "-V" | "--version" | "--"))
        .count();
    assert!(
        dropped <= droppable,
        "{args:?} -> {:?} / {:?}",
        split.tool,
        split.claude
    );
    if !split.wants_help && !split.wants_version {
        assert!(dropped <= 1, "only `--` is dropped silently: {args:?}");
    }
});
