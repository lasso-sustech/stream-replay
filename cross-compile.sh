#!/usr/bin/bash

cargo build --target aarch64-linux-android --release
cp -f target/aarch64-linux-android/release/libreplay.so ../app/src/main/jni/arm64/libreplay.so
