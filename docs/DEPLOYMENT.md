# Snowboot Deployment Guide

## Table of Contents
1. [Prerequisites](#prerequisites)
2. [Installation Methods](#installation-methods)
3. [Configuration](#configuration)
4. [Running Snowboot](#running-snowboot)
5. [Monitoring](#monitoring)
6. [Security Best Practices](#security-best-practices)

## Prerequisites

### System Requirements
- **OS**: Linux (recommended), macOS, or Docker
- **CPU**: 1+ cores
- **RAM**: 256MB minimum, 512MB recommended
- **Network**: Stable connection to Icecast server
- **Rust**: 1.70+ (for building from source)

### Icecast Server
- Icecast 2.4+ running and accessible
- Source credentials (username/password)
- Mount point configured

## Installation Methods

### Method 1: Binary Installation (Recommended)

```bash
# Download latest release
curl -LO https://github.com/dspearson/snowboot/releases/latest/download/snowboot-linux-x86_64

# Make executable
chmod +x snowboot-linux-x86_64
sudo mv snowboot-linux-x86_64 /usr/local/bin/snowboot

# Verify installation
snowboot --version
```

### Method 2: From Source

```bash
# Clone repository
git clone https://github.com/dspearson/snowboot.git
cd snowboot

# Build release binary
cargo build --release

# Install
sudo cp target/release/snowboot /usr/local/bin/
```

### Method 3: Docker

```bash
# Pull image
docker pull snowboot/snowboot:latest

# Or build locally
docker build -t snowboot:latest .
```

### Method 4: Systemd Service (Linux)

```bash
# Run installation script
sudo ./systemd/install.sh

# Edit configuration
sudo nano /etc/snowboot/config.toml

# Enable and start
sudo systemctl enable snowboot
sudo systemctl start snowboot
```

## Configuration

### Configuration File

Create `/etc/snowboot/config.toml`:

```toml
[server]
host = "icecast.example.com"
port = 8000
mount = "/live.ogg"
username = "source"
# Use environment variable: SNOWBOOT_PASSWORD=your_password
use_tls = false

[audio]
sample_rate = 44100  # Hz
bitrate = 320        # kbps
buffer_seconds = 1.0

[input]
pipe_path = "/var/run/snowboot/input.fifo"

[logging]
level = "info"
format = "text"  # or "json"

[monitoring]
metrics_enabled = true
metrics_port = 9090
health_enabled = true
health_port = 8080
```

### Environment Variables

All configuration can be overridden via environment variables:

```bash
export SNOWBOOT_HOST="icecast.example.com:8000"
export SNOWBOOT_MOUNT="/live.ogg"
export SNOWBOOT_USER="source"
export SNOWBOOT_PASSWORD="your_secure_password"
export SNOWBOOT_SAMPLE_RATE=48000
export SNOWBOOT_BITRATE=192
export SNOWBOOT_INPUT_PIPE="/tmp/snowboot.fifo"
export SNOWBOOT_LOG_LEVEL="debug"
export SNOWBOOT_LOG_FORMAT="json"
export SNOWBOOT_METRICS_ENABLED="true"
export SNOWBOOT_METRICS_PORT=9090
```

### Creating the Input Pipe

```bash
# Create FIFO
mkfifo /var/run/snowboot/input.fifo

# Set permissions
chmod 600 /var/run/snowboot/input.fifo
chown snowboot:snowboot /var/run/snowboot/input.fifo
```

## Running Snowboot

### Standalone

```bash
snowboot \
  --host icecast.example.com:8000 \
  --mount /live.ogg \
  --user source \
  --password your_password \
  --input-pipe /var/run/snowboot/input.fifo \
  --sample-rate 44100 \
  --bitrate 320 \
  --log-level info
```

### With Configuration File

```bash
# Using config file
snowboot --config /etc/snowboot/config.toml

# Override specific values
snowboot --config /etc/snowboot/config.toml --log-level debug
```

### Systemd

```bash
# Start
sudo systemctl start snowboot

# Stop
sudo systemctl stop snowboot

# Restart
sudo systemctl restart snowboot

# Status
sudo systemctl status snowboot

# Logs
sudo journalctl -u snowboot -f
```

### Docker

```bash
# Using docker run
docker run -d \
  --name snowboot \
  -e SNOWBOOT_HOST=icecast:8000 \
  -e SNOWBOOT_PASSWORD=your_password \
  -v /path/to/config:/etc/snowboot \
  -v snowboot_pipes:/var/run/snowboot \
  -p 9090:9090 \
  -p 8080:8080 \
  snowboot:latest

# Using docker-compose
docker-compose up -d
```

## Monitoring

### Health Checks

```bash
# Liveness probe
curl http://localhost:8080/health

# Readiness probe
curl http://localhost:8080/ready
```

Response when healthy:
```json
{
  "status": "healthy",
  "uptime_seconds": 3600,
  "connection_state": "connected",
  "bytes_sent": 1048576,
  "bytes_read": 1048576,
  "errors": 0
}
```

### Prometheus Metrics

```bash
# Fetch metrics
curl http://localhost:9090/metrics
```

Key metrics:
- `snowboot_connection_state` - Connection status
- `snowboot_bytes_sent_total` - Total bytes sent
- `snowboot_bytes_read_total` - Total bytes read
- `snowboot_errors_total` - Error count
- `snowboot_uptime_seconds` - Service uptime

### Grafana Dashboard

Import dashboard from `docs/grafana-dashboard.json` for visualizations.

## Security Best Practices

### 1. Credentials Management
- **Never** hardcode passwords in configuration files
- Use environment variables or secret management systems
- Rotate credentials regularly

```bash
# Good: Environment variable
export SNOWBOOT_PASSWORD="$(cat /run/secrets/snowboot_password)"

# Bad: Plaintext in config
password = "mysecretpassword"  # DON'T DO THIS
```

### 2. Network Security
- Use TLS for Icecast connections when possible
- Restrict metrics/health endpoints to trusted networks
- Use firewall rules to limit access

```bash
# Firewall example (ufw)
sudo ufw allow from 10.0.0.0/24 to any port 9090 proto tcp
sudo ufw allow from 10.0.0.0/24 to any port 8080 proto tcp
```

### 3. File Permissions
- Run as non-root user
- Restrict FIFO permissions (600)
- Secure configuration directory (700)

```bash
chmod 700 /etc/snowboot
chmod 600 /etc/snowboot/config.toml
chmod 600 /var/run/snowboot/input.fifo
```

### 4. Systemd Hardening
The included systemd service has security hardening:
- `NoNewPrivileges=true`
- `ProtectSystem=strict`
- `ProtectHome=true`
- `PrivateTmp=true`
- Memory and syscall restrictions

### 5. Resource Limits
Configure limits in systemd or Docker:

```ini
# /etc/systemd/system/snowboot.service.d/limits.conf
[Service]
LimitNOFILE=65536
LimitNPROC=512
CPUQuota=50%
MemoryMax=512M
```

## Troubleshooting

See [TROUBLESHOOTING.md](TROUBLESHOOTING.md) for common issues and solutions.

## Upgrading

### Binary Upgrade
```bash
# Stop service
sudo systemctl stop snowboot

# Download new version
curl -LO https://github.com/dspearson/snowboot/releases/latest/download/snowboot-linux-x86_64

# Replace binary
sudo mv snowboot-linux-x86_64 /usr/local/bin/snowboot
sudo chmod +x /usr/local/bin/snowboot

# Start service
sudo systemctl start snowboot
```

### Docker Upgrade
```bash
# Pull latest
docker pull snowboot/snowboot:latest

# Restart container
docker-compose down
docker-compose up -d
```

## Backup and Recovery

### What to Backup
- Configuration: `/etc/snowboot/`
- Logs (optional): `/var/log/snowboot/`

### Backup Script
```bash
#!/bin/bash
tar czf snowboot-backup-$(date +%Y%m%d).tar.gz \
  /etc/snowboot \
  /etc/systemd/system/snowboot.service
```

### Recovery
```bash
# Extract backup
tar xzf snowboot-backup-20250101.tar.gz -C /

# Reload systemd
sudo systemctl daemon-reload

# Start service
sudo systemctl start snowboot
```
