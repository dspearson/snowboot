# Snowboot

Equipment to help brave icy streams - an Icecast source client with queue-based playback and an HTTP API, built on `oggmux`.

## Overview

Snowboot is an Icecast source client that streams Ogg Vorbis content to an Icecast server. It provides a queue-based playback system with an HTTP API for programmatic control, making it suitable as a backend for radio applications. It uses the `oggmux` library to handle stream processing, including automatic silence generation when no input is available.

### Features

- **Queue-based playback**: Add, remove, reorder and clear tracks via API
- **HTTP API**: Full control over playback and queue management
- **Skip control**: Skip the currently playing track at any time
- **Automatic silence**: When the queue is empty, silence is automatically generated
- **Robust connectivity**: Handles network issues gracefully
- **Low resource usage**: Efficient Rust implementation with async I/O
- **Prometheus metrics**: Track playback, queue length and connection stats
- **Configurable**: Adjust buffer size, bitrate, sample rate and more

## Usage

```bash
# Start snowboot with default settings
snowboot

# Or with custom settings
snowboot --host icecast.example.com:8000 --mount /my-stream.ogg --user source --password mypass --api-port 3000

# Add a track to the queue
curl -X POST http://localhost:3000/api/queue \
  -H 'Content-Type: application/json' \
  -d '{"path": "/path/to/track.ogg", "title": "My Track"}'

# List the queue
curl http://localhost:3000/api/queue

# Add a track to play next
curl -X POST http://localhost:3000/api/queue/next \
  -H 'Content-Type: application/json' \
  -d '{"path": "/path/to/urgent.ogg", "title": "Play This Next"}'

# Skip the current track
curl -X POST http://localhost:3000/api/skip

# Check status (now playing, queue length, connection state)
curl http://localhost:3000/api/status

# Clear the queue
curl -X DELETE http://localhost:3000/api/queue
```

### API Endpoints

| Method   | Path                    | Description                          |
|----------|-------------------------|--------------------------------------|
| `GET`    | `/api/queue`            | List queued tracks                   |
| `POST`   | `/api/queue`            | Add track to end of queue            |
| `DELETE` | `/api/queue`            | Clear queue                          |
| `DELETE` | `/api/queue/:id`        | Remove track by ID                   |
| `PUT`    | `/api/queue/:id/position` | Move track to position `{"position": N}` |
| `POST`   | `/api/queue/next`       | Insert track at front of queue       |
| `POST`   | `/api/skip`             | Skip current track                   |
| `GET`    | `/api/status`           | Now playing + queue length + state   |
| `GET`    | `/health`               | Health check                         |
| `GET`    | `/ready`                | Readiness probe                      |
| `GET`    | `/metrics`              | Prometheus metrics                   |

### Command Line Options

```
OPTIONS:
    --host <HOST[:PORT]>       Icecast server address [default: localhost:8000]
    --mount <PATH>             Mount point path [default: /stream.ogg]
    --user <USERNAME>          Username for server authentication [default: source]
    --password <PASSWORD>      Password for server authentication [default: hackme]
    --sample-rate <RATE>       Sample rate in Hz [default: 44100]
    --bitrate <BITRATE>        Bitrate in kbps [default: 320]
    --buffer <SECONDS>         Buffer size in seconds [default: 1.0]
    --api-port <PORT>          API server port [default: 3000]
    --api-bind <ADDR>          API server bind address [default: 0.0.0.0]
    --log-level <LEVEL>        Log level (trace, debug, info, warn, error) [default: info]
    --help                     Print help
    --version                  Print version
```

## Requirements

- Rust 2024 edition
- An Icecast server to connect to
- Ogg Vorbis files (.ogg or .oga) for playback

## Dependencies

Snowboot depends on the `oggmux` library for Ogg stream processing and silence generation.

## Licence

ISC Licence — See LICENCE file for details.
