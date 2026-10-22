#!/bin/sh
# Builds the Android app (docs/design/android.md).
#
#   platform/android/build.sh [--abi ABI]... [--libs-only]
#
# Builds libmain.so for each ABI (arm64-v8a and x86_64 unless --abi says otherwise) with cargo-ndk, copies SDL's Java
# sources from the SDL that sdl3-sys builds, then runs Gradle to make a debug APK:
#   platform/android/app/build/outputs/apk/debug/app-debug.apk
# --libs-only stops after the libraries.
#
# Needs rustup (the Android targets are added by the script), cargo-ndk, CMake, a JDK that Gradle 8.14 runs on (17 to
# 24; set JAVA_HOME, or the script looks in Homebrew's openjdk folders) and the Android SDK with an NDK and
# platforms;android-35 (ANDROID_HOME, or ~/Library/Android/sdk and ~/Android/Sdk). The NDK is ANDROID_NDK_HOME or the
# newest one in the SDK.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
abis=""
libs_only=no
while [ $# -gt 0 ]; do
    case "$1" in
        --abi)
            abis="$abis $2"
            shift 2
            ;;
        --libs-only)
            libs_only=yes
            shift
            ;;
        *)
            echo "usage: $0 [--abi ABI]... [--libs-only]" >&2
            exit 2
            ;;
    esac
done
[ -n "$abis" ] || abis="arm64-v8a x86_64"

# The same minimum Android as app/build.gradle (minSdk).
api=24

# The Android SDK and NDK.
if [ -z "${ANDROID_HOME:-}" ]; then
    for d in "${ANDROID_SDK_ROOT:-}" "$HOME/Library/Android/sdk" "$HOME/Android/Sdk"; do
        if [ -n "$d" ] && [ -d "$d" ]; then
            ANDROID_HOME=$d
            break
        fi
    done
fi
[ -n "${ANDROID_HOME:-}" ] || { echo "no Android SDK: set ANDROID_HOME" >&2; exit 1; }
export ANDROID_HOME
if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    ANDROID_NDK_HOME=$(ls -d "$ANDROID_HOME"/ndk/*/ 2>/dev/null | sort -V | tail -n 1)
    ANDROID_NDK_HOME=${ANDROID_NDK_HOME%/}
fi
[ -d "${ANDROID_NDK_HOME:-}" ] || { echo "no Android NDK: install one or set ANDROID_NDK_HOME" >&2; exit 1; }
export ANDROID_NDK_HOME

# SDL's CMake build must use the NDK's toolchain file; with only the NDK's location it picks the wrong processor name.
export CMAKE_TOOLCHAIN_FILE="$ANDROID_NDK_HOME/build/cmake/android.toolchain.cmake"

# Gradle 8.14 runs on JDK 17 to 24. If JAVA_HOME is not set, use a Homebrew JDK when there is one.
if [ -z "${JAVA_HOME:-}" ]; then
    for d in /opt/homebrew/opt/openjdk@24 /opt/homebrew/opt/openjdk@21 /opt/homebrew/opt/openjdk@17 /opt/homebrew/opt/openjdk; do
        if [ -d "$d/libexec/openjdk.jdk/Contents/Home" ]; then
            JAVA_HOME=$d/libexec/openjdk.jdk/Contents/Home
            break
        fi
    done
fi
[ -z "${JAVA_HOME:-}" ] || export JAVA_HOME

targets=""
for abi in $abis; do
    case "$abi" in
        arm64-v8a) targets="$targets aarch64-linux-android" ;;
        x86_64) targets="$targets x86_64-linux-android" ;;
        *)
            echo "unsupported ABI $abi (arm64-v8a or x86_64)" >&2
            exit 2
            ;;
    esac
done
# shellcheck disable=SC2086
rustup target add $targets >/dev/null

echo "==> libmain.so for:$abis"
ndk_args=""
for abi in $abis; do
    ndk_args="$ndk_args -t $abi"
done
rm -rf "$here/app/src/main/jniLibs"
# shellcheck disable=SC2086
(cd "$root" && cargo ndk $ndk_args -P "$api" -o "$here/app/src/main/jniLibs" build --release -p tidewater-android)

if [ "$libs_only" = yes ]; then
    exit 0
fi

# SDL's Java sources (zlib licence, the header of each file) come from the SDL that sdl3-sys builds, so the Java and C
# sides always match. They are not kept in this repository.
echo "==> SDL's Java sources"
sdl_src=$(cd "$root" && cargo metadata --format-version 1 --locked |
    python3 -c 'import json, sys
for p in json.load(sys.stdin)["packages"]:
    if p["name"] == "sdl3-src":
        print(p["manifest_path"].rsplit("/", 1)[0])')
java_from="$sdl_src/SDL/android-project/app/src/main/java/org/libsdl/app"
[ -d "$java_from" ] || { echo "SDL's Java sources not found under $sdl_src" >&2; exit 1; }
rm -rf "$here/app/src/main/java/org"
mkdir -p "$here/app/src/main/java/org/libsdl"
cp -R "$java_from" "$here/app/src/main/java/org/libsdl/app"

echo "==> debug APK"
(cd "$here" && ./gradlew --no-daemon -q assembleDebug)
echo "built: $here/app/build/outputs/apk/debug/app-debug.apk"
