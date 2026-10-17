# Running a testcontainers suite in the sandbox

A test suite that starts PostgreSQL, Kafka or a broker through testcontainers
needs a Docker API. The sandbox has no host daemon; `--docker` puts rootless
podman inside the image and starts its Docker API socket at session start, so
the suite runs as it does on a workstation, without reaching anything on the
host (ADR 0010).

## Start

```sh
cd ~/src/orders-service
claude_here --jvm --docker
```

Inside the session `DOCKER_HOST` points at the socket and
`TESTCONTAINERS_RYUK_DISABLED=true` is exported, as the testcontainers
documentation asks for rootless podman. Nothing else is configured; `mvn
verify` or `gradle test` finds the API the way it does on the host.

To make it the project's default:

```toml
# .claude_here/config.toml
toolchains = ["jvm", "docker"]
```

The same works for every other test stack: `--node --docker` for a Node suite,
`--python --docker` for pytest with testcontainers-python, `--go --docker` for
Go.

## What the first run does

The first `docker pull` of a session fetches the image into a named docker
volume for this project (`claude_here-containers-<sha8>`), so the next session
starts the container from the volume instead of pulling again. The volume stays
until you remove it:

```sh
claude_here volumes              # names of the volumes that exist
claude_here volumes --rm         # this project's
claude_here volumes --rm --all   # every project's
```

`claude_here uninstall` removes them together with the images.

## Limits that apply to the nested containers

The nested containers run inside the sandbox's cgroup and namespaces, so the
sandbox's limits are their limits:

| Limit             | Effect on the suite                                                                                                  |
|-------------------|----------------------------------------------------------------------------------------------------------------------|
| `--memory`        | Bounds the sandbox and every nested container together; Kafka plus three databases fit in 8g                         |
| pids (4096)       | The total of all processes inside, nested containers included                                                        |
| `--net allowlist` | Pulls reach Docker Hub, `ghcr.io` and `quay.io`; a company registry goes into `net_allow` and its CA into `[tls] ca` |
| Network capture   | The nested containers' traffic appears in `claude_here net last` like everything else                                |

On a Mac the container limit cannot exceed what the Docker Desktop VM has; give
the VM 8GB or more for a JVM suite.

## Things that differ from a workstation

* **Ryuk is off.** testcontainers still removes its containers when the test
  process exits normally. A test JVM killed with `kill -9` leaves them running
  until the session ends; `docker ps` and `docker rm` inside the session clean
  up earlier.
* **One session per project volume.** podman locks the storage; a second
  session of the same project fails at its first `docker` call.
* **No host images.** Images on the host daemon are invisible; every image is
  pulled once into the project volume.
* **No `--privileged`, no host paths.** A test that bind-mounts a host
  directory into its container has to use a path inside the sandbox, which is
  the project directory and the mounts you added.
* **Kernel 5.11 or newer** on the host, or in the Docker Desktop VM, for
  overlay storage in a user namespace. The session refuses to start the
  service on older kernels and says so.

## Checking that it works

```sh
claude_here --docker -p "run docker run --rm alpine echo ok and report the output"
```

A session that prints `ok` has the socket, the storage driver and the network
of the nested containers in place.
