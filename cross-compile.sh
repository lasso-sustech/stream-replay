#!/usr/bin/bash

cargo build --target aarch64-linux-android --release
ln -sf target/aarch64-linux-android/release/libreplay.so ../app/src/main/jniLibs/arm64/libreplay.so
