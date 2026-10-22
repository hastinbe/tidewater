#!/bin/sh
# Builds the iOS app's pieces (docs/design/ios.md).
#
#   platform/ios/build.sh [sim|device|all] [--app]
#
# sim (the default): the Rust static library for the Simulator (aarch64-apple-ios-sim).
# device:            the library for a device (aarch64-apple-ios).
# all:               both.
# --app:             also build Tidewater.app for the Simulator with xcodebuild (no signing needed), into
#                    platform/ios/DerivedData/.
#
# The libraries go to platform/ios/build/iphonesimulator/ and build/iphoneos/, where the Xcode project looks for them.
# Then xcodegen turns project.yml into Tidewater.xcodeproj. Needs Xcode, xcodegen, CMake and rustup.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
what=sim
app=no
for arg in "$@"; do
    case "$arg" in
        sim | device | all) what=$arg ;;
        --app) app=yes ;;
        *)
            echo "usage: $0 [sim|device|all] [--app]" >&2
            exit 2
            ;;
    esac
done

# The same minimum iOS as project.yml, for SDL's C code and Rust's own.
export IPHONEOS_DEPLOYMENT_TARGET=15.0

build() { # build TARGET SDK-DIR
    echo "==> library for $1"
    rustup target add "$1" >/dev/null
    (cd "$root" && cargo build --release --target "$1" -p tidewater-ios)
    mkdir -p "$here/build/$2"
    cp "$root/target/$1/release/libtidewater_ios.a" "$here/build/$2/libtidewater_ios.a"
}

case "$what" in
    sim | all) build aarch64-apple-ios-sim iphonesimulator ;;
esac
case "$what" in
    device | all) build aarch64-apple-ios iphoneos ;;
esac

echo "==> Xcode project"
(cd "$here" && xcodegen generate --quiet)

if [ "$app" = yes ]; then
    echo "==> Tidewater.app for the Simulator"
    (cd "$here" && xcodebuild -project Tidewater.xcodeproj -scheme Tidewater -configuration Debug \
        -sdk iphonesimulator -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build -quiet)
    echo "built: $here/DerivedData/Build/Products/Debug-iphonesimulator/Tidewater.app"
fi
