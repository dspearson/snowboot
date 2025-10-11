#!/usr/bin/env bash
# Uninstallation script for Snowboot

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if running as root
if [[ $EUID -ne 0 ]]; then
   echo -e "${RED}Error: This script must be run as root${NC}"
   exit 1
fi

echo -e "${YELLOW}Uninstalling Snowboot...${NC}"

# Stop and disable service
if systemctl is-active --quiet snowboot; then
    echo "Stopping snowboot service..."
    systemctl stop snowboot
fi

if systemctl is-enabled --quiet snowboot 2>/dev/null; then
    echo "Disabling snowboot service..."
    systemctl disable snowboot
fi

# Remove systemd service
if [ -f "/etc/systemd/system/snowboot.service" ]; then
    echo "Removing systemd service..."
    rm -f /etc/systemd/system/snowboot.service
    systemctl daemon-reload
fi

# Remove binary
if [ -f "/usr/local/bin/snowboot" ]; then
    echo "Removing binary..."
    rm -f /usr/local/bin/snowboot
fi

# Ask about configuration and data
echo ""
read -p "Remove configuration files? (y/N) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    rm -rf /etc/snowboot
    echo "Configuration removed"
fi

echo ""
read -p "Remove data directory? (y/N) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    rm -rf /var/lib/snowboot
    rm -rf /var/run/snowboot
    echo "Data removed"
fi

echo ""
read -p "Remove snowboot user? (y/N) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    if id -u snowboot > /dev/null 2>&1; then
        userdel snowboot
        echo "User removed"
    fi
fi

echo ""
echo -e "${GREEN}Uninstallation complete!${NC}"
