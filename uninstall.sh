#!/bin/bash

set -e

echo "Stopping and disabling service..."
sudo systemctl stop solbridxml2mqtt || true
sudo systemctl disable solbridxml2mqtt || true

echo "Removing systemd service..."
sudo rm -f /etc/systemd/system/solbridxml2mqtt.service
sudo systemctl daemon-reload

echo "Removing binary..."
sudo rm -f /usr/local/bin/solbridxml2mqtt

echo "Removing configuration (you may want to backup first)..."
read -p "Remove /etc/ solbridxml2mqtt? [y/N] " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    sudo rm -rf /etc/solbridxml2mqtt
fi

echo "Removing user and group..."
read -p "Remove solbridxml2mqtt user? [y/N] " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    sudo userdel solbridxml2mqtt || true
fi

echo "Uninstallation complete!"
