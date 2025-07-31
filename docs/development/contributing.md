# Contributing to Omikuji

Thank you for your interest in contributing to Omikuji! This guide will help you get started with development.

## Code of Conduct

By participating in this project, you agree to maintain a respectful and inclusive environment for all contributors.

## Getting Started

### Prerequisites

- Rust stable (latest version recommended)
- Git
- A GitHub account
- Familiarity with Rust and blockchain concepts

**Important**: Keep your Rust toolchain updated to match GitHub Actions CI:
```bash
rustup update stable
rustup component add clippy rustfmt
```

### Development Setup

1. **Fork and Clone**
   ```bash
   git clone https://github.com/YOUR_USERNAME/omikuji.git
   cd omikuji
   ```

2. **Install Development Tools**
   ```bash
   # Install Rust
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   
   # Install additional tools
   cargo install cargo-watch
   cargo install cargo-edit
   ```

3. **Setup Git Hooks**
   ```bash
   ./.githooks/setup.sh
   ```
   This enables automatic formatting and linting before commits.

4. **Install Local Blockchain (Optional)**
   ```bash
   # Install Foundry for Anvil
   curl -L https://foundry.paradigm.xyz | bash
   foundryup
   ```

## Development Workflow

### 1. Create a Branch

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/issue-description
```

Branch naming conventions:
- `feature/` - New features
- `fix/` - Bug fixes
- `docs/` - Documentation updates
- `refactor/` - Code refactoring
- `test/` - Test additions/fixes

### 2. Make Changes

Follow the coding standards:
- Run `cargo fmt` before committing
- Ensure `cargo clippy` passes with no warnings
- Add tests for new functionality
- Update documentation as needed

### 3. Write Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run tests with output
cargo test -- --nocapture

# Run tests in watch mode
cargo watch -x test
```

### 4. Commit Changes

Write clear, descriptive commit messages:

```bash
# Good
git commit -m "fix: Resolve gas estimation error for EIP-1559 transactions"
git commit -m "feat: Add support for WebSocket RPC connections"
git commit -m "docs: Update configuration examples for Base network"

# Bad
git commit -m "Fixed stuff"
git commit -m "Updates"
```

Commit message format:
- `feat:` - New feature
- `fix:` - Bug fix
- `docs:` - Documentation changes
- `style:` - Code style changes (formatting, etc.)
- `refactor:` - Code refactoring
- `test:` - Test additions or fixes
- `chore:` - Maintenance tasks

### 5. Push and Create PR

```bash
git push origin feature/your-feature-name
```

Then create a Pull Request on GitHub with:
- Clear title describing the change
- Description of what was changed and why
- Reference to any related issues
- Test results or screenshots if applicable

## Code Standards

### Rust Style Guide

We follow the standard Rust style guide with some additions:

1. **Use `cargo fmt`** - Required by git hooks
2. **Pass `cargo clippy`** - No warnings allowed
3. **Document public APIs** - All public functions need doc comments
4. **Error Handling** - Use `Result<T, E>` and `?` operator
5. **Logging** - Use `tracing` for all log output

### Code Organization

```
src/
├── module/
│   ├── mod.rs      # Module exports
│   ├── types.rs    # Type definitions
│   ├── impl.rs     # Implementation
│   └── tests.rs    # Unit tests
```

### Documentation

1. **Code Comments**: Explain "why", not "what"
2. **Doc Comments**: Use `///` for public items
3. **Examples**: Include examples in doc comments
4. **README**: Update if adding new features

Example:
```rust
/// Calculates the percentage deviation between two values.
///
/// # Arguments
/// * `old_value` - The previous value
/// * `new_value` - The current value
///
/// # Returns
/// The percentage change as a float
///
/// # Example
/// ```
/// let deviation = calculate_deviation(100.0, 105.0);
/// assert_eq!(deviation, 5.0);
/// ```
pub fn calculate_deviation(old_value: f64, new_value: f64) -> f64 {
    // Handle edge case where old value is zero
    if old_value == 0.0 {
        return 0.0;
    }
    
    ((new_value - old_value) / old_value) * 100.0
}
```

## Testing Guidelines

### Test Categories

1. **Unit Tests** - Test individual functions
2. **Integration Tests** - Test module interactions
3. **End-to-End Tests** - Test full workflows

### Writing Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_name() {
        // Arrange
        let input = prepare_test_data();
        
        // Act
        let result = function_under_test(input);
        
        // Assert
        assert_eq!(result, expected_value);
    }
    
    #[tokio::test]
    async fn test_async_function() {
        // Test async functions
    }
}
```

### Test Coverage

- Aim for >80% code coverage
- Test edge cases and error conditions
- Use `mockito` for HTTP mocking
- Use `tempfile` for file system tests

## Database and SQLx

### Query Cache Management

Omikuji uses SQLx for database interactions, which validates queries at compile time. To support building releases without database access, we maintain a query cache:

1. **Regenerating Query Cache**: When you modify database queries, regenerate the cache:
   ```bash
   DATABASE_URL="postgresql://omikuji:omikuji_password@localhost:5433/omikuji_db" cargo sqlx prepare
   ```

2. **Committing Changes**: The `.sqlx` directory must be committed to version control
   ```bash
   git add .sqlx/
   git commit -m "chore: Update SQLx query cache"
   ```

3. **Building Releases**: Release builds use offline mode automatically:
   ```bash
   make release  # Uses SQLX_OFFLINE=true
   ```

### Important Notes
- Always regenerate the cache after modifying SQL queries
- The cache ensures release builds work without database access
- CI/CD can build releases without setting up a database

## Performance Considerations

1. **Async/Await**: Use tokio for all I/O operations
2. **Memory**: Avoid unnecessary clones
3. **Allocations**: Minimize heap allocations in hot paths
4. **Dependencies**: Keep dependencies minimal

## Security Guidelines

1. **No Secrets**: Never commit private keys or sensitive data
2. **Input Validation**: Validate all external input
3. **Dependencies**: Keep dependencies updated
4. **Error Messages**: Don't leak sensitive information

## Common Tasks

### Adding a New Configuration Option

1. Update `src/config/models.rs`
2. Update configuration parser
3. Add validation if needed
4. Update documentation in `docs/reference/configuration.md`
5. Add tests

### Adding a New Metric

1. Define metric in `src/metrics/mod.rs`
2. Register in metrics server
3. Update metric in relevant module
4. Document in `docs/guides/prometheus-metrics.md`

### Supporting a New Network

1. Add network configuration example
2. Test with actual RPC endpoint
3. Document any special requirements
4. Update quickstart guide

## Debugging Tips

### Enable Debug Logging

```bash
RUST_LOG=debug cargo run
```

### Use Test Fixtures

Create test data files in `tests/fixtures/`

### Local Blockchain Testing

```bash
# Start Anvil
anvil

# Run against local blockchain
cargo run -- -c test-config.yaml
```

## Release Process

1. Update version in `Cargo.toml`
2. Update CHANGELOG.md
3. Create git tag: `git tag -a v0.2.0 -m "Release v0.2.0"`
4. Push tag: `git push origin v0.2.0`
5. GitHub Actions handles the rest

## Getting Help

- Check existing [issues](https://github.com/ijonas/omikuji/issues)
- Read the [documentation](https://github.com/ijonas/omikuji/tree/main/docs)
- Ask in discussions
- Review recent PRs for examples

## Recognition

Contributors are recognized in:
- GitHub contributors page
- Release notes
- Project documentation

Thank you for contributing to Omikuji!