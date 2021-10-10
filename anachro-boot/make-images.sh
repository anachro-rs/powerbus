#!/bin/bash

set -euxo pipefail

cp memory.x memory-bl.x
cp memory-app.x memory.x

touch src/bin/blink_1.rs
touch src/bin/blink_2.rs

cargo build --release --bin blink_1
cargo build --release --bin blink_2

cp target/thumbv7em-none-eabihf/release/blink_1 ./test-images/blink_1_app
cp target/thumbv7em-none-eabihf/release/blink_2 ./test-images/blink_2_app

arm-none-eabi-objcopy -O ihex ./test-images/blink_1_app ./test-images/blink_1_app.hex
arm-none-eabi-objcopy -O ihex ./test-images/blink_2_app ./test-images/blink_2_app.hex

arm-none-eabi-objcopy -O binary ./test-images/blink_1_app ./test-images/blink_1_app.bin
arm-none-eabi-objcopy -O binary ./test-images/blink_2_app ./test-images/blink_2_app.bin

cd ../boot-chonker

cargo run --release -- ../anachro-boot/test-images/blink_1_app.bin ../anachro-boot/test-images/blink_1_app.toml ../anachro-boot/test-images/blink_1_app_combo.bin
cargo run --release -- ../anachro-boot/test-images/blink_2_app.bin ../anachro-boot/test-images/blink_2_app.toml ../anachro-boot/test-images/blink_2_app_combo.bin

cd ../anachro-boot

cp memory.x memory-app.x
cp memory-bl.x memory.x
