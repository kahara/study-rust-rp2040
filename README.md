# study-rust-rp2040
Waiting for more hardware to arrive for Pico probing.

tldr;

```console
apt install gcc-arm-none-eabi
rustup target add thumbv6m-none-eabi
cargo install --git https://github.com/rp-rs/probe-run --branch main
```

To create a flash-able artifact:
```console
cargo install uf2conv cargo-binutils
rustup component add llvm-tools-preview
cargo build --release
cargo objcopy --release -- -O binary study-rust-rp2040.bin
uf2conv study-rust-rp2040.bin --base 0x10000000 --family 0xe48bff56 --output study-rust-rp2040.uf2
```
