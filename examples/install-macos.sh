#!/bin/bash
# Install LLM Gateway as a launchd service on macOS

set -e

# Configuration
INSTALL_DIR="/usr/local/bin"
WORKING_DIR="/opt/llm-gateway"
PLIST_DIR="$HOME/Library/LaunchAgents"
PLIST_FILE="com.llmgateway.plist"

echo "Installing LLM Gateway on macOS..."
echo

# Check if binary exists
if [ ! -f "./target/release/llm-gateway" ]; then
    echo "Error: Binary not found. Please run 'cargo build --release' first."
    exit 1
fi

# Copy binary
echo "1. Installing binary to $INSTALL_DIR..."
sudo cp ./target/release/llm-gateway "$INSTALL_DIR/"
sudo chmod +x "$INSTALL_DIR/llm-gateway"

# Create working directory
echo "2. Creating working directory at $WORKING_DIR..."
sudo mkdir -p "$WORKING_DIR"

# Copy config
if [ -f "./config.toml" ]; then
    echo "3. Copying configuration..."
    sudo cp ./config.toml "$WORKING_DIR/"
else
    echo "3. Creating config from example..."
    sudo cp ./config.toml.example "$WORKING_DIR/config.toml"
    echo "   WARNING: Please edit $WORKING_DIR/config.toml with your API keys!"
fi

# Create log directory
echo "4. Creating log directory..."
sudo mkdir -p /var/log/llm-gateway

# Install launchd plist
echo "5. Installing launchd service..."
mkdir -p "$PLIST_DIR"

# Update plist with actual paths
sed "s|/usr/local/bin/llm-gateway|$INSTALL_DIR/llm-gateway|g" examples/llm-gateway.plist | \
sed "s|/opt/llm-gateway|$WORKING_DIR|g" | \
sed "s|/var/log/llm-gateway.log|/var/log/llm-gateway/gateway.log|g" | \
sed "s|/var/log/llm-gateway.err.log|/var/log/llm-gateway/gateway.err.log|g" > "$PLIST_DIR/$PLIST_FILE"

echo
echo "Installation complete!"
echo
echo "Next steps:"
echo "  1. Edit your configuration:"
echo "     sudo nano $WORKING_DIR/config.toml"
echo
echo "  2. Load the service:"
echo "     launchctl load $PLIST_DIR/$PLIST_FILE"
echo
echo "  3. Start the service:"
echo "     launchctl start com.llmgateway"
echo
echo "  4. Check status:"
echo "     launchctl list | grep llmgateway"
echo
echo "  5. View logs:"
echo "     tail -f /var/log/llm-gateway/gateway.log"
echo
echo "Commands:"
echo "  Test config:   $INSTALL_DIR/llm-gateway test"
echo "  Reload:        $INSTALL_DIR/llm-gateway reload"
echo "  Stop service:  launchctl stop com.llmgateway"
echo "  Unload:        launchctl unload $PLIST_DIR/$PLIST_FILE"
echo
