check:
    cargo check

fmt:
    cargo fmt

clippy:
    cargo clippy

lint: fmt check clippy
