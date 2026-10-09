# Rust UI Engine

An experimental native desktop UI framework written in Rust, targeting Windows,
Linux (X11 and Wayland), and macOS. Normal rendering uses a hardware GPU through
`wgpu`; the UI model is independent of the graphics backend.

The repository currently contains a research prototype: rectangle scene
primitives, an instanced GPU renderer, and a native window example with selection
and scrolling over 100,000 logical row indices. The rectangles are visual
placeholders. Ribbon, TreeGrid, text rendering, layout, IME, and accessibility
are planned features.

## Workspace

| Path | Purpose |
| --- | --- |
| `crates/ui-core` | Renderer-independent geometry, scene, demo state and unit tests |
| `crates/ui-render-wgpu` | Experimental GPU rectangle renderer |
| `examples/gpu-shell` | Native window and input demonstration |
| `docs/public` | Architecture and product requirements |

## Development

Install a Rust toolchain supporting edition 2024, then run:

```sh
cargo run --release -p gpu-shell
```

Validation commands:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

The imported prototype has not yet passed the full build and GPU runtime audit.
Dependency versions, window lifecycle, device recovery, and platform support
still need verification. No cross-platform runtime or performance result is
claimed. A compatible hardware GPU is required.

See [architecture](docs/public/architecture.md) and
[requirements](docs/public/requirements.md).

## License

The project is licensed under [Apache-2.0](LICENSE). Third-party dependencies
retain their own licenses; a complete dependency and resource audit is pending.
