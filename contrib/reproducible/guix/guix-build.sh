#!/usr/bin/env sh

set -ex

# How many cores to allocate to Guix building.
JOBS="${JOBS:-$(nproc)}"

# The binary to check the hash of downloaded archives.
SHASUM_BIN="${SHASUM_BIN:-sha256sum}"

# We do everything in a single directory. That's the root of it, configurable
# through the environment.
BUILD_ROOT="${BUILD_ROOT:-$(mktemp -d)}"

# Various folders we expose to the container. The vendor directory will contain
# the sources of all our dependencies. Because we restrict network access from
# within the container, this is pulled beforehand.
# The out directory will contain the resulting binaries. It's wired to the --target-dir
# for a cargo build.
VENDOR_DIR="$BUILD_ROOT/vendor"
OUT_DIR="${OUT_DIR:-"$BUILD_ROOT/out"}"
BIN_DIR="${BIN_DIR:-"$BUILD_ROOT/bin"}"

# Create the directory if it doesn't exist already
maybe_create_dir() {
    if ! [ -d "$@" ]; then
        mkdir -p "$@"
    fi
}
maybe_create_dir "$BIN_DIR"

# That's what Guix comes with.
RUST_VERSION="1.60.0"
CARGO_BIN="$BIN_DIR/cargo"
MUSL_STDLIB="$BIN_DIR/musl_stdlib"
MUSL_RUSTC_DIR="$BIN_DIR/musl_rustc"

# First off get the cargo binary to run on the host to vendor dependencies.
# We assume the host is a 64bit Linux system.
if ! [ -f "$CARGO_BIN" ]; then
    test -f "$ARCHIVE_PATH" || ARCHIVE_PATH="$BIN_DIR/rust-for-cargo.tar.gz"
    curl -o "$ARCHIVE_PATH" "https://static.rust-lang.org/dist/rust-$RUST_VERSION-x86_64-unknown-linux-gnu.tar.gz"
    echo "b8a4c3959367d053825e31f90a5eb86418eb0d80cacda52bfa80b078e18150d5 $ARCHIVE_PATH" | $SHASUM_BIN -c
    # Path of the cargo binary within the archive
    CARGO_BIN_PATH="rust-$RUST_VERSION-x86_64-unknown-linux-gnu/cargo/bin/cargo"
    ( cd $BIN_DIR && tar -xzf $ARCHIVE_PATH $CARGO_BIN_PATH && mv $CARGO_BIN_PATH $CARGO_BIN )
fi

# Then get the Rust stdlib for musl.
if ! [ -d "$MUSL_STDLIB" ] || ! [ -f "$MUSL_RUSTC_DIR" ]; then
    ARCHIVE_PATH="$BIN_DIR/rust-for-musl.tar.gz"
    test -f "$ARCHIVE_PATH" || curl -o "$ARCHIVE_PATH" "https://static.rust-lang.org/dist/rust-$RUST_VERSION-x86_64-unknown-linux-musl.tar.gz"
    #echo "b8a4c3959367d053825e31f90a5eb86418eb0d80cacda52bfa80b078e18150d5 $ARCHIVE_PATH" | $SHASUM_BIN -c
    # Path of the compiled stdlib within the archive
    STDLIB_PATH="rust-$RUST_VERSION-x86_64-unknown-linux-musl/rust-std-x86_64-unknown-linux-musl/lib/rustlib/x86_64-unknown-linux-musl/lib/"
    RUSTC_DIR_PATH="rust-$RUST_VERSION-x86_64-unknown-linux-musl/rustc"
    (
        cd "$BIN_DIR"
        if ! [ -d "$MUSL_STDLIB" ]; then
            tar -xzf "$ARCHIVE_PATH" "$STDLIB_PATH"
            mv "$STDLIB_PATH" "$MUSL_STDLIB"
        fi
        if ! [ -d "$MUSL_RUSTC_DIR" ]; then
            tar -xzf "$ARCHIVE_PATH" "$RUSTC_DIR_PATH"
            mv "$RUSTC_DIR_PATH" "$MUSL_RUSTC_DIR"
        fi
    )
    ls
    ls $MUSL_RUSTC_DIR
fi

# Execute "$@" in a pinned, possibly older version of Guix, for reproducibility
# across time.
time_machine() {
    guix time-machine --url=https://git.savannah.gnu.org/git/guix.git \
                      --commit=059d38dc3f8b087f4a42df586daeb05761ee18d7 \
                      --cores="$JOBS" \
                      --keep-failed \
                      --fallback \
                      -- "$@"
}

# Now build the daemon.

