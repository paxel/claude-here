# Changelog

## [0.6.0]

* Network recording is off by default; `net_capture = true` or the new `--net-log` turns it on ([here-1018](https://github.com/paxel/claude-here/issues/27)).
* Ending a recorded session no longer waits for the network summary: `net last`, `show`, `top` and `grep` build it from the capture the first time they need it. `images/` changed, so every image is rebuilt once on first start ([here-1019](https://github.com/paxel/claude-here/issues/28)).
* `net last` and `net show` print one row per host, the busiest 20, and count the plain HTTP requests; `--all` shows every host and lists the requests ([here-1020](https://github.com/paxel/claude-here/issues/29)).
* `--resume`, `-r`, `--continue` and `-c` in a project without an earlier conversation start a fresh session with a note instead of finding nothing to resume ([here-1021](https://github.com/paxel/claude-here/issues/30)).
* Toolchain images install only locked, hash-checked packages: pip and npm tools come from lockfiles, and uv, rustup, Claude Code and NodeSource's key are checked by sha256 instead of piping an install script into a shell. `images/` changed, so every image is rebuilt once on first start ([here-1023](https://github.com/paxel/claude-here/issues/32)).
* The container's `/tmp` is a private directory under the host's `/tmp` (`/tmp/claude_here-<uid>/<session>`), deleted when the session ends, instead of a tmpfs that filled RAM ([here-1025](https://github.com/paxel/claude-here/issues/39)).
* Containers are limited to 8g of memory by default; `--memory none` or `memory = "none"` removes the limit ([here-1026](https://github.com/paxel/claude-here/issues/40)).
* A memory limit no longer allows the same amount of swap on top; the container stops at the limit ([here-1027](https://github.com/paxel/claude-here/issues/41)).
* Internal: unit tests drive docker, git and the terminal paths through a fake `docker`, temp directories and seams, aiming at 80% line coverage ([here-1022](https://github.com/paxel/claude-here/issues/31)).

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
