lint:
    cargo fmt --all --check
    cargo clippy --all --all-targets --all-features -- -D warnings

fix:
    cargo fmt
    __CARGO_FIX_YOLO=1 cargo clippy --all-targets --all-features --fix --allow-dirty
