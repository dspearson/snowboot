# Snowboot Architecture

## Overview

Snowboot is an asynchronous, production-ready Icecast source client built in Rust. It streams Ogg Vorbis audio from a named pipe (FIFO) to an Icecast server with automatic reconnection, comprehensive monitoring, and robust error handling.

## High-Level Architecture

```
┌─────────────┐         ┌──────────────┐         ┌─────────────┐
│   Audio     │         │              │         │   Icecast   │
│   Producer  ├────────>│   Snowboot   ├────────>│   Server    │
│             │  FIFO   │              │  HTTP   │             │
└─────────────┘         └──────────────┘         └─────────────┘
                                │
                                │
                        ┌───────▼────────┐
                        │   Monitoring   │
                        │ (Metrics/Health)│
                        └────────────────┘
```

## Core Components

### 1. Main Application (`src/main.rs`)
- **Purpose**: Entry point and coordination
- **Responsibilities**:
  - CLI argument parsing (clap)
  - Configuration loading
  - Spawning async tasks
  - Signal handling (SIGINT/SIGTERM)
  - Graceful shutdown coordination

**Key Flow**:
```
1. Parse CLI args → 2. Load config → 3. Initialize logging →
4. Set up signal handlers → 5. Connect to Icecast →
6. Spawn input/output tasks → 7. Monitor until shutdown
```

### 2. Configuration System (`src/config.rs`)
- **Purpose**: Unified configuration management
- **Features**:
  - TOML file parsing
  - Environment variable overrides
  - Validation with bounds checking
  - Precedence: CLI > env > config file > defaults

**Config Structure**:
```rust
Config
├── ServerConfig (Icecast connection)
├── AudioConfig (sample rate, bitrate, buffer)
├── InputConfig (pipe path)
├── LoggingConfig (level, format)
└── MonitoringConfig (metrics, health)
```

### 3. Connection Manager (`src/connection.rs`)
- **Purpose**: Resilient Icecast connectivity
- **Features**:
  - Exponential backoff retry (configurable)
  - Connection state tracking
  - Automatic reconnection on failure
  - Smart retry logic (no retry on auth errors)

**State Machine**:
```
Disconnected ──> Connecting ──> Connected
                      │              │
                      │              │
                      ▼              ▼
                  Failed      Reconnecting
```

**Retry Algorithm**:
```
backoff = initial_backoff
while retry_count < max_retries:
    try_connect()
    if success: return
    if auth_error: fail_permanently
    sleep(backoff)
    backoff = min(backoff * multiplier, max_backoff)
    retry_count++
```

### 4. Icecast Client (`src/icecast.rs`)
- **Purpose**: HTTP protocol handling
- **Features**:
  - Proper HTTP PUT request construction
  - Robust response parsing (httparse)
  - TCP_NODELAY for low latency
  - Thread-safe connection management (Arc<Mutex>)

**HTTP Flow**:
```
1. TCP connect
2. Send PUT request with Basic Auth
3. Read response until \r\n\r\n
4. Parse status code (100/200 = OK, 401/403 = auth fail)
5. Keep connection open for streaming
```

### 5. Error Handling (`src/errors.rs`)
- **Purpose**: Type-safe, actionable errors
- **Features**:
  - Custom error types with error codes
  - Contextual error messages
  - Helpful suggestions for resolution
  - No panics in production code

**Error Categories**:
```
1xxx - Configuration errors
2xxx - Connection errors
3xxx - I/O errors
4xxx - Protocol errors
5xxx - Internal errors
```

### 6. Input Validation (`src/validation.rs`)
- **Purpose**: Security and correctness
- **Validates**:
  - Port numbers (1-65535)
  - Hostnames (length, characters)
  - Sample rates (8000-192000 Hz)
  - Bitrates (8-500 kbps)
  - Buffer sizes (0.1-10.0 seconds)
  - FIFO existence and permissions

### 7. Metrics & Monitoring (`src/metrics.rs`)
- **Purpose**: Observability
- **Prometheus Metrics**:
  ```
  - snowboot_connection_state
  - snowboot_connection_attempts_total
  - snowboot_connection_failures_total
  - snowboot_bytes_sent_total
  - snowboot_bytes_read_total
  - snowboot_chunks_sent_total
  - snowboot_send_duration_seconds
  - snowboot_errors_total
  - snowboot_uptime_seconds
  ```

### 8. HTTP Server (`src/server.rs`)
- **Purpose**: Health checks and metrics export
- **Endpoints**:
  - `GET /health` - Liveness probe with detailed status
  - `GET /ready` - Readiness probe (connected = ready)
  - `GET /metrics` - Prometheus metrics in text format

**Health Response**:
```json
{
  "status": "healthy|degraded",
  "uptime_seconds": 3600,
  "connection_state": "connected",
  "bytes_sent": 1048576,
  "bytes_read": 1048576,
  "errors": 0
}
```

## Data Flow

### Input Pipeline
```
Audio Source
    │
    ▼
Named Pipe (FIFO)
    │
    ▼
Tokio Async Read (8KB chunks)
    │
    ▼
OggMux (from oggmux crate)
    │
    ▼
Connection Manager
    │
    ▼
Icecast Server
```

