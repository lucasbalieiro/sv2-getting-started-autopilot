#!/bin/bash

set -e

FILENAME="bitcoin-sv2-tp-0.1.15-x86_64-linux-gnu.tar.gz"
EXPECTED_HASH_FILE_URL="https://raw.githubusercontent.com/Sjors/guix.sigs/refs/heads/sv2/sv2-tp-0.1.15/Sjors/noncodesigned.SHA256SUMS"
DOWNLOAD_URL="https://github.com/Sjors/bitcoin/releases/download/sv2-tp-0.1.15/bitcoin-sv2-tp-0.1.15-x86_64-linux-gnu.tar.gz"

# Step 1: Make sure the tar.gz file exists, download if not
if [ ! -f "$FILENAME" ]; then
    echo "$FILENAME not found. Downloading..."
    curl -L "$DOWNLOAD_URL" -o "$FILENAME"
fi

# Step 2: Download the hash file
curl -sSL "$EXPECTED_HASH_FILE_URL" -o SHA256SUMS

# Step 3: Verify the hash
echo "Verifying SHA256 checksum..."
grep "  $FILENAME" SHA256SUMS > SHA256SUMS.single

sha256sum -c SHA256SUMS.single

if [ $? -ne 0 ]; then
    echo "SHA256 checksum verification failed!"
    exit 1
fi

echo "Checksum verified."

# Step 4: Extract the tar.gz only if the folder does not exist
EXTRACTED_DIR=$(tar -tf "$FILENAME" | head -1 | cut -f1 -d"/")
if [ ! -d "$EXTRACTED_DIR" ]; then
    tar xvf "$FILENAME"
else
    echo "Directory $EXTRACTED_DIR already exists, skipping extraction."
fi
DATADIR="$PWD/$EXTRACTED_DIR/.bitcoin"

# Step 5: Create bitcoin.conf only if it does not exist
mkdir -p "$DATADIR/testnet4"
if [ ! -f "$DATADIR/bitcoin.conf" ]; then
cat > "$DATADIR/bitcoin.conf" <<EOF
[testnet4]
server=1
rpcuser=username
rpcpassword=password
prune=550
EOF
else
    echo "$DATADIR/bitcoin.conf already exists, skipping creation."
fi

# Step 6: Unzip chain.zip to get blocks and chainstate only if not already unzipped
if [ ! -d "$DATADIR/testnet4/blocks" ] || [ ! -d "$DATADIR/testnet4/chainstate" ]; then
    echo "Unzipping chain.zip to $DATADIR/testnet4/..."
    unzip -q chain.zip -d "$DATADIR/testnet4/"
else
    echo "Blocks and chainstate already exist in $DATADIR/testnet4/, skipping unzip."
fi

# Step 7: Start bitcoind with SV2 support
./$EXTRACTED_DIR/bin/bitcoind -testnet4 -sv2 -sv2port=8442 -debug=sv2 -datadir="$DATADIR" > "$PWD/bitcoind.log" 2>&1 &