#!/bin/bash

set -e

echo "Building solbridxml2mqtt..."
cargo build --release

echo "Installing binary to /usr/local/bin..."
sudo cp target/release/solbridxml2mqtt /usr/local/bin/
sudo chmod +x /usr/local/bin/solbridxml2mqtt

echo "Creating configuration directory..."
sudo mkdir -p /etc/solbridxml2mqtt
sudo cp config.toml /etc/solbridxml2mqtt/config.toml

echo "Installing systemd service..."
sudo cp solbridxml2mqtt.service /etc/systemd/system/
sudo systemctl daemon-reload

echo ""
echo "Installation complete!"
echo ""
echo "To start the service:"
echo "  sudo systemctl start solbridxml2mqtt"
echo ""
echo "To enable auto-start on boot:"
echo "  sudo systemctl enable solbridxml2mqtt"
echo ""
echo "To check status:"
echo "  sudo systemctl status solbridxml2mqtt"
echo ""
echo "To view logs:"
echo "  sudo journalctl -u solbridxml2mqtt -f"