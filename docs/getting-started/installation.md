# Installation Guide

Omikuji can be installed in several ways depending on your needs and environment. This guide covers all available installation methods.

## System Requirements

- **Operating System**: Linux, macOS, or Windows
- **Rust**: Version 1.70 or higher (for building from source)
- **Docker**: Latest version (for Docker installation)
- **PostgreSQL**: Version 12+ (optional, for historical data storage)

## Binary Installation

Pre-built binaries are available for common platforms. This is the quickest way to get started.

### Linux x64

```bash
# Download the binary (requires glibc 2.31+, standard on Ubuntu 20.04+, Debian 11+)
wget https://github.com/ijonas/omikuji/releases/latest/download/omikuji-linux-x64
chmod +x omikuji-linux-x64
sudo mv omikuji-linux-x64 /usr/local/bin/omikuji
```

### macOS (Intel)

```bash
# Download the binary
wget https://github.com/ijonas/omikuji/releases/latest/download/omikuji-macos-x64
chmod +x omikuji-macos-x64
sudo mv omikuji-macos-x64 /usr/local/bin/omikuji
```

### macOS (Apple Silicon)

```bash
# Download the binary
wget https://github.com/ijonas/omikuji/releases/latest/download/omikuji-macos-arm64
chmod +x omikuji-macos-arm64
sudo mv omikuji-macos-arm64 /usr/local/bin/omikuji
```

### Verify Installation

After downloading, verify the checksum:

```bash
# Download checksums file
wget https://github.com/ijonas/omikuji/releases/latest/download/checksums.txt

# Verify checksum (Linux/macOS)
sha256sum -c checksums.txt --ignore-missing
```

## Docker Installation

Docker provides a consistent environment across all platforms.

### Pull the Latest Image

No authentication required for pulling:

```bash
docker pull public.ecr.aws/stackingturtles/omikuji/omikuji-core:latest
```

### Run with Docker

```bash
# Basic usage with configuration file
docker run -v $(pwd)/config.yaml:/config/config.yaml \
           -e OMIKUJI_PRIVATE_KEY=$OMIKUJI_PRIVATE_KEY \
           public.ecr.aws/stackingturtles/omikuji/omikuji-core:latest

# With persistent database
docker run -v $(pwd)/config.yaml:/config/config.yaml \
           -v omikuji_data:/data \
           -e OMIKUJI_PRIVATE_KEY=$OMIKUJI_PRIVATE_KEY \
           -e DATABASE_URL=$DATABASE_URL \
           public.ecr.aws/stackingturtles/omikuji/omikuji-core:latest
```

### Docker Compose

For a complete setup with PostgreSQL, see the [Docker Setup Guide](../guides/docker-setup.md).

## Building from Source

Building from source gives you the latest development version and allows customization.

### Prerequisites

1. Install Rust:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env
   ```

2. Install build dependencies:
   - **Linux**: `build-essential`, `pkg-config`, `libssl-dev`
   - **macOS**: Xcode Command Line Tools
   - **Windows**: Visual Studio Build Tools

### Build Steps

```bash
# Clone the repository
git clone https://github.com/ijonas/omikuji.git
cd omikuji

# Build in release mode
cargo build --release

# Install to system
sudo mv target/release/omikuji /usr/local/bin/
```

### Development Build

For development with debugging symbols:

```bash
# Build with debug symbols
cargo build

# Run directly
cargo run -- --config config.yaml
```

### Setup Git Hooks (Optional)

For contributors, setup pre-commit hooks:

```bash
./.githooks/setup.sh
```

This enables automatic code formatting and linting before commits.

## Verify Installation

Regardless of installation method, verify Omikuji is working:

```bash
# Check version
omikuji --version

# View help
omikuji --help
```

## Next Steps

- [Configuration Guide](configuration.md) - Set up your first datafeed
- [Quick Start Tutorial](quickstart.md) - Get running in 5 minutes
- [Database Setup](../guides/database-setup.md) - Enable historical data storage

## Troubleshooting

### Binary Not Found

If `omikuji: command not found`:
- Ensure `/usr/local/bin` is in your PATH
- Try using the full path: `/usr/local/bin/omikuji`

### Permission Denied

If you get permission errors:
```bash
chmod +x /usr/local/bin/omikuji
```

### Docker Issues

If Docker commands fail:
- Ensure Docker daemon is running
- Check you have permissions: `docker run hello-world`
- On Linux, add user to docker group: `sudo usermod -aG docker $USER`

For more help, see our [GitHub Issues](https://github.com/ijonas/omikuji/issues).