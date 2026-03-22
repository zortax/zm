# zm

A desktop email client built with Rust and [GPUI](https://github.com/zed-industries/zed). Early stage, expect rough edges.


![zm](https://github.com/user-attachments/assets/00aff6c7-ad29-4705-adbf-328ecd99b541)

## Features

- Multi-account IMAP/SMTP support
- Real-time sync via IMAP IDLE
- Semantic search using local ML embeddings (multilingual-e5-small via ONNX)
- Secure credential storage via system keyring
- Configurable themes

## Building

Requires a working Rust toolchain.

```sh
cargo build --release
```

For GPU-accelerated semantic search (requires system ONNX Runtime with CUDA):

```sh
cargo build --release --features cuda
```

## Configuration

Config file lives at `~/.config/zm/zm.toml`. Accounts can be added through the setup wizard or by editing the file directly.

## License

GPL-3.0-or-later
