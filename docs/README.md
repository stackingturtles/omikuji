# Omikuji Documentation

Welcome to the comprehensive documentation for Omikuji, a lightweight EVM blockchain datafeed provider.

## 📚 Documentation Structure

### 🚀 Getting Started
Start here if you're new to Omikuji.

- **[Installation Guide](getting-started/installation.md)** - Install Omikuji via binary, Docker, or source
- **[Configuration Guide](getting-started/configuration.md)** - Learn how to configure networks and datafeeds  
- **[Quick Start Tutorial](getting-started/quickstart.md)** - Get running with a live feed in 5 minutes

### 📖 Guides
Detailed guides for specific features and use cases.

- **[Gas Configuration](guides/gas-configuration.md)** - Configure gas settings, EIP-1559, fee bumping
- **[Database Setup](guides/database-setup.md)** - PostgreSQL setup for historical data storage
- **[Docker Setup](guides/docker-setup.md)** - Docker and Docker Compose deployment
- **[Prometheus Metrics](guides/prometheus-metrics.md)** - Monitor feeds, gas usage, and system health
- **[Gas Monitoring](guides/gas-monitoring.md)** - Track and analyze gas consumption
- **[Debug Logging](guides/debug-logging.md)** - Troubleshooting with detailed logs
- **[Scheduled Tasks](guides/scheduled-tasks.md)** - Automate smart contract function execution
- **[WebSocket Events](guides/websocket-events.md)** - Real-time event monitoring with WebSocket support

### 📋 Reference
Complete technical reference documentation.

- **[Configuration Reference](reference/configuration.md)** - All configuration options explained
- **[Smart Contracts](reference/contracts.md)** - Supported contracts and integration details
- **[Architecture](reference/architecture.md)** - System design and component overview
- **[Library References](reference/libraries/)** - Documentation for key dependencies (alloy-rs, tokio, etc.)

### 🔧 Development
For contributors and developers.

- **[Contributing Guide](development/contributing.md)** - How to contribute to Omikuji
- **[Testing Guide](development/testing.md)** - Testing strategies and best practices
- **[Git Hooks](development/git-hooks.md)** - Automated code quality checks

## 🎯 Quick Links

### By Use Case

**"I want to..."**

- **Run my first datafeed** → [Quick Start Tutorial](getting-started/quickstart.md)
- **Deploy with Docker** → [Docker Setup](guides/docker-setup.md)
- **Monitor my feeds** → [Prometheus Metrics](guides/prometheus-metrics.md)
- **Store historical data** → [Database Setup](guides/database-setup.md)
- **Optimize gas costs** → [Gas Configuration](guides/gas-configuration.md)
- **Contribute code** → [Contributing Guide](development/contributing.md)

### By Topic

**Configuration**
- [Basic Configuration](getting-started/configuration.md)
- [Complete Reference](reference/configuration.md)
- [Gas Settings](guides/gas-configuration.md)

**Deployment**
- [Binary Installation](getting-started/installation.md#binary-installation)
- [Docker Deployment](guides/docker-setup.md)
- [Building from Source](getting-started/installation.md#building-from-source)

**Monitoring**
- [Metrics Export](guides/prometheus-metrics.md)
- [Gas Tracking](guides/gas-monitoring.md)
- [Debug Logging](guides/debug-logging.md)

**Smart Contracts**
- [Contract Types](reference/contracts.md#supported-contract-types)
- [FluxAggregator](reference/contracts.md#fluxaggregator-interface)
- [Integration Examples](reference/contracts.md#integration-examples)

## 📊 Architecture Overview

```
External APIs → Omikuji → Smart Contracts
                  ↓
              PostgreSQL
                  ↓
              Prometheus
```

Key components:
- **Datafeed Manager** - Orchestrates all feeds
- **Network Manager** - Handles blockchain connections
- **Contract Updater** - Submits transactions
- **Metrics Server** - Exposes Prometheus metrics

See [Architecture Reference](reference/architecture.md) for details.

## 🔍 Finding Information

### Search Tips

1. **Use your browser's search** (Ctrl/Cmd + F) within documents
2. **Check the table of contents** at the top of longer documents
3. **Follow cross-references** between related topics

### Document Conventions

- 📝 **Notes** - Important information
- ⚠️ **Warnings** - Critical warnings
- 💡 **Tips** - Helpful suggestions
- 🔧 **Examples** - Practical code examples

## 🤝 Getting Help

If you can't find what you need:

1. **Search [GitHub Issues](https://github.com/ijonas/omikuji/issues)**
2. **Check [example configurations](https://github.com/ijonas/omikuji/blob/main/example_config.yaml)**
3. **Ask in [Discussions](https://github.com/ijonas/omikuji/discussions)**
4. **Open a [new issue](https://github.com/ijonas/omikuji/issues/new)**

## 📈 Keeping Documentation Updated

This documentation is maintained alongside the code. When contributing:

- Update relevant docs with code changes
- Add examples for new features
- Fix any outdated information you find
- Follow the [Contributing Guide](development/contributing.md)

## 🏗️ Documentation Roadmap

Planned additions:
- Production deployment guide
- Multi-oracle coordination
- Performance tuning guide
- Security best practices
- Video tutorials

---

**Latest Update**: January 2025 | **Omikuji Version**: 0.2.15

For the most up-to-date information, see the [GitHub repository](https://github.com/ijonas/omikuji).