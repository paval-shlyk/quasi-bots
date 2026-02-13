lint:
    cargo fmt --all --check
    cargo clippy --all-targets -- -D warnings

fix:
    cargo fmt
    __CARGO_FIX_YOLO=1 cargo clippy --all-targets --all-features --fix --allow-dirty
