# Snowboot

Equipment to help brave icy streams - a simple, efficient Icecast source client built on `oggmux`.

## Overview

Snowboot is a lightweight Icecast source client that streams Ogg Vorbis content to an Icecast server. It uses the `oggmux` library to handle stream processing, including automatic silence generation when no input is available.

### Features

- **Simple pipe-based input**: Feed audio data through a named pipe
- **Automatic silence**: When input is unavailable, silence is automatically generated
- **Robust connectivity**: Handles network issues gracefully
- **Low resource usage**: Efficient Rust implementation with async I/O
- **Clean transitions**: Seamless transitions between real audio and silence
- **Configurable**: Adjust buffer size, bitrate, sample rate and more

## Usage

```bash
# Create a named pipe
mkfifo /tmp/snowboot.in

# Start snowboot (with default settings)
snowboot

# Or with custom settings
snowboot --host icecast.example.com:8000 --mount /my-stream.ogg --user source --password mypass --input-pipe /tmp/snowboot.in --sample-rate 48000 --bitrate 192

# Feed audio to the pipe (in another terminal)
cat audio.ogg > /tmp/snowboot.in
# Or stream from another source
some_audio_generator > /tmp/snowboot.in
```

### Command Line Options

```
OPTIONS:
    --host <HOST[:PORT]>       Icecast server address [default: localhost:8000]
    --mount <PATH>             Mount point path [default: /stream.ogg]
    --user <USERNAME>          Username for server authentication [default: source]
    --password <PASSWORD>      Password for server authentication [default: hackme]
    --input-pipe <PATH>        Path to the input pipe file [default: /tmp/snowboot.in]
    --sample-rate <RATE>       Sample rate in Hz [default: 44100]
    --bitrate <BITRATE>        Bitrate in kbps [default: 320]
    --buffer <SECONDS>         Buffer size in seconds [default: 1.0]
    --log-level <LEVEL>        Log level (trace, debug, info, warn, error) [default: info]
    --help                     Print help
    --version                  Print version
```

## Requirements

- Rust 2024 edition
- An Icecast server to connect to
- Ogg Vorbis encoded input (or no input, for silence generation)

## Dependencies

Snowboot depends on the `oggmux` library for Ogg stream processing and silence generation.

## License

ISC License - See LICENSE file for details.
