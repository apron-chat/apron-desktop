#!/bin/sh
# Workspace-only dependencies; ordinary/Nix installs do not need this file.
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
export LD_LIBRARY_PATH="$ROOT/.deps/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export GI_TYPELIB_PATH="$ROOT/.deps/usr/lib/x86_64-linux-gnu/girepository-1.0${GI_TYPELIB_PATH:+:$GI_TYPELIB_PATH}"
export XDG_DATA_DIRS="$ROOT/.deps/usr/share:/usr/local/share:/usr/share"
export PYTHONPATH="$ROOT:$ROOT/.deps/usr/lib/python3/dist-packages${PYTHONPATH:+:$PYTHONPATH}"
exec "$@"
