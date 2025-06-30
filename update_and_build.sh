#!/usr/bin/env bash

set -euo pipefail

# Downloading caddy
if [ ! -f ./caddy ]; then
    echo "Downloading caddy..."
    curl -L "https://github.com/caddyserver/caddy/releases/download/v2.10.0/caddy_2.10.0_linux_amd64.tar.gz" | tar xz
fi

#  Run the template provider in the background
echo "Starting template provider..."
cd template-provider
./entrypoint.sh
cd -

REPO_DIR=./stratum
REPO_URL="https://github.com/stratum-mining/stratum.git"
BRANCH="main"
COMMIT_FILE="last_commit.txt"

# # To track running services
PIDS=()

# # Cleanup function with optional "exit" flag
cleanup() {
    echo "Stopping running services..."
    for pid in "${PIDS[@]}"; do
        echo "Killing process $pid"
        kill "$pid" 2>/dev/null || true
    done
    PIDS=()  # Clear PID list
    wait 2>/dev/null || true

    if [ "${1:-}" == "exit" ]; then
        echo "Exiting."
        exit 0
    fi
}

trap 'cleanup exit' SIGINT SIGTERM

# Clone or pull repo
if [ ! -d "$REPO_DIR/.git" ]; then
    git clone "$REPO_URL" "$REPO_DIR"
else
    (cd "$REPO_DIR" && git pull origin "$BRANCH")
fi

# Start Caddy server
echo "Starting Caddy server..."
sudo ./caddy run --config ./Caddyfile --adapter caddyfile > ./caddy.log 2>&1 &
PIDS+=($!)

# Initial service start (always run at least once)
start_services() {
    # Run services in background and collect their PIDs
    cd "$REPO_DIR/roles/pool/config-examples"
    cargo run --release -- -c pool-config-local-tp-example.toml > ../../../../pool.log 2>&1 &
    PIDS+=($!)
    cd -

    cd "$REPO_DIR/roles/jd-server/config-examples"
    cargo run --release -- -c jds-config-local-example.toml > ../../../../jd-server.log 2>&1 &
    PIDS+=($!)
    cd -

    cd "$REPO_DIR/roles/jd-client/config-examples"
    cargo run --release -- -c jdc-config-local-example.toml > ../../../../jd-client.log 2>&1 &
    PIDS+=($!)
    cd -

    cd "$REPO_DIR/roles/translator/config-examples"
    cargo run --release -- -c tproxy-config-local-jdc-example.toml > ../../../../translator.log 2>&1 &
    PIDS+=($!)
    cd -

    ./minerd -t 1 -a sha256d -o stratum+tcp://localhost:34255 -q -D -P > ./minerd.log 2>&1 &
    PIDS+=($!)

    echo "All services started. Logs are being written."
}

# Start services initially
start_services

while true; do
    LATEST_COMMIT=$(git ls-remote "$REPO_URL" refs/heads/$BRANCH | awk '{print $1}')
    LAST_COMMIT=$(cat "$COMMIT_FILE" 2>/dev/null || echo "")

    if [ "$LATEST_COMMIT" != "$LAST_COMMIT" ]; then
        echo "New commit detected. Stopping all services..."

        # Stop any previous services
        cleanup

        echo "$LATEST_COMMIT" > "$COMMIT_FILE"
        echo "Rebuilding..."

        cargo build --manifest-path="$REPO_DIR/roles/pool/Cargo.toml" --release --jobs 3 &> ./pool-build.log
        cargo build --manifest-path="$REPO_DIR/roles/jd-server/Cargo.toml" --release --jobs 3 &> ./jd-server-build.log
        cargo build --manifest-path="$REPO_DIR/roles/jd-client/Cargo.toml" --release --jobs 3 &> ./jd-client-build.log
        cargo build --manifest-path="$REPO_DIR/roles/translator/Cargo.toml" --release --jobs 3 &> ./translator-build.log

        # Start services after rebuild
        start_services
    else
        echo "No new commit. Skipping build. Restarting services."
        echo "Sleeping for 5 minutes."
        sleep 300
    fi

done
