# Key Storage Configuration

Omikuji supports multiple secure key storage backends to accommodate different deployment scenarios. This guide covers all available options and helps you choose the right one for your environment.

## Understanding Key Storage Options

### Desktop vs Server Environments

The choice of key storage backend depends primarily on your deployment environment:

| Environment | Recommended Backend | Why |
|------------|-------------------|-----|
| Desktop/Local Development | OS Keyring | Native OS integration, user-friendly |
| SSH/Remote Servers | Vault or AWS Secrets | No desktop session required |
| Production Servers | Vault or AWS Secrets | Enterprise features, audit logging |
| CI/CD Pipelines | Environment Variables | Simple, temporary usage |

### Why OS Keyring Doesn't Work on SSH Servers

When you SSH into a server, you don't have access to a desktop session or the D-Bus session bus that OS keyrings require. This causes errors like:

```
Error: No such interface "org.freedesktop.DBus.Properties" on object at path /
```

This is why Omikuji provides Vault and AWS Secrets Manager as alternatives for server deployments.

## Configuration Overview

Key storage is configured in your `omikuji.yaml` file:

```yaml
key_storage:
  # Choose one: "keyring", "vault", "aws-secrets", or "env"
  storage_type: "keyring"
  
  # Backend-specific configuration
  keyring:
    service: "omikuji"
  
  vault:
    url: "https://vault.example.com:8200"
    mount_path: "secret"
    path_prefix: "omikuji/keys"
    auth_method: "token"
    token: "${VAULT_TOKEN}"  # Supports environment variable expansion
    cache_ttl_seconds: 300
  
  aws_secrets:
    region: "us-east-1"  # Optional, uses default AWS config if not specified
    prefix: "omikuji"
    cache_ttl_seconds: 300
```

## Desktop: OS Keyring Setup

### macOS

The OS keyring works out of the box on macOS using the system Keychain.

```bash
# Import a key
omikuji key import --network ethereum-mainnet

# List networks (note: actual key listing not supported by keyring)
omikuji key list

# Export a key (requires confirmation)
omikuji key export --network ethereum-mainnet

# Remove a key
omikuji key remove --network ethereum-mainnet
```

### Linux

On Linux desktop environments, you need a keyring service running:

**GNOME/Ubuntu:**
```bash
# Install if not present
sudo apt-get install gnome-keyring

# Verify it's running
ps aux | grep gnome-keyring-daemon
```

**KDE:**
```bash
# KWallet should be installed by default
# Verify with:
qdbus org.kde.kwalletd5 /modules/kwalletd5
```

### Windows

Windows Credential Manager is used automatically. Keys are stored securely in the Windows Vault.

## HashiCorp Vault Setup

### 1. Vault Server Configuration

First, ensure your Vault server has the KV v2 secrets engine enabled:

```bash
# Enable KV v2 at the desired path
vault secrets enable -path=secret kv-v2

# Create a policy for Omikuji
cat > omikuji-policy.hcl <<EOF
path "secret/data/omikuji/*" {
  capabilities = ["create", "read", "update", "delete", "list"]
}

path "secret/metadata/omikuji/*" {
  capabilities = ["list", "delete"]
}
EOF

vault policy write omikuji omikuji-policy.hcl
```

### 2. Authentication Setup

**Token Authentication (Simple):**
```bash
# Create a token with the omikuji policy
vault token create -policy=omikuji -ttl=8760h

# Export the token
export VAULT_TOKEN="s.XXXXXXXXXXXXXX"
```

**AppRole Authentication (Production):**
```bash
# Enable AppRole
vault auth enable approle

# Create role
vault write auth/approle/role/omikuji \
    token_policies="omikuji" \
    token_ttl=1h \
    token_max_ttl=4h

# Get credentials
vault read auth/approle/role/omikuji/role-id
vault write -f auth/approle/role/omikuji/secret-id
```

### 3. Omikuji Configuration

```yaml
key_storage:
  storage_type: "vault"
  vault:
    url: "https://vault.example.com:8200"
    mount_path: "secret"
    path_prefix: "omikuji/keys"
    auth_method: "token"
    token: "${VAULT_TOKEN}"
    cache_ttl_seconds: 300
```

### 4. Storing Keys in Vault

Keys are automatically stored when Omikuji starts, but you can also manage them manually:

```bash
# Store a key
vault kv put secret/omikuji/keys/ethereum-mainnet \
  private_key="0x..." \
  network="ethereum-mainnet" \
  created_at="2024-01-01T00:00:00Z" \
  created_by="admin"

# Read a key
vault kv get secret/omikuji/keys/ethereum-mainnet

# List all keys
vault kv list secret/omikuji/keys/
```

## AWS Secrets Manager Setup

### 1. IAM Permissions

Create an IAM policy for Omikuji:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "secretsmanager:GetSecretValue",
        "secretsmanager:CreateSecret",
        "secretsmanager:UpdateSecret",
        "secretsmanager:DeleteSecret",
        "secretsmanager:ListSecrets",
        "secretsmanager:TagResource"
      ],
      "Resource": "arn:aws:secretsmanager:*:*:secret:omikuji/*"
    },
    {
      "Effect": "Allow",
      "Action": "secretsmanager:ListSecrets",
      "Resource": "*"
    }
  ]
}
```

### 2. Authentication Methods

**IAM Role (EC2/ECS/Lambda):**
```yaml
# No explicit credentials needed - uses instance profile
key_storage:
  storage_type: "aws-secrets"
  aws_secrets:
    prefix: "omikuji"
    cache_ttl_seconds: 300
```

**IAM User (Development):**
```bash
# Configure AWS credentials
aws configure

# Or use environment variables
export AWS_ACCESS_KEY_ID="AKIA..."
export AWS_SECRET_ACCESS_KEY="..."
export AWS_REGION="us-east-1"
```

### 3. Omikuji Configuration

```yaml
key_storage:
  storage_type: "aws-secrets"
  aws_secrets:
    region: "us-east-1"  # Optional
    prefix: "omikuji"
    cache_ttl_seconds: 300
```

### 4. Managing Secrets

Keys are stored as JSON objects in AWS Secrets Manager:

```bash
# Create a secret
aws secretsmanager create-secret \
  --name "omikuji/ethereum-mainnet" \
  --description "Omikuji private key for ethereum-mainnet" \
  --secret-string '{
    "private_key": "0x...",
    "network": "ethereum-mainnet",
    "created_at": "2024-01-01T00:00:00Z",
    "created_by": "omikuji"
  }'

# Read a secret
aws secretsmanager get-secret-value \
  --secret-id "omikuji/ethereum-mainnet"

# List secrets
aws secretsmanager list-secrets \
  --filters Key=name,Values=omikuji/

# Delete a secret (with 7-day recovery window)
aws secretsmanager delete-secret \
  --secret-id "omikuji/ethereum-mainnet" \
  --recovery-window-in-days 7
```

## Environment Variables (Fallback)

For development or CI/CD pipelines, you can use environment variables:

```yaml
key_storage:
  storage_type: "env"
