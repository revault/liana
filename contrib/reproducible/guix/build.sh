# ==========================================================================
# The script ran within the GUIX container to build the Liana daemon or GUI.
# ==========================================================================

set -ex

# Instruct cargo to use our vendored sources
mkdir -p .cargo
cat <<EOF >.cargo/config.toml
[source.vendored_sources]
directory = "/vendor"

[source.crates-io]
replace-with = "vendored_sources"

[source."https://github.com/darosior/rust-miniscript"]
git = "https://github.com/darosior/rust-miniscript"
branch = "multipath_descriptors_on_9.0"
replace-with = "vendored_sources"

[source."https://github.com/revault/liana"]
git = "https://github.com/revault/liana"
branch = "master"
replace-with = "vendored_sources"
EOF

cat <<EOF >rustc_musl_wrapper.sh
#!/bin/sh
LD_LIBRARY_PATH="$LD_LIBRARY_PATH:$PWD/musl_rustc/lib" "$PWD/musl_rustc/bin/rustc" "\$@"
EOF
chmod +x rustc_musl_wrapper.sh

./rustc_musl_wrapper.sh -vV

cargo clean
RUSTC_BOOTSTRAP=1 RUSTC="$PWD/rustc_musl_wrapper.sh" cargo -vvv \
    --color always \
    --frozen \
    --offline \
    -Zbuild-std \
    rustc \
    --jobs "$JOBS" \
    --release \
    --target-dir "/out" \
    --target x86_64-unknown-linux-musl

# We need to set RUSTC_BOOTSTRAP=1 as a workaround to be able to use unstable
# features in the GUI dependencies
RUSTC_BOOTSTRAP=1 cargo -vvv \
    --color always \
    --frozen \
    --offline \
    rustc \
    --jobs "$JOBS" \
    --release \
    --target-dir "/out"

# Assume 64bits. Even bitcoind doesn't ship 32bits binaries for x86.
# FIXME: is there a cleaner way than using patchelf for this?
patchelf --set-interpreter /lib64/ld-linux-x86-64.so.2 "/out/release/$BINARY_NAME"

# FIXME: Find a way to use GUIX_LD_WRAPPER_DISABLE_RPATH=yes instead
patchelf --remove-rpath "/out/release/$BINARY_NAME"

set +ex
