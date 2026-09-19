# cpp toolchain (alias `c`): the C/C++ build ecosystem around the compilers
# that base already ships via build-essential — cmake, ninja, gdb, clang with
# the clangd language server, valgrind, and Conan as package manager.
ARG BASE=claude_here:base
FROM ${BASE}
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      cmake ninja-build gdb clang clangd clang-format clang-tidy valgrind \
 && rm -rf /var/lib/apt/lists/* \
 && python3 -m venv /opt/conan \
 && /opt/conan/bin/pip install --no-cache-dir conan \
 && ln -s /opt/conan/bin/conan /usr/local/bin/conan \
 && cmake --version && ninja --version && clangd --version && conan --version
