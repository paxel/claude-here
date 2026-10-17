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

The run shape changes in five places, and only while this toolchain is on:

* **`no-new-privileges` is dropped.** Mapping the uids of a multi-user image
  (`postgres` switching to its own user) goes through the helpers
  `newuidmap`/`newgidmap`, which gain `CAP_SETUID`/`CAP_SETGID` on exec, and
  that flag forbids every privilege gain on exec. The flag was defending
  nothing else: the layer strips the setuid and setgid bits from every file
  in the image (`su`, `mount`, `passwd`, and the two helpers themselves) and
  the build fails if any is left. The helpers carry file capabilities
  instead, as Fedora ships them, and not out of taste: the kernel lets a
  process write another namespace's `uid_map` only when it owns that
  namespace or holds `CAP_SYS_ADMIN` over it. A setuid-root helper is root,
  which does not own the namespace the sandbox user created and must not get
  `CAP_SYS_ADMIN`; the first real run failed exactly there (`newuidmap: write
  to uid_map failed: Operation not permitted`). With file capabilities the
  helper stays the sandbox user, owns the namespace, and needs only
  `CAP_SETUID`/`CAP_SETGID`, which are in the bounding set already for the
  entrypoint's own `setpriv`.
* **A custom seccomp profile replaces Docker's default.** The default allows
  `unshare`, `clone3`, `mount`, `umount2`, `pivot_root`, `setns`,
  `sethostname` and the new mount API only with `CAP_SYS_ADMIN`, and
  `chroot` only with `CAP_SYS_CHROOT`; the sandbox has neither and must not
  get them. Inside the user namespace the sandbox user owns, the kernel
  grants these calls on its own; the profile only has to stop refusing them. The shipped profile is Docker's
  default (`images/seccomp-default.json`, a verbatim copy with its origin
  recorded next to it) plus one rule that allows those syscalls
  unconditionally. `unconfined` was rejected: it opens forty more syscalls
  (`kexec_load`, `reboot`, `bpf`…) for nothing. The profile is written to the
  config directory before the run and passed as `--security-opt seccomp=`.
* **Docker's default AppArmor profile is not applied** (`--security-opt
  apparmor=unconfined`). `docker-default` contains `deny mount,`, which
  AppArmor enforces regardless of capabilities and namespaces, so podman's
  overlay probe failed on an Ubuntu host with "kernel does not support
  overlay fs". A profile of our own cannot be used: it would have to be
  loaded into the host kernel as root. What `docker-default` adds beyond the
  dropped capabilities and the read-only `/proc/sys` and `/sys` mounts docker
  makes anyway is small: the `mount` denial, a few `/proc` write denials and
  ptrace scoping. The daemon ignores the option on hosts without AppArmor.
* **`/dev/net/tun` is passed through** (`--device`). Nested containers get
  their network from pasta, which builds the container's interface on a tap
  device; the sandbox container has no such node of its own. The node is
  usable only with `CAP_NET_ADMIN` over a network namespace, which the
  sandbox user holds inside its own user namespace and nowhere else.
* **Docker's `/proc` masks and read-only paths are not applied**
  (`--security-opt systempaths=unconfined`). The kernel lets a user namespace
  mount a procfs of its own only if the procfs it can already see is "fully
  visible", that is, not overmounted; Docker's masks (`/proc/kcore`,
  `/proc/keys`…) and read-only binds (`/proc/sys`…) are such overmounts.
  With them in place crun cannot mount `/proc` for the nested container,
  falls back to binding the sandbox's, and the container's own sysctls fail
  (`open /proc/sys/net/ipv4/ping_group_range: Read-only file system`), as
  would anything reading its pids through `/proc`. What this gives up: the
  root phase of the entrypoint could now write global sysctls, which the
  read-only bind prevented; the sandbox user cannot, the files are root's,
  and the nested containers' network namespaces are the only ones it owns.
  The masked files are readable by root only. The root phase is this tool's
  own script, short, and runs before anything of the user's.

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
`TESTCONTAINERS_RYUK_DISABLED=true` is exported too: testcontainers' own
documentation for rootless podman says to disable the reaper (Ryuk; the
`TESTCONTAINERS_RYUK_PRIVILEGED` switch only ever applied to rootful podman
before 1.19). Testcontainers still removes its containers when the test JVM
exits; only a `kill -9` leaks them, and the whole set dies with the session.

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
* Within one session, a test JVM killed with `kill -9` leaves its containers
  running and counting against the pids limit; `docker ps` / `docker rm` as
  usual.
* `cargo test` guards: no setuid or setgid file is left and the two helpers
  get their file capabilities (build-time check in the layer, tested through
  the Dockerfile text and on a real image in the integration tests), the
  generated profile parses and keeps Docker's `defaultAction`, and the run
  arguments carry neither `no-new-privileges` nor the default seccomp
  profile, and do carry `apparmor=unconfined`, when the toolchain is on, and
  the reverse when it is off.
* The pids limit of 4096 and the memory limit now also bound the nested
  containers. A test suite that starts Kafka and three databases fits; one
  that needs more raises `--memory`, not the sandbox.
