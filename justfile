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

test-install:
    #!/bin/bash
    docker buildx build -t paval-shlyk/quasi-bots/skill-master:latest -f skill-master/docker/Dockerfile .

    # Build the package (no compilation, just assets)
    cargo deb -p skill-master --no-build

    # Clean up previous test container
    docker rm -f skill-master-test 2>/dev/null || true

    echo "Running systemd container..."
    # Run privileged container with systemd
    docker run -it --rm -d --name skill-master-test --privileged \
        --cgroupns=host \
        -v /sys/fs/cgroup:/sys/fs/cgroup:rw \
        -v $(pwd):/repo \
        jrei/systemd-ubuntu:24.04 \
        /bin/bash

    echo "Installing package..."
    # Install dependencies and our package
    docker exec skill-master-test bash -c "apt-get update && apt-get install -y /repo/target/debian/skill-master_*.deb"
    
    echo ""
    echo "---------------------------------------------------"
    echo "✅ Installation complete!"
    echo "To test the service:"
    echo "  1. Enter container: docker exec -it skill-master-test bash"
    echo "  2. Edit config:     vi /etc/skill-master/config.toml"
    echo "  3. Start service:   systemctl start skill-master"
    echo "---------------------------------------------------"


reset-db:
    #!/bin/bash
    export DATABASE_URL="sqlite://$(pwd)/scrapper.db"

    sqlx database drop
    sqlx database create
    sqlx migrate run --source=./skill-master/migrations

    cargo sqlx prepare --workspace -- --all-features --all-targets --all
