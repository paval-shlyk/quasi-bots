lint:
    #!/bin/bash
    export SQLX_OFFLINE=true

    cargo fmt --all --check
    if [ $? -ne 0 ]; then
        echo "Code is not formatted. Please run 'just fix' to format the code."
        exit 1
    fi

    cargo clippy --all --all-targets --all-features -- -D warnings
    if [ $? -ne 0 ]; then
        echo "Clippy found issues. Please run 'just fix' to fix the issues."
        exit 1
    fi

fix:
    cargo fmt
    __CARGO_FIX_YOLO=1 cargo clippy --all-targets --all-features --fix --allow-dirty

arm64-build:
    docker buildx build -t paval-shlyk/quasi-bots/skill-master:latest -f skill-master/docker/arm64.Dockerfile .
    cargo deb -p skill-master --target=aarch64-unknown-linux-gnu --no-build


reset-db:
    #!/bin/bash
    export DATABASE_URL="sqlite://$(pwd)/scrapper.db"

    sqlx database drop
    sqlx database create
    sqlx migrate run --source=./skill-master/migrations

    cargo sqlx prepare --workspace -- --all-features --all-targets --all