# Start by pulling the dependencies, we'll build them in the container.
test -d "$VENDOR_DIR" || $CARGO_BIN vendor "$VENDOR_DIR"

# Bootstrap a reproducible environment as specified by the manifest in an isolated
# container, and build the project.
IS_GUI=0 time_machine shell --no-cwd \
           --expose="$PWD/src=/liana/src" \
           --expose="$PWD/Cargo.toml=/liana/Cargo.toml" \
           --expose="$PWD/Cargo.lock=/liana/Cargo.lock" \
           --expose="$PWD/contrib/reproducible/guix/build.sh=/liana/build.sh" \
           --expose="$MUSL_STDLIB=/liana/musl_stdlib" \
           --expose="$MUSL_RUSTC_DIR=/liana/musl_rustc" \
           --expose="$VENDOR_DIR=/vendor" \
           --share="$OUT_DIR=/out" \
           --cores="$JOBS" \
           --container \
           --pure \
           --fallback \
           --rebuild-cache \
           -m "$PWD/contrib/reproducible/guix/manifest.scm" \
           -- env CC=gcc VENDOR_DIR="$VENDOR_DIR" TARGET_DIR="$OUT_DIR" BINARY_NAME="lianad" JOBS="$JOBS" \
              /bin/sh -c "cd /liana && ./build.sh"

# Now build the GUI.
GUI_ROOT_DIR="$PWD/gui"
GUI_VENDOR_DIR="$VENDOR_DIR/gui"
GUI_OUT_DIR="$OUT_DIR/gui"
GUI_PATCHES_DIR="$PWD/contrib/reproducible/guix/patches/gui"

# Again, start by vendoring the dependencies. But in addition here the GUI sources need to
# be patched in order to be able to build it with the current Rust version (its MSRV is insane).
if ! [ -d "$GUI_VENDOR_DIR" ]; then
    # Download the dependencies
    ( cd "./gui" && $CARGO_BIN vendor "$GUI_VENDOR_DIR" )

    # Patch the dependencies as needed.
    (
        cd "$GUI_VENDOR_DIR"
        for patch_file in $(ls "$GUI_PATCHES_DIR"); do
            patch -p1 < "$GUI_PATCHES_DIR/$patch_file"
        done
    )

    # Some of the checksums will be incorrect. Instead of cherry-picking remove them
    # altogether, since they aren't useful anyways (see comment below).
    for dep in $(ls "$GUI_VENDOR_DIR"); do
        echo "{\"files\":{}}" > "$GUI_VENDOR_DIR/$dep/.cargo-checksum.json"
    done
fi

# Remove the checksums from the Cargo.lock. In the container `cargo rustc` would compare
# them against the .cargo-checksum.json to make sure they weren't tampered with since they
# where vendored. But we just removed the checksums from the .cargo-checksum.json.
# There is little point in checking integrity between the above vendor step and now anyways.
# What matters is checking integrity after downloading the crates from the internet and
# `cargo vendor` does that already.
cp "$GUI_ROOT_DIR/Cargo.lock" "$BUILD_ROOT/Cargo.lock"
sed -i '/checksum/d' "$BUILD_ROOT/Cargo.lock"

# Bootstrap a reproducible environment as specified by the manifest in an isolated
# container, and build the project.
# NOTE: it looks like "--rebuild-cache" is necessary for the BINARY_NAME variable to
# be taken into account when building the container (otherwise the GUI container could
# miss some dependencies).
IS_GUI=1 time_machine shell --no-cwd \
           --expose="$GUI_ROOT_DIR/src=/liana/src" \
           --expose="$GUI_ROOT_DIR/static=/liana/static" \
           --expose="$GUI_ROOT_DIR/Cargo.toml=/liana/Cargo.toml" \
           --expose="$BUILD_ROOT/Cargo.lock=/liana/Cargo.lock" \
           --expose="$MUSL_STDLIB=/liana/musl_stdlib" \
           --expose="$PWD/contrib/reproducible/guix/build.sh=/liana/build.sh" \
           --expose="$GUI_VENDOR_DIR=/vendor" \
           --share="$GUI_OUT_DIR=/out" \
           --cores="$JOBS" \
           --container \
           --pure \
           --fallback \
           --rebuild-cache \
           -m "$PWD/contrib/reproducible/guix/manifest.scm" \
           -- env CC=gcc VENDOR_DIR="$GUI_VENDOR_DIR" TARGET_DIR="$GUI_OUT_DIR" BINARY_NAME="liana-gui" JOBS="$JOBS" \
              /bin/sh -c "cd /liana && ./build.sh"


set +ex

echo "Build successful. Output available at $OUT_DIR"
