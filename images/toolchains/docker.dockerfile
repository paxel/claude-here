# syntax=docker/dockerfile:1.7
# docker toolchain: rootless podman, the `docker` CLI shim and the Docker API
# socket (ADR 0010). Nested containers run as the sandbox user inside a user
# namespace; no host daemon is reachable. The entrypoint starts the socket and
# the run drops `no-new-privileges` for the uid-mapping helpers below.
ARG BASE=claude_here:base
FROM ${BASE}
ARG CH_USER=ni

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    apt-get update \
 && apt-get install -y --no-install-recommends \
      podman podman-docker crun passt netavark aardvark-dns uidmap catatonit \
      libcap2-bin \
 && touch /etc/containers/nodocker \
 && usermod --add-subuids 100000-165535 --add-subgids 100000-165535 "${CH_USER}"

# `no-new-privileges` is off while this toolchain runs, so no file in the
# image may be setuid or setgid; the build fails if one is left. The two
# uid-mapping helpers carry file capabilities instead, the way Fedora ships
# them. Setuid root would not even work: the kernel lets a process write
# another namespace's uid_map only when it owns that namespace or holds
# CAP_SYS_ADMIN over it. Root in the sandbox has neither (and must not get
# CAP_SYS_ADMIN); the sandbox user owns the namespace podman creates and
# needs only CAP_SETUID/CAP_SETGID, which are in the bounding set. The tool's
# tests pin this block.
RUN find / -xdev -type f \( -perm -4000 -o -perm -2000 \) -exec chmod ug-s {} + \
 && left="$(find / -xdev -type f \( -perm -4000 -o -perm -2000 \) | sort | tr '\n' ' ')" \
 && [ -z "${left}" ] \
 || { echo "unexpected setuid/setgid files: ${left}"; exit 1; } \
 && setcap cap_setuid+ep /usr/bin/newuidmap \
 && setcap cap_setgid+ep /usr/bin/newgidmap \
 && getcap /usr/bin/newuidmap /usr/bin/newgidmap

# Debian's storage.conf stays untouched: it selects native overlay (kernel
# >= 5.11; no FUSE fallback and never vfs) and declares the rootful runroot
# and graphroot a storage.conf must contain for podman to start at all; the
# rootless run uses its own defaults under $XDG_RUNTIME_DIR and ~/.local/share.
# cgroupfs: there is no systemd in the sandbox. The rootless storage root is a
# named volume (an overlay upper cannot sit on the sandbox's own overlay).
# Every directory created under the home is handed to the sandbox user,
# `.local/share` included: the claude layer writes there as that user.
RUN printf '[engine]\ncgroup_manager = "cgroupfs"\nevents_logger = "file"\n[containers]\nnetns = "pasta"\n' \
      > /etc/containers/containers.conf \
 && install -d -o "${CH_USER}" -g "$(id -g "${CH_USER}")" \
      "/home/${CH_USER}/.local/share" \
      "/home/${CH_USER}/.local/share/containers" \
      "/home/${CH_USER}/.local/share/containers/storage" \
 && podman --version && docker --version
