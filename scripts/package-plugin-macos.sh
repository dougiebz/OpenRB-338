#!/usr/bin/env bash
# Builds the plugin (crates/orb-vst) for one macOS target and makes its three bundles in
# dist/<target>/plugin/:
#
#   OpenRB-338.clap        the CLAP plugin
#   OpenRB-338.vst3        the VST3 plugin (the same library)
#   OpenRB-338.component   the Audio Unit: free-audio's clap-wrapper around the .clap, which is
#                          copied inside it (crates/orb-vst/au/CMakeLists.txt)
#
#   scripts/package-plugin-macos.sh aarch64-apple-darwin|x86_64-apple-darwin
#
# Needs a Mac with Xcode's command line tools and cmake 3.21 or newer. SKIP_AU=1 leaves the
# Audio Unit out. Everything is ad-hoc signed only, as the app is (docs/building.md).
# To try the Audio Unit: copy OpenRB-338.component to ~/Library/Audio/Plug-Ins/Components/ and
# run `auval -v aumu Rb38 Orb3`.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
target=${1:?usage: scripts/package-plugin-macos.sh aarch64-apple-darwin|x86_64-apple-darwin}
case $target in
    aarch64-apple-darwin) cmake_arch=arm64 ;;
    x86_64-apple-darwin) cmake_arch=x86_64 ;;
    *) echo "not a macOS target: $target" >&2; exit 2 ;;
esac
[ "$(uname -s)" = Darwin ] || { echo "the plugin bundles are made on macOS" >&2; exit 2; }

name=OpenRB-338
bundle_id=net.emphased.openrb338
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -n 1)
dist=${DIST:-$root/dist}
out=$dist/$target/plugin
target_dir=${CARGO_TARGET_DIR:-$root/target}
export MACOSX_DEPLOYMENT_TARGET=11.0

rustup target add "$target" >/dev/null
(cd "$root" && cargo build --release --locked --target "$target" -p orb-vst)
lib=$target_dir/$target/release/libopenrb338.dylib
[ -f "$lib" ] || { echo "no $lib" >&2; exit 1; }

rm -rf "$out"
mkdir -p "$out"

# A macOS plugin bundle: Contents/MacOS/<name>, Info.plist, PkgInfo.
bundle() {
    local dir=$1 id=$2
    mkdir -p "$dir/Contents/MacOS"
    cp "$lib" "$dir/Contents/MacOS/$name"
    printf 'BNDL????' > "$dir/Contents/PkgInfo"
    cat > "$dir/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>$name</string>
    <key>CFBundleIdentifier</key><string>$id</string>
    <key>CFBundleName</key><string>$name</string>
    <key>CFBundleDisplayName</key><string>$name</string>
    <key>CFBundlePackageType</key><string>BNDL</string>
    <key>CFBundleSignature</key><string>????</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>$MACOSX_DEPLOYMENT_TARGET</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF
    codesign --force -s - "$dir"
}

bundle "$out/$name.clap" "$bundle_id.clap"
bundle "$out/$name.vst3" "$bundle_id.vst3"

if [ "${SKIP_AU:-0}" != 1 ]; then
    build=$target_dir/$target/au
    cmake -S "$root/crates/orb-vst/au" -B "$build" \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_OSX_ARCHITECTURES="$cmake_arch" \
        -DOPENRB_CLAP="$out/$name.clap"
    cmake --build "$build" --config Release
    component=$(find "$build" -maxdepth 3 -name "$name.component" -type d | head -n 1)
    [ -n "$component" ] || { echo "cmake built no $name.component" >&2; exit 1; }
    cp -R "$component" "$out/$name.component"
    codesign --force --deep -s - "$out/$name.component"
    codesign --verify --deep --strict "$out/$name.component"
fi

echo "built:"
ls -d "$out"/*
