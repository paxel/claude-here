# Windows: running claude_here in WSL2

claude_here has no Windows build. On Windows it runs inside a WSL2 distro,
where it has what it needs: a Linux kernel with namespaces and cgroups, a Linux
filesystem with uid and gid, and a `docker` command. WSL1 cannot run containers.

## Install inside the distro

Everything lives in the distro, not on the Windows side: the binary, the
configuration under `~/.config/claude_here`, Claude Code for the one-time
`claude setup-token`, and the projects.

```sh
# Linuxbrew inside the distro
brew tap paxel/tap && brew install claude-here

# or the release script into ~/.local/bin
curl -fsSL https://github.com/paxel/claude-here/releases/latest/download/install.sh | sh

claude_here init
```

claude_here shells out to `docker`, `git` and `id`, and `claude setup-token`
has to write into the WSL home that the container home is later seeded from.
A Windows installation of either would not see that home.

## Docker

Two ways to get a `docker` command in the distro:

* **Docker Desktop** with WSL integration: *Settings, Resources, WSL
  integration*, enable the distro. Without it `docker` is not on `PATH`
  there.
* **docker-ce inside the distro**: install it as on any Debian or Ubuntu
  machine and start it yourself with `sudo service docker start`. systemd is
  off in WSL unless you enabled it in `/etc/wsl.conf`.

Either way the daemon runs on a Linux kernel, so the `--docker` toolchain,
the egress allowlist and the network capture work as on Linux.

## Keep the project in the WSL filesystem

```
~/src/foo                      good
/mnt/c/Users/you/src/foo       no
```

A path under `/mnt/c` is a Windows bind mount. It is slow, and it does not
carry Linux uid and gid. claude_here bakes your uid and gid into the image and
the entrypoint refuses to start on a mismatch, so a project on the Windows
drive either fails that check or hands the container files it cannot own. The
same applies to `~/.config/claude_here`, which belongs in the WSL home.

Edit from Windows through `\\wsl$\<distro>\home\you\src\foo` or an editor's
WSL remote mode; the files stay on the Linux side.

## Memory

Docker Desktop and WSL share the memory limit of the WSL VM, set in
`%UserProfile%\.wslconfig`:

```ini
[wsl2]
memory=12GB
```

The container limit set by `--memory` cannot exceed what the VM has; a JVM or
Flutter toolchain is comfortable with 8GB.

## Checking that it works

```sh
cd ~/src/foo && claude_here --dry-run -p x
```

prints the `docker run` command. A first real start builds the base image once,
which takes a few minutes.
