# Snowboot

Equipment to help brave icy streams — an Icecast source client with queue-based playback and an HTTP API, built on `oggmux`.

## Overview

Snowboot is an Icecast source client that streams Ogg Vorbis content to an Icecast server. It provides a queue-based playback system with an HTTP API for programmatic control, making it suitable as a backend for radio applications. It uses the `oggmux` library to handle stream processing, including automatic silence generation when no input is available.

### Features

- **Queue-based playback**: Add, remove, reorder, shuffle and clear tracks via API
- **Bulk operations**: Add multiple files or scan directories in one call
- **Automatic metadata**: Title and artist extracted from Ogg Vorbis comments
- **SSE event stream**: Real-time track change notifications for UI clients
- **Playback history**: Track what was played, when, and for how long
- **API authentication**: Optional bearer token auth for API endpoints
- **Media directory restriction**: Lock file access to a specific directory
- **Skip control**: Skip the currently playing track at any time
- **Automatic silence**: When the queue is empty, silence is automatically generated
- **Automatic reconnection**: Exponential backoff reconnection on Icecast connection loss
- **Prometheus metrics**: Track playback, queue length and connection stats
- **Configurable**: Adjust buffer size, bitrate, sample rate and more

## Usage

```bash
# Start snowboot with default settings
snowboot

# With custom settings and authentication
snowboot --host icecast.example.com:8000 \
  --mount /my-stream.ogg \
  --password mypass \
  --api-port 3000 \
  --api-token mysecret \
  --media-dir /srv/music

# Add a track to the queue (title read from file metadata)
curl -X POST http://localhost:3000/api/queue \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer mysecret' \
  -d '{"path": "/srv/music/track.ogg"}'

# Add a whole directory
curl -X POST http://localhost:3000/api/queue/bulk \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer mysecret' \
  -d '{"directory": "/srv/music/album", "recursive": true}'

# Shuffle the queue
curl -X POST http://localhost:3000/api/queue/shuffle \
  -H 'Authorization: Bearer mysecret'

# Skip the current track
curl -X POST http://localhost:3000/api/skip \
  -H 'Authorization: Bearer mysecret'

# Check status
curl http://localhost:3000/api/status \
  -H 'Authorization: Bearer mysecret'

# Listen for real-time events
curl -N http://localhost:3000/api/events \
  -H 'Authorization: Bearer mysecret'

# View playback history
curl http://localhost:3000/api/history \
  -H 'Authorization: Bearer mysecret'
```

### API Endpoints

Endpoints under `/api/` require a bearer token when `--api-token` is set. Health, readiness and metrics endpoints are always public.

| Method   | Path                      | Description                              |
|----------|---------------------------|------------------------------------------|
| `GET`    | `/api/queue`              | List queued tracks                       |
| `POST`   | `/api/queue`              | Add track `{"path": "..."}`              |
| `DELETE` | `/api/queue`              | Clear queue                              |
| `DELETE` | `/api/queue/:id`          | Remove track by ID                       |
| `PUT`    | `/api/queue/:id/position` | Move track `{"position": N}`             |
| `POST`   | `/api/queue/next`         | Insert track at front of queue           |
| `POST`   | `/api/queue/bulk`         | Add multiple tracks or scan a directory  |
| `POST`   | `/api/queue/shuffle`      | Shuffle the queue                        |
| `POST`   | `/api/skip`               | Skip current track                       |
| `GET`    | `/api/status`             | Now playing + queue length + state       |
| `GET`    | `/api/history`            | Playback history                         |
| `GET`    | `/api/events`             | SSE event stream (track changes)         |
| `GET`    | `/health`                 | Health check (public)                    |
| `GET`    | `/ready`                  | Readiness probe (public)                 |
| `GET`    | `/metrics`                | Prometheus metrics (public)              |

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
    --api-token <TOKEN>        Bearer token for API authentication
    --media-dir <DIR>          Restrict file paths to this directory
    --log-level <LEVEL>        Log level (trace, debug, info, warn, error) [default: info]
    --help                     Print help
    --version                  Print version
```

## Requirements

- Rust 2021 edition
- An Icecast server to connect to
- Ogg Vorbis files (.ogg or .oga) for playback

## Dependencies

Snowboot depends on the `oggmux` library for Ogg stream processing and silence generation.

## Licence

ISC Licence — See LICENCE file for details.
