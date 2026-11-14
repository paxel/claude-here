# Changelog

## 0.6.0

* Network recording is off by default; `net_capture = true` or the new `--net-log` turns it on ([here-1018](https://github.com/paxel/claude-here/issues/27)).
* Ending a recorded session no longer waits for the network summary: `net last`, `show`, `top` and `grep` build it from the capture the first time they need it. `images/` changed, so every image is rebuilt once on first start ([here-1019](https://github.com/paxel/claude-here/issues/28)).

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
