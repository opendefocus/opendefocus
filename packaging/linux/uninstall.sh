#!/bin/sh
# Removes OpenDefocus.ofx.bundle from $DESTDIR$PREFIX/OFX/Plugins (default /usr/OFX/Plugins).
#
# Usage: ./uninstall.sh [--prefix DIR]
#   DESTDIR  optional staging root prepended to the prefix (for packagers)
#   PREFIX   install prefix, default /usr; --prefix DIR overrides it

set -eu

BUNDLE_NAME=OpenDefocus.ofx.bundle
PREFIX=${PREFIX:-/usr}

usage() {
    echo "Usage: $0 [--prefix DIR]"
    echo "Removes $BUNDLE_NAME from \${DESTDIR}\${PREFIX}/OFX/Plugins (default /usr/OFX/Plugins)."
}

while [ $# -gt 0 ]; do
    case $1 in
        --prefix)
            if [ $# -lt 2 ]; then
                echo "error: --prefix needs a directory" >&2
                exit 2
            fi
            PREFIX=$2
            shift 2
            ;;
        --prefix=*)
            PREFIX=${1#--prefix=}
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            echo "error: unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

target="${DESTDIR:-}${PREFIX}/OFX/Plugins/$BUNDLE_NAME"
if [ -e "$target" ]; then
    rm -rf "${target:?}"
    echo "Removed $target"
else
    echo "Nothing to remove: $target does not exist"
fi
