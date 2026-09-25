# Changelog

## 0.2.0 (unreleased)

* Claude Code moved out of the base image into its own last layer (`images/claude.dockerfile`), above the toolchains and the user and project layers. A new Claude release rebuilds that one layer, in seconds, instead of the whole chain. Image files changed, so every image is rebuilt once on first start, and the image a session runs gains a `-claude` suffix. The existing tags stay in use as intermediate layers; `docker image prune` reclaims the space of the replaced content afterwards.
* Claude Code release detection: the newest version of the configured channel is read from `downloads.claude.ai` at most once a day after a session and cached in `~/.config/claude_here/claude-version.json`. A newer version is offered at the next interactive start (`[y/N]`, asked again until accepted; one hint line without a terminal). An accepted version is picked up by every image on its next start without another question; new images get the accepted version. `claude_version = "stable"` follows the stable channel, a version number pins it and switches the check off; `update_check = false` switches off both checks.
* `claude_here update` fetches the newest Claude Code, accepts it and rebuilds only the Claude layer, reporting the version that was actually in the image before and after. It takes the toolchain flags of a run (`update --node`), so it updates the image that run uses. `update --base` also refreshes the OS packages of the base with `--no-cache --pull`.
* Fixed: after `update`, images of other toolchain combinations (for example a `--node` run) kept the old Claude Code because the rebuilt base kept its hash. A layer's hash now includes the image id of its parent, so a rebuilt base or toolchain makes every image above it rebuild on its next start. Under `--no-build` such an image now fails instead of running on the old base.
* `--save` no longer lists a toolchain twice when it is already in the config.

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