```

Set environment variables for each network:
```bash
export OMIKUJI_KEY_ETHEREUM_MAINNET="0x..."
export OMIKUJI_KEY_BASE_SEPOLIA="0x..."
```

## Security Best Practices

### General Guidelines

1. **Never commit private keys** to version control
2. **Use least-privilege access** for all storage backends
3. **Enable audit logging** to track key access
4. **Rotate keys regularly** according to your security policy
5. **Use separate keys** for different networks and environments

### Vault-Specific

1. **Use short-lived tokens** with automatic renewal
2. **Enable Vault audit logging**:
   ```bash
   vault audit enable file file_path=/vault/logs/audit.log
   ```
3. **Use namespaces** to isolate environments
4. **Enable versioning** for secret recovery

### AWS-Specific

1. **Use IAM roles** instead of access keys when possible
2. **Enable CloudTrail** for audit logging
3. **Use KMS encryption** for secrets at rest
4. **Set up secret rotation** with Lambda functions
5. **Use resource tags** for cost allocation and access control

## Troubleshooting

### OS Keyring Issues

**"No such interface" error on Linux:**
- You're likely in an SSH session without desktop access
- Switch to Vault or AWS Secrets Manager

**"Keyring is locked" error:**
- Unlock your desktop session
- Or provide the keyring password when prompted

### Vault Issues

**"Permission denied" errors:**
```bash
# Check your token capabilities
vault token capabilities secret/omikuji/keys/
```

**"Connection refused" errors:**
```bash
# Verify Vault is accessible
curl -k https://vault.example.com:8200/v1/sys/health
```

### AWS Issues

**"Access denied" errors:**
```bash
# Verify IAM permissions
aws sts get-caller-identity
aws secretsmanager list-secrets
```

**Region issues:**
```bash
# Explicitly set region in config or environment
export AWS_REGION=us-east-1
```

## Migration Guide

### From Environment Variables to Keyring

```bash
# Use the built-in migration command
omikuji key migrate
```

### From Keyring to Vault

1. Export keys from keyring:
   ```bash
   omikuji key export --network ethereum-mainnet > mainnet.key
   ```

2. Store in Vault:
   ```bash
   vault kv put secret/omikuji/keys/ethereum-mainnet \
     private_key="$(cat mainnet.key)" \
     network="ethereum-mainnet"
   ```

3. Update omikuji.yaml to use Vault backend

4. Verify:
   ```bash
   omikuji run --dry-run
   ```

### From Keyring to AWS

1. Export keys from keyring
2. Create secrets in AWS:
   ```bash
   aws secretsmanager create-secret \
     --name "omikuji/ethereum-mainnet" \
     --secret-string "{\"private_key\": \"$(cat mainnet.key)\"}"
   ```
3. Update configuration
4. Test with dry run

## Performance Considerations

### Caching

All storage backends implement in-memory caching to minimize external API calls:

- Default TTL: 5 minutes
- Configurable via `cache_ttl_seconds`
- Cache is automatically refreshed on miss
- Falls back to cached values on backend errors

### Connection Pooling

- Vault: Uses persistent HTTP connections
- AWS: Uses the AWS SDK connection pool
- Both backends retry failed requests automatically

## Monitoring and Alerting

### Audit Logs

All key operations are logged with:
- Operation type (get, store, remove, list)
- Network name
- Success/failure status
- Timestamp
- Storage backend type

Example log entry:
```
2024-01-01T00:00:00Z INFO omikuji::audit: Key storage operation operation="get_key" network="ethereum-mainnet" success=true storage_type="vault"
```

### Metrics to Monitor

1. **Key retrieval latency** - Track performance degradation
2. **Cache hit rate** - Optimize cache TTL settings
3. **Backend errors** - Detect availability issues
4. **Audit anomalies** - Security monitoring

### Example Prometheus Metrics

```yaml
# Key operation counter
omikuji_key_operations_total{operation="get", backend="vault", status="success"} 42

# Operation duration histogram
omikuji_key_operation_duration_seconds{operation="get", backend="vault"} 0.023

# Cache metrics
omikuji_key_cache_hits_total{backend="vault"} 150
omikuji_key_cache_misses_total{backend="vault"} 10
```

## CLI Key Management

### Using Alternative Storage Backends

Starting with version 0.4.0, the CLI key commands respect your configured storage backend. Simply specify your configuration file:

```bash
# Export a key from Vault
omikuji -c config.yaml key export --network ethereum-mainnet

# Import a key to AWS Secrets Manager
omikuji -c config.yaml key import --network ethereum-mainnet

# List keys from configured backend
omikuji -c config.yaml key list
```

If no configuration file is specified, the CLI will:
1. Check for a config file in the default location (`~/.omikuji/config.yaml`)
2. Fall back to OS keyring if no configuration is found (backward compatibility)

## Summary

Choose your key storage backend based on your deployment environment:

- **Desktop/Development**: OS Keyring for simplicity
- **Servers/Production**: Vault or AWS Secrets Manager for reliability
- **CI/CD**: Environment variables for temporary usage

All backends provide the same interface, making it easy to switch between them as your needs evolve.