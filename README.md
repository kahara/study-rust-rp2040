# study-rust-rp2040
Waiting for more hardware to arrive for Pico probing. Meanwhile using
[pico-debug](https://github.com/majbthrd/pico-debug/).

## Prerequisites

```console
apt install gcc-arm-none-eabi gdb-multiarch
rustup target add thumbv6m-none-eabi
rustup component add llvm-tools-preview
cargo install uf2conv cargo-binutils
```

## udev rules

```
# E.g., /etc/udev/rules.d/80-picoprobe.rules
ATTRS{idVendor}=="2e8a", ATTRS{idProduct}=="0004", MODE="660", GROUP="plugdev", TAG+="uaccess"
ATTRS{idVendor}=="1209", ATTRS{idProduct}=="2488", MODE="660", GROUP="plugdev", TAG+="uaccess"
```

## pico-debug

Prep an ELF format file for debugging:

```console
cargo objcopy -- -O elf32-littlearm study-rust-rp2040.elf
```

Run `pico-debug` and connect with `gdb`:

```console
curl -L -o /path/to/mounted/pico/fs/pico-debug-gimmecache.uf2 https://github.com/majbthrd/pico-debug/releases/download/v10.03/pico-debug-gimmecache.uf2
openocd -f interface/cmsis-dap.cfg -c "transport select swd" -c "adapter speed 4000" -f target/rp2040-core0.cfg
gdb-multiarch study-rust-rp2040.elf
```

In `gdb`:

```
target extended-remote localhost:3333
load
monitor reset init  # still halted here
continue  # run the program
```

## picoprobe

**Not tested yet, lack of hardware.**

Build
[picoprobe](https://github.com/raspberrypi/picoprobe)
and load it to a Raspberry Pi Pico board. When building, use the _"automatic download from GitHub"_ option for
Raspberry Pi Pico SDK, described in
["Quick-start your own project"](https://github.com/raspberrypi/pico-sdk/blob/master/README.md#quick-start-your-own-project):

```
diff --git a/CMakeLists.txt b/CMakeLists.txt
index 252bf4e..e9f5555 100644
--- a/CMakeLists.txt
+++ b/CMakeLists.txt
@@ -1,5 +1,7 @@
 cmake_minimum_required(VERSION 3.12)
 
+set(PICO_SDK_FETCH_FROM_GIT on)
+
 include(pico_sdk_import.cmake)
 
 project(picoprobe)
```

To build `picoprobe`:

```console
mkdir build
cd build
cmake ..
make
./elf2uf2/elf2uf2 picoprobe.elf picoprobe.uf2
```

Then copy `picoprobe.uf2` to the Pico board.


Connect with `gdb`:

```console
openocd -f interface/cmsis-dap.cfg -c "transport select swd" -c "adapter speed 4000" -f target/rp2040.cfg
gdb-multiarch study-rust-rp2040.elf
```

In `gdb`:

```
target extended-remote localhost:3333
load
monitor reset init  # still halted here
continue  # run the program
```

## probe-run

```console
cargo install --git https://github.com/rp-rs/probe-run --branch main
```

Then,

```console
cargo run
```

This runs `probe-run-rp --chip RP2040 target/thumbv6m-none-eabi/debug/study-rust-rp2040` automagically.

## Deploy

To create a flash-able artifact:
```console
cargo build --release
cargo objcopy --release -- -O binary study-rust-rp2040.bin
uf2conv study-rust-rp2040.bin --base 0x10000000 --family 0xe48bff56 --output study-rust-rp2040.uf2
```
