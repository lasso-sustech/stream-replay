#!/usr/bin/bash

cargo build --target aarch64-linux-android --release
mkdir -p ../app/src/main/jniLibs/arm64-v8a
cp -f target/aarch64-linux-android/release/libreplay.so ../app/src/main/jniLibs/arm64-v8a/libreplay.so