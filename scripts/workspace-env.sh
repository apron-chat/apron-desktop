#!/bin/sh
# Optional local workspace toolchain; normal installations do not need this.
export CARGO_HOME=/workspace/.tools/cargo RUSTUP_HOME=/workspace/.tools/rustup
GPUI_SYSROOT=/workspace/.tools/gpui-sysroot
export PATH="$CARGO_HOME/bin:$GPUI_SYSROOT/usr/bin:$PATH"
export LD_LIBRARY_PATH="$GPUI_SYSROOT/usr/lib/x86_64-linux-gnu:$GPUI_SYSROOT/usr/lib/llvm-19/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export LIBRARY_PATH="$GPUI_SYSROOT/usr/lib/x86_64-linux-gnu"
export PKG_CONFIG_PATH="$GPUI_SYSROOT/usr/lib/x86_64-linux-gnu/pkgconfig:$GPUI_SYSROOT/usr/share/pkgconfig"
export LIBCLANG_PATH="$GPUI_SYSROOT/usr/lib/llvm-19/lib"
export CMAKE_ROOT="$GPUI_SYSROOT/usr/share/cmake-3.31"
export CARGO_BUILD_JOBS=4
exec "$@"
