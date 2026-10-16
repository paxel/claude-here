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
 && touch /etc/containers/nodocker \
 && usermod --add-subuids 100000-165535 --add-subgids 100000-165535 "${CH_USER}"

# `no-new-privileges` is off while this toolchain runs, so the only setuid or
# setgid files left in the image are the two helpers that need it. The build
# fails if anything else is found; the tool's tests pin this block.
RUN find / -xdev -type f \( -perm -4000 -o -perm -2000 \) \
      ! -name newuidmap ! -name newgidmap -exec chmod ug-s {} + \
 && left="$(find / -xdev -type f \( -perm -4000 -o -perm -2000 \) | sort | tr '\n' ' ')" \
 && [ "${left}" = "/usr/bin/newgidmap /usr/bin/newuidmap " ] \
 || { echo "unexpected setuid/setgid files: ${left}"; exit 1; }

# Native overlay in the user namespace (kernel >= 5.11); no FUSE fallback and
# never vfs. cgroupfs: there is no systemd in the sandbox. The storage root is
# a named volume (an overlay upper cannot sit on the sandbox's own overlay).
RUN printf '[storage]\ndriver = "overlay"\n' > /etc/containers/storage.conf \
 && printf '[engine]\ncgroup_manager = "cgroupfs"\nevents_logger = "file"\n[containers]\nnetns = "pasta"\n' \
      > /etc/containers/containers.conf \
 && mkdir -p "/home/${CH_USER}/.local/share/containers/storage" \
 && chown "${CH_USER}:" "/home/${CH_USER}/.local/share/containers" \
                        "/home/${CH_USER}/.local/share/containers/storage" \
 && podman --version && docker --version
