# ADR 0010: Containers inside the sandbox through rootless podman

Date: 2026-10-16
Status: accepted

## Context

Integration tests with testcontainers are everyday work: the test starts a
database or a broker as a container and talks to it. Inside the sandbox there
is no docker, so such a test suite cannot run and Claude cannot run it either.
claude_here itself builds and runs images and cannot dogfood its own chain
from within a session.

Every usual way of giving a container "docker" hands over the host:

* The host socket (`/var/run/docker.sock`) is root on the host — `docker run
  -v /:/host --privileged` from inside ends the sandbox. The containers it
  starts are siblings on the host daemon, outside the egress allowlist
  (ADR 0006), outside the capture (ADR 0003), outside the pids and memory
  limits, with any host path mountable.
* A daemon inside (`docker:dind`) needs `--privileged`, which removes
  `--cap-drop ALL` and `no-new-privileges` and lets root in the container
  mount host disks.
* A filtered socket proxy keeps the host daemon and its sibling containers, and
  testcontainers' reaper (Ryuk) itself bind-mounts the socket, so the filter
  must either allow bind mounts or lose Ryuk.
* Sysbox does it properly but has to be installed on the host and does not
  exist for Docker Desktop.

The purpose is one user's test suite, but the tool is used by people with
other purposes, so the mechanism must not depend on trusting the session.

## Decision

**A `docker` toolchain (alias `podman`) that runs rootless podman inside the
sandbox**, as the sandbox user, in a nested user namespace. The `docker`
command is podman's own shim (`podman-docker`), and `podman system service`
provides the Docker API socket testcontainers and other API clients expect.
Nested containers live inside the sandbox's user namespace, network namespace
and cgroup: an escape from one of them yields the sandbox user, nothing on the
host; their traffic goes through the sandbox resolver and packet filter and is
recorded like everything else; their processes count against `--pids-limit`
and the memory limit.

It is a toolchain and not a fourth restriction axis because the user's need is
"docker in there", like "rust in there", and because the image layer is
useless without the run-shape changes and vice versa. The `Toolchain` registry
therefore gains run-side fields, which this toolchain is the first to use.

The run shape relaxes in two places, and only while this toolchain is on:

* **`no-new-privileges` is dropped.** Mapping the uids of a multi-user image
  (`postgres` switching to its own user) goes through the setuid helpers
  `newuidmap`/`newgidmap`, and that flag forbids every setuid gain. The
  flag was defending nothing else: the layer strips the setuid and setgid bits
  from every other file in the image (`su`, `mount`, `passwd`…) and the build
  fails if anything but those two helpers is left setuid. `CAP_SETUID` and
  `CAP_SETGID` are already in the bounding set for the entrypoint's own
  `setpriv`.
* **A custom seccomp profile replaces Docker's default.** The default allows
  `unshare`, `clone3`, `mount`, `umount2`, `pivot_root`, `setns`,
  `sethostname` and the new mount API only with `CAP_SYS_ADMIN`, which the
  sandbox does not have and must not get. The shipped profile is Docker's
  default (`images/seccomp-default.json`, a verbatim copy with its origin
  recorded next to it) plus one rule that allows those syscalls
  unconditionally. `unconfined` was rejected: it opens forty more syscalls
  (`kexec_load`, `reboot`, `bpf`…) for nothing. The profile is written to the
  config directory before the run and passed as `--security-opt seccomp=`.

Image and layer storage of the nested containers live in a **named docker
volume per project** (`claude_here-containers-<sha8(project path)>`), mounted
at podman's storage root. The sandbox's own root filesystem is an overlay and
the kernel refuses an overlay upper directory on an overlay; a host bind mount
like the other caches would work on Linux but not under Docker Desktop, where
host mounts come through virtiofs and fail the same way. The volume works on
both and survives sessions, so images are pulled once. It is the first mount in
the registry that is not a host path, and the only one that needs a cleanup
command of its own (`claude_here volumes`); `uninstall` removes them with the
images.

The storage driver is native overlay in the user namespace and nothing else.
That needs kernel 5.11 (2021); the entrypoint refuses to start the service on
older kernels with a message instead of silently falling back to `vfs`, which
copies every layer and fills the volume. `fuse-overlayfs` as a second path
would be a path nobody tests.

The entrypoint starts `podman system service --time=0` as the sandbox user on
`/run/user/<uid>/podman/podman.sock` and exports `DOCKER_HOST`, so the first
test run works instead of failing with "cannot connect to the docker daemon".
`TESTCONTAINERS_RYUK_PRIVILEGED=true` keeps testcontainers' reaper on:
sessions are long, and leaked containers of an afternoon's test runs would eat
the pids limit. "Privileged" under rootless podman stays inside the user
namespace.

Under `net_mode = allowlist` the toolchain allows Docker Hub, `ghcr.io` and
`quay.io`; company registries go into `net_allow` like any other host. The
cloud toolchains are the place for the cloud vendors' registries.

The system prompt states the relaxation in the same sentence that states the
three restriction axes.

## Consequences

* Two sessions of the same project cannot share the storage volume at the same
  time; podman locks it and the second one fails at the first `docker` call.
  Acceptable for a per-project tool; the message says which session holds it.
* `--privileged`, host paths outside the sandbox, `--network host` of the
  *host*, and the host's images are all unavailable to nested containers.
  Image pulls happen inside, into the volume, once per project.
* A new Docker default profile means re-copying `images/seccomp-default.json`;
  the file records where it came from and when. The appended rule is
  generated from a list in `toolchain.rs`, so a second toolchain can add
  syscalls without a second profile file.
* `cargo test` guards: `newuidmap`/`newgidmap` are the only setuid files
  (build-time check in the layer, tested through the Dockerfile text), the
  generated profile parses and keeps Docker's `defaultAction`, and the run
  arguments carry neither `no-new-privileges` nor the default profile when the
  toolchain is on and both when it is off.
* The pids limit of 4096 and the memory limit now also bound the nested
  containers. A test suite that starts Kafka and three databases fits; one
  that needs more raises `--memory`, not the sandbox.