### Async Task Architecture
```
┌─────────────────────────────────────┐
│         Main Event Loop             │
│  (monitors running flag)            │
└────────────────┬────────────────────┘
                 │
        ┌────────┴─────────┐
        │                  │
┌───────▼────────┐  ┌─────▼──────────┐
│  Input Reader  │  │ Icecast Sender │
│  Task          │  │ Task           │
│                │  │                │
│ Reads from     │  │ Receives from  │
│ FIFO, sends    │  │ OggMux, sends  │
│ to OggMux      │  │ to Icecast     │
└────────────────┘  └────────────────┘
```

### Error Flow
```
Error Occurs
    │
    ▼
Error Typed (SnowbootError)
    │
    ▼
Log Error (with context)
    │
    ▼
Increment Error Metric
    │
    ▼
Attempt Recovery or Fail Gracefully
```

## Concurrency Model

### Async Runtime: Tokio
- **Executor**: Multi-threaded work-stealing
- **Features**: Full (all Tokio features enabled)

### Synchronization Primitives
- **Arc<AtomicBool>**: Running flag (shared across tasks)
- **Arc<Mutex<>>**: Connection state, TCP stream
- **mpsc channels**: OggMux communication

### Thread Safety
- All shared state is Send + Sync
- No unsafe code
- Mutex contention minimized (held briefly)

## Security Architecture

### Defense in Depth
1. **Input Validation**: All inputs validated before use
2. **Least Privilege**: Systemd runs as non-root user
3. **Sandboxing**: Systemd hardening (ProtectSystem, PrivateTmp, etc.)
4. **No Credentials in Logs**: Passwords sanitized from all logs
5. **TLS Support**: Optional encrypted Icecast connections
6. **Resource Limits**: Configurable memory/CPU limits

### Attack Surface Minimization
- **No HTTP server** (just health/metrics endpoints)
- **No file writes** (except logs via systemd)
- **No dynamic code loading**
- **No external dependencies at runtime** (static linking)

## Performance Characteristics

### Latency
- **TCP_NODELAY enabled**: Low latency mode
- **Async I/O**: Non-blocking, scales to thousands of connections
- **Zero-copy where possible**: Minimal memory allocations

### Throughput
- **Buffered I/O**: 8KB chunks by default
- **Configurable buffer**: 0.1-10 seconds of audio
- **Back-pressure handling**: Flow control between components

### Resource Usage
- **Memory**: ~50-100MB typical
- **CPU**: <5% on modern hardware for 320kbps stream
- **Network**: Minimal overhead (Ogg is efficient)

## Failure Modes & Recovery

### Connection Failures
- **Retry with backoff**: Exponential (1s → 60s max)
- **Circuit breaker**: Stop retrying auth failures
- **Graceful degradation**: Continue attempting in background

### Pipe Read Failures
- **Transient errors**: Log and retry
- **Persistent errors**: Error metrics incremented
- **EOF**: Reopen pipe automatically

### Network Partitions
- **Detection**: TCP keepalive + write timeouts
- **Recovery**: Automatic reconnection
- **Monitoring**: Connection state metrics

## Dependencies

### Core Runtime
- `tokio` - Async runtime
- `futures` - Async primitives

### Networking
- `httparse` - HTTP parsing
- `rustls` + `tokio-rustls` - TLS support
- `base64` - Auth encoding

### Serialization
- `serde` + `serde_derive` - Config serialization
- `toml` - TOML parsing
- `serde_json` - JSON health responses

### Monitoring
- `prometheus` - Metrics collection
- `axum` + `tower-http` - HTTP server
- `tracing` + `tracing-subscriber` - Structured logging

### Audio
- `ogg` - Ogg container format
- `oggmux` - Stream multiplexing

### CLI & Config
- `clap` - CLI parsing
- `anyhow` - Error context (replaced by custom errors in most places)
- `thiserror` - Error derive macros

### Utilities
- `bytes` - Efficient byte buffers
- `ctrlc` - Signal handling

## Testing Strategy

### Unit Tests
- Located in each module (`#[cfg(test)]`)
- Test business logic in isolation
- 100+ test cases across modules

### Integration Tests
- Located in `tests/integration/`
- Test component interactions
- Mock Icecast server for network tests

### Benchmarks
- Located in `benches/`
- Criterion for performance testing
- Track validation, config parsing, data processing

### CI/CD Testing
- GitHub Actions on push/PR
- Multi-platform (Linux, macOS)
- Multi-version (stable, beta)
- Security audit (cargo audit)

## Extension Points

### Adding New Protocols
1. Implement `Transport` trait
2. Add to `connection.rs`
3. Update config schema

### Custom Metrics
1. Add to `metrics.rs` lazy_static
2. Register in `init_metrics()`
3. Increment in relevant code

### New Health Checks
1. Add endpoint to `server.rs`
2. Implement health logic
3. Update documentation

## Build & Deployment

### Build Modes
- **Debug**: Fast compilation, no optimization
- **Release**: Optimized, LTO enabled
- **Musl**: Static linking for Alpine/scratch containers

### Deployment Targets
- **Bare metal**: Systemd service
- **Docker**: Multi-stage Alpine image
- **Kubernetes**: Helm chart (future)

### Binary Size
- Debug: ~100MB
- Release: ~15MB
- Release (musl, stripped): ~8MB
