#!/bin/bash

set -e

echo "Creating solbridxml2mqtt user and group..."
sudo useradd -r -s /bin/false solbridxml2mqtt || true

echo "Creating log directory..."
sudo mkdir -p /var/log/solbridxml2mqtt
sudo chown solbridxml2mqtt:solbridxml2mqtt /var/log/xml2mqtt

echo "Setting permissions..."
sudo chown root:solbridxml2mqtt /etc/solbridxml2mqtt/config.toml
sudo chmod 640 /etc/solbridxml2mqtt/config.toml

echo "Service user setup complete!"
