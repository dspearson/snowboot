#!/usr/bin/env bash
# Installation script for Snowboot

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

echo -e "${GREEN}Installing Snowboot...${NC}"

# Create user and group if they don't exist
if ! id -u snowboot > /dev/null 2>&1; then
    echo "Creating snowboot user..."
    useradd -r -s /bin/false -d /var/lib/snowboot snowboot
fi

# Create directories
echo "Creating directories..."
mkdir -p /etc/snowboot
mkdir -p /var/lib/snowboot
mkdir -p /var/run/snowboot
mkdir -p /usr/local/bin

# Set permissions
chown -R snowboot:snowboot /var/lib/snowboot
chown -R snowboot:snowboot /var/run/snowboot
chmod 755 /etc/snowboot
chmod 750 /var/lib/snowboot
chmod 750 /var/run/snowboot

# Install binary
echo "Installing binary..."
if [ -f "./target/release/snowboot" ]; then
    install -m 755 ./target/release/snowboot /usr/local/bin/snowboot
elif [ -f "./snowboot" ]; then
    install -m 755 ./snowboot /usr/local/bin/snowboot
else
    echo -e "${RED}Error: snowboot binary not found${NC}"
    exit 1
fi

# Install configuration
if [ -f "./config.example.toml" ]; then
    if [ ! -f "/etc/snowboot/config.toml" ]; then
        echo "Installing example configuration..."
        install -m 644 ./config.example.toml /etc/snowboot/config.example.toml
        cp /etc/snowboot/config.example.toml /etc/snowboot/config.toml
        echo -e "${YELLOW}Please edit /etc/snowboot/config.toml before starting the service${NC}"
    else
        echo "Configuration file already exists, skipping..."
    fi
fi

# Install systemd service
if [ -d "/etc/systemd/system" ]; then
    echo "Installing systemd service..."
    install -m 644 ./systemd/snowboot.service /etc/systemd/system/snowboot.service
    systemctl daemon-reload
    echo -e "${GREEN}Systemd service installed${NC}"
    echo ""
    echo "To enable and start the service:"
    echo "  systemctl enable snowboot"
    echo "  systemctl start snowboot"
    echo ""
    echo "To view logs:"
    echo "  journalctl -u snowboot -f"
else
    echo -e "${YELLOW}Warning: systemd not found, service not installed${NC}"
fi

echo ""
echo -e "${GREEN}Installation complete!${NC}"
echo ""
echo "Next steps:"
echo "1. Edit /etc/snowboot/config.toml with your settings"
echo "2. Create input FIFO: mkfifo /var/run/snowboot/input.fifo"
echo "3. Enable service: systemctl enable snowboot"
echo "4. Start service: systemctl start snowboot"
echo "5. Check status: systemctl status snowboot"
