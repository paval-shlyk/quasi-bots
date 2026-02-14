lint:
    cargo fmt --all --check
    cargo clippy --all --all-targets --all-features -- -D warnings

fix:
    cargo fmt
    __CARGO_FIX_YOLO=1 cargo clippy --all-targets --all-features --fix --allow-dirty

arm64-build:
    docker buildx build -t paval-shlyk/quasi-bots/skill-master:latest -f skill-master/docker/arm64.Dockerfile .
    cargo deb -p skill-master --target=aarch64-unknown-linux-gnu --no-build
