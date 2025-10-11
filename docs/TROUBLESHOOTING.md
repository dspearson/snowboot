# Snowboot Troubleshooting Guide

## Table of Contents
1. [Connection Issues](#connection-issues)
2. [Authentication Failures](#authentication-failures)
3. [Audio Problems](#audio-problems)
4. [Performance Issues](#performance-issues)
5. [Configuration Problems](#configuration-problems)
6. [Monitoring & Metrics](#monitoring--metrics)
7. [Common Error Codes](#common-error-codes)
8. [Debugging Tools](#debugging-tools)

---

## Connection Issues

### Problem: Cannot connect to Icecast server

**Symptoms**:
```
ERROR Failed to connect to icecast.example.com:8000: Connection refused
```

**Causes & Solutions**:

1. **Icecast server not running**
   ```bash
   # Check if Icecast is running
   systemctl status icecast2

   # Check if port is listening
   netstat -tuln | grep 8000
   ```

2. **Wrong host/port**
   ```bash
   # Verify configuration
   snowboot --host icecast.example.com:8000 --log-level debug

   # Test connectivity
   telnet icecast.example.com 8000
   ```

3. **Firewall blocking**
   ```bash
   # Check firewall rules
   sudo iptables -L -n | grep 8000

   # Allow port
   sudo ufw allow 8000/tcp
   ```

4. **Network unreachable**
   ```bash
   # Test network path
   ping icecast.example.com
   traceroute icecast.example.com
   ```

### Problem: Connection drops frequently

**Symptoms**:
```
WARN Lost connection while sending data
INFO Retrying connection in 2.0 seconds...
```

**Solutions**:

1. **Check network stability**
   ```bash
   # Monitor packet loss
   ping -c 100 icecast.example.com

   # Check MTU issues
   ping -M do -s 1472 icecast.example.com
   ```

2. **Increase reconnection parameters**
   ```toml
   # In connection configuration
   max_retries = 0  # Infinite retries
   max_backoff_secs = 120.0  # Longer backoff
   ```

3. **Check Icecast server logs**
   ```bash
   tail -f /var/log/icecast2/error.log
   ```

---

## Authentication Failures

### Problem: 401 Unauthorized / 403 Forbidden

**Symptoms**:
```
ERROR Authentication failed: HTTP/1.1 401 Unauthorized
```

**Solutions**:

1. **Verify credentials**
   ```bash
   # Check environment variables
   echo $SNOWBOOT_USER
   echo $SNOWBOOT_PASSWORD

   # Test with explicit credentials
   snowboot --user source --password correct_password
   ```

2. **Check Icecast configuration**
   ```xml
   <!-- In /etc/icecast2/icecast.xml -->
   <authentication>
       <source-password>your_password</source-password>
   </authentication>
   ```

3. **Mount point permissions**
   ```xml
   <!-- Check mount-specific auth -->
   <mount>
       <mount-name>/live.ogg</mount-name>
       <username>source</username>
       <password>mount_specific_pass</password>
   </mount>
   ```

4. **Special characters in password**
   ```bash
   # URL-encode special characters or use env var
   export SNOWBOOT_PASSWORD='p@ssw0rd!'  # Quotes important
   ```

---

## Audio Problems

### Problem: No audio streaming (pipe issues)

**Symptoms**:
```
ERROR Input pipe not found: /tmp/snowboot.fifo
ERROR Failed to open input pipe: Permission denied
```

**Solutions**:

1. **Create FIFO if missing**
   ```bash
   mkfifo /tmp/snowboot.fifo
   chmod 600 /tmp/snowboot.fifo
   ```

2. **Check FIFO vs regular file**
   ```bash
   # Verify it's a FIFO
   file /tmp/snowboot.fifo
   # Should show: fifo (named pipe)

   # If regular file, remove and recreate
   rm /tmp/snowboot.fifo
   mkfifo /tmp/snowboot.fifo
   ```

3. **Permission issues**
   ```bash
   # Check ownership
   ls -la /tmp/snowboot.fifo

   # Fix permissions
   sudo chown snowboot:snowboot /tmp/snowboot.fifo
   chmod 600 /tmp/snowboot.fifo
   ```

4. **Test pipe manually**
   ```bash
   # Terminal 1: Start snowboot
   snowboot --input-pipe /tmp/test.fifo

   # Terminal 2: Send data
   cat audio.ogg > /tmp/test.fifo
   ```

### Problem: Audio quality issues

**Symptoms**:
- Choppy playback
- Dropouts
- Distortion

**Solutions**:

1. **Increase buffer size**
   ```bash
   snowboot --buffer 2.0  # 2 seconds
   ```

2. **Check sample rate match**
   ```bash
   # Ensure input matches output
   ffprobe input_audio.ogg  # Check source rate
   snowboot --sample-rate 48000  # Match it
   ```

3. **Reduce bitrate if network constrained**
   ```bash
   snowboot --bitrate 192  # Lower bitrate
   ```

4. **Monitor buffer metrics**
   ```bash
   curl localhost:9090/metrics | grep buffer
   ```

---

## Performance Issues

### Problem: High CPU usage

**Symptoms**:
```bash
top  # Shows snowboot using >50% CPU
```

**Solutions**:

1. **Check for excessive reconnection**
   ```bash
   # Look for reconnection loop
   journalctl -u snowboot | grep -i retry

   # Increase backoff
   snowboot --max-backoff-secs 60
   ```

2. **Reduce logging verbosity**
   ```bash
   snowboot --log-level warn  # Less logging
   ```

3. **Check for blocking operations**
   ```bash
   # Profile with perf
   perf record -F 99 -p $(pgrep snowboot)
   perf report
   ```

### Problem: High memory usage

**Symptoms**:
```bash
ps aux | grep snowboot  # Shows high RSS
```

**Solutions**:

1. **Reduce buffer size**
   ```bash
   snowboot --buffer 0.5  # Smaller buffer
   ```

2. **Set memory limits (systemd)**
   ```ini
   [Service]
   MemoryMax=512M
   MemoryHigh=384M
   ```

3. **Check for memory leaks**
   ```bash
   # Monitor over time
   watch -n 1 'ps aux | grep snowboot'
   ```

---

## Configuration Problems

### Problem: Configuration not loading

**Symptoms**:
```
ERROR Failed to read config file: No such file or directory
```

**Solutions**:

1. **Verify file path**
   ```bash
   ls -la /etc/snowboot/config.toml
   snowboot --config /etc/snowboot/config.toml
   ```

2. **Check TOML syntax**
   ```bash
   # Validate TOML
   python3 -c "import toml; toml.load('/etc/snowboot/config.toml')"
   ```

3. **Use absolute paths**
   ```bash
   # Not this
   snowboot --config config.toml

   # This
   snowboot --config /etc/snowboot/config.toml
   ```

### Problem: Environment variables not working

**Solutions**:

1. **Check variable names**
   ```bash
   # Must be SNOWBOOT_* prefix
   export SNOWBOOT_HOST=icecast.example.com  # ✓
   export HOST=icecast.example.com           # ✗
   ```

2. **Systemd environment**
   ```ini
   # /etc/systemd/system/snowboot.service
   [Service]
   Environment="SNOWBOOT_PASSWORD=secret"
   EnvironmentFile=/etc/snowboot/environment
   ```

3. **Docker environment**
   ```yaml
   # docker-compose.yml
   environment:
     - SNOWBOOT_HOST=icecast:8000
     - SNOWBOOT_PASSWORD=${SNOWBOOT_PASSWORD}
   ```

---

## Monitoring & Metrics

### Problem: Metrics endpoint not accessible

**Symptoms**:
```bash
curl localhost:9090/metrics
# Connection refused
```

**Solutions**:

1. **Check if metrics enabled**
   ```toml
   [monitoring]
   metrics_enabled = true
   metrics_port = 9090
   ```

2. **Verify port binding**
   ```bash
   netstat -tuln | grep 9090
   lsof -i :9090
   ```

3. **Check firewall**
   ```bash
   sudo ufw status | grep 9090
   sudo ufw allow 9090/tcp
   ```

### Problem: Health check always returns unhealthy

**Symptoms**:
```bash
curl localhost:8080/health
# {"status":"degraded","connection_state":"reconnecting"}
```

**Solutions**:

1. **Check connection state**
   ```bash
   # Look at logs for connection errors
   journalctl -u snowboot | grep -i connection
   ```

2. **Verify Icecast is reachable**
   ```bash
   curl -I http://icecast.example.com:8000/
   ```

3. **Check metrics for clues**
   ```bash
   curl localhost:9090/metrics | grep -E '(connection|error)'
   ```

---

## Common Error Codes

### Configuration Errors (1000-1999)
- **1001**: Invalid port number
  - Solution: Use port 1-65535
- **1003**: Invalid buffer size
  - Solution: Use 0.1-10.0 seconds
- **1004**: Invalid sample rate
  - Solution: Use 8000-192000 Hz
- **1005**: Invalid bitrate
  - Solution: Use 8-500 kbps

### Connection Errors (2000-2999)
- **2000**: Connection failed
  - Solution: Check network, firewall, Icecast status
- **2001**: Connection timeout
  - Solution: Increase timeout, check network latency
- **2002**: Authentication failed
  - Solution: Verify credentials
- **2003**: Unexpected response
  - Solution: Check Icecast version, mount point

### I/O Errors (3000-3999)
- **3000**: Pipe not found
  - Solution: Create FIFO with `mkfifo`
- **3001**: Pipe open failed
  - Solution: Check permissions
- **3003**: Not a FIFO
  - Solution: Remove file, create FIFO
- **3004**: Permission denied
  - Solution: Fix file permissions

### Protocol Errors (4000-4999)
- **4000**: HTTP parse failed
  - Solution: Check Icecast server, enable debug logging

---

## Debugging Tools

### Enable Debug Logging
```bash
# Maximum verbosity
snowboot --log-level trace

# JSON format for parsing
export SNOWBOOT_LOG_FORMAT=json
snowboot --log-level debug | jq
```

### Check System Resources
```bash
# Monitor in real-time
watch -n 1 'ps aux | grep snowboot'

# Check file descriptors
lsof -p $(pgrep snowboot)

# Network connections
ss -tunap | grep snowboot
```

### Packet Capture
```bash
# Capture Icecast traffic
sudo tcpdump -i any -w snowboot.pcap port 8000

# Analyze
wireshark snowboot.pcap
```

### Metrics Analysis
```bash
# Get all metrics
curl -s localhost:9090/metrics

# Watch specific metric
watch -n 1 'curl -s localhost:9090/metrics | grep bytes_sent'

# Graph in terminal (with gnuplot)
while true; do
  curl -s localhost:9090/metrics | \
  grep bytes_sent_total | \
  awk '{print systime(), $2}'
  sleep 1
done | gnuplot -e "plot '-' with lines"
```

### Strace for System Calls
```bash
# Trace system calls
sudo strace -p $(pgrep snowboot) -f

# Focus on network calls
sudo strace -p $(pgrep snowboot) -e trace=network
```

### Core Dump Analysis
```bash
# Enable core dumps
ulimit -c unlimited

# After crash
gdb /usr/local/bin/snowboot core.12345

# In gdb
(gdb) bt
(gdb) info threads
```

---

## Getting Help

### Logs to Collect
```bash
# Snowboot logs
journalctl -u snowboot --since "1 hour ago" > snowboot.log

# System info
uname -a > system.txt
snowboot --version >> system.txt

# Configuration (redact passwords!)
cat /etc/snowboot/config.toml | sed 's/password.*/password = REDACTED/' > config.txt

# Metrics snapshot
curl localhost:9090/metrics > metrics.txt

# Health status
curl localhost:8080/health > health.json
```

### Issue Template
```markdown
**Environment**:
- OS: (Linux/macOS/Docker)
- Snowboot version: (output of `snowboot --version`)
- Icecast version:

**Configuration**:
```toml
[paste redacted config]
```

**Logs**:
```
[paste relevant logs]
```

**Expected behavior**:
[what should happen]

**Actual behavior**:
[what actually happens]

**Steps to reproduce**:
1.
2.
3.
```

### Community Support
- GitHub Issues: https://github.com/dspearson/snowboot/issues
- Discussions: https://github.com/dspearson/snowboot/discussions
- IRC: #snowboot on Libera.Chat (if available)
