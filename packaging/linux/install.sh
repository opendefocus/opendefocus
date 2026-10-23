#!/bin/sh
# Installs OpenDefocus.ofx.bundle from the directory containing this script into
# $DESTDIR$PREFIX/OFX/Plugins (default /usr/OFX/Plugins).
#
# Usage: ./install.sh [--prefix DIR]
#   DESTDIR  optional staging root prepended to the prefix (for packagers)
#   PREFIX   install prefix, default /usr; --prefix DIR overrides it

set -eu

BUNDLE_NAME=OpenDefocus.ofx.bundle
PREFIX=${PREFIX:-/usr}

usage() {
    echo "Usage: $0 [--prefix DIR]"
    echo "Installs $BUNDLE_NAME into \${DESTDIR}\${PREFIX}/OFX/Plugins (default /usr/OFX/Plugins)."
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

script_dir=$(cd "$(dirname "$0")" && pwd)
source_bundle="$script_dir/$BUNDLE_NAME"
if [ ! -d "$source_bundle" ]; then
    echo "error: $BUNDLE_NAME not found next to this script" >&2
    exit 1
fi

target_dir="${DESTDIR:-}${PREFIX}/OFX/Plugins"
mkdir -p "$target_dir"
rm -rf "${target_dir:?}/$BUNDLE_NAME"
cp -R "$source_bundle" "$target_dir/"
echo "Installed $BUNDLE_NAME to $target_dir"
