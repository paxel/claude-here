# syntax=docker/dockerfile:1.7
# cpp toolchain (alias `c`): the C/C++ build ecosystem around the compilers
# that base already ships via build-essential — cmake, ninja, gdb, clang with
# the clangd language server, valgrind, and Conan as package manager.
ARG BASE=claude_here:base
FROM ${BASE}
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    apt-get update \
 && apt-get install -y --no-install-recommends \
      cmake ninja-build gdb clang clangd clang-format clang-tidy valgrind
RUN --mount=type=cache,target=/root/.cache/pip,sharing=locked \
    python3 -m venv /opt/conan \
 && /opt/conan/bin/pip install conan \
 && ln -s /opt/conan/bin/conan /usr/local/bin/conan \
 && cmake --version && ninja --version && clangd --version && conan --version
