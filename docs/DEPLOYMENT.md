# Deployment Guide

## Directory Structure Overview

```
atomiq/
├── src/                   # Source code (DO NOT deploy)
├── deployment/           # Deployment configurations
│   ├── docker/          # Docker & docker-compose
│   ├── nginx/           # Reverse proxy configs
│   ├── certs/           # SSL certificates
│   └── monitoring/      # Prometheus configs
├── scripts/             # Utility scripts
├── docs/                # Documentation (reference only)
└── target/release/      # Built binaries (deploy these)
```

## What to Deploy

### Required Files

- `target/release/atomiq-unified` - Main blockchain binary
- `target/release/atomiq-api` - API server binary
- `atomiq.toml` - Configuration file
- `deployment/` - Full deployment directory

### Optional Files

- `scripts/` - Maintenance scripts
- `logs/` - Create on target (will be populated)
- `DB/` - Create on target (database storage)

## Deployment Methods

### 1. Docker Deployment (Recommended)

```bash
# On your deployment server
cd deployment/docker

# Basic deployment
docker-compose up -d

# With monitoring stack
docker-compose --profile monitoring up -d

# With nginx reverse proxy
docker-compose --profile production up -d

# Full stack
docker-compose --profile monitoring --profile production up -d

# Check status
docker-compose ps

# View logs
docker-compose logs -f blockchain

# Stop services
docker-compose down

# Stop and remove volumes (⚠️ DELETES DATA)
docker-compose down -v
```

### 2. Systemd Service (Linux)

1. **Build Release Binaries**

   ```bash
   cargo build --release --bin atomiq-unified
   cargo build --release --bin atomiq-api
   ```

2. **Copy Files to Server**

   ```bash
   # Create deployment directory
   ssh user@server "mkdir -p /opt/atomiq/{bin,config,data,logs}"

   # Copy binaries
   scp target/release/atomiq-unified user@server:/opt/atomiq/bin/
   scp target/release/atomiq-api user@server:/opt/atomiq/bin/

   # Copy configuration
   scp atomiq.toml user@server:/opt/atomiq/config/

   # Copy deployment configs
   scp -r deployment/ user@server:/opt/atomiq/
   ```

3. **Create Systemd Service**

   Create `/etc/systemd/system/atomiq.service`:

   ```ini
   [Unit]
   Description=Atomiq Blockchain Node
   After=network.target

   [Service]
   Type=simple
   User=atomiq
   Group=atomiq
   WorkingDirectory=/opt/atomiq
   ExecStart=/opt/atomiq/bin/atomiq-unified
   Restart=always
   RestartSec=10
   StandardOutput=append:/opt/atomiq/logs/atomiq.log
   StandardError=append:/opt/atomiq/logs/atomiq-error.log

   # Security
   NoNewPrivileges=true
   PrivateTmp=true
   ProtectSystem=strict
   ProtectHome=true
   ReadWritePaths=/opt/atomiq/data /opt/atomiq/logs

   [Install]
   WantedBy=multi-user.target
   ```

4. **Enable and Start**
   ```bash
   sudo systemctl daemon-reload
   sudo systemctl enable atomiq
   sudo systemctl start atomiq
   sudo systemctl status atomiq
   ```

### 3. Manual Deployment

```bash
# On target server
cd /opt/atomiq

# Start blockchain
./bin/atomiq-unified &

# Or with nohup
nohup ./bin/atomiq-unified > logs/blockchain.log 2>&1 &

# Check if running
ps aux | grep atomiq-unified
lsof -i :8080
```

## Environment Configuration

### Production Settings (`atomiq.toml`)

```toml
[blockchain]
chain_id = 1
max_transactions_per_block = 1000

[storage]
data_directory = "/opt/atomiq/data/blockchain"
write_buffer_size_mb = 128
compression_type = "Lz4"
clear_on_start = false  # ⚠️ NEVER true in production

[consensus]
mode = "FullHotStuff"  # Use BFT consensus

[network]
mode = "MultiValidator"
host = "0.0.0.0"
port = 8080

[monitoring]
enable_logging = true
enable_metrics = true
```

## Security Checklist

- [ ] Use HTTPS/TLS for API endpoints
- [ ] Configure firewall (only allow necessary ports)
- [ ] Set up SSL certificates (Let's Encrypt)
- [ ] Enable API authentication
- [ ] Configure rate limiting
- [ ] Set up monitoring and alerts
- [ ] Regular backups of DB directory
- [ ] Use non-root user for service
- [ ] Keep dependencies updated
- [ ] Enable audit logging

## Firewall Configuration

```bash
# Allow SSH
sudo ufw allow 22/tcp

# Allow HTTP/HTTPS (if using nginx)
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp

# Allow blockchain API (or use nginx proxy)
sudo ufw allow 8080/tcp

# Allow Prometheus (internal only)
sudo ufw allow from 10.0.0.0/8 to any port 9090

# Enable firewall
sudo ufw enable
```

## SSL Certificate Setup

### Using Let's Encrypt (Recommended)

```bash
# Install certbot
sudo apt-get install certbot

# Get certificate
sudo certbot certonly --standalone -d blockchain.yourdomain.com

# Certificates will be at:
# /etc/letsencrypt/live/blockchain.yourdomain.com/fullchain.pem
# /etc/letsencrypt/live/blockchain.yourdomain.com/privkey.pem

# Auto-renewal (already configured by certbot)
sudo certbot renew --dry-run
```

### Using Self-Signed (Development)

```bash
cd deployment/certs

openssl req -x509 -newkey rsa:4096 \
  -keyout privkey.pem \
  -out fullchain.pem \
  -days 365 \
  -nodes \
  -subj "/CN=blockchain.local"
```

## Nginx Configuration

Edit `deployment/nginx/nginx.conf`:

```nginx
upstream blockchain {
    server blockchain:8080;
    # For multiple nodes:
    # server blockchain1:8080;
    # server blockchain2:8080;
    # server blockchain3:8080;
}

server {
    listen 80;
    server_name blockchain.yourdomain.com;
    return 301 https://$server_name$request_uri;
}

server {
    listen 443 ssl http2;
    server_name blockchain.yourdomain.com;

    ssl_certificate /etc/letsencrypt/live/blockchain.yourdomain.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/blockchain.yourdomain.com/privkey.pem;

    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers HIGH:!aNULL:!MD5;

    location / {
        proxy_pass http://blockchain;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # WebSocket support
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }

    # Rate limiting
    limit_req_zone $binary_remote_addr zone=api:10m rate=100r/s;
    limit_req zone=api burst=200 nodelay;
}
```

## Monitoring Setup

### Prometheus Configuration

Edit `deployment/monitoring/prometheus.yml`:

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: "atomiq"
    static_configs:
      - targets: ["blockchain:9090"]
        labels:
          instance: "atomiq-node-1"
```

### Grafana Dashboard

1. Add Prometheus data source: `http://prometheus:9090`
2. Import dashboard from `deployment/monitoring/grafana-dashboard.json`
3. Configure alerts for:
   - High error rate
   - Low TPS
   - High memory usage
   - Disk space low

## Backup Strategy

### Database Backup

```bash
#!/bin/bash
# backup.sh

BACKUP_DIR="/backup/atomiq"
DATE=$(date +%Y%m%d_%H%M%S)
DB_DIR="/opt/atomiq/data/blockchain"

# Create backup directory
mkdir -p $BACKUP_DIR

# Stop service temporarily (optional, for consistent backup)
sudo systemctl stop atomiq

# Backup database
tar -czf $BACKUP_DIR/blockchain_$DATE.tar.gz -C $DB_DIR .

# Start service
sudo systemctl start atomiq

# Keep only last 7 days
find $BACKUP_DIR -name "blockchain_*.tar.gz" -mtime +7 -delete

echo "Backup completed: blockchain_$DATE.tar.gz"
```

### Automated Backups (Cron)

```bash
# Add to crontab
crontab -e

# Backup daily at 2 AM
0 2 * * * /opt/atomiq/scripts/backup.sh
```

## Health Checks

### Manual Checks

```bash
# Check service status
systemctl status atomiq

# Check API health
curl http://localhost:8080/health

# Check metrics
curl http://localhost:8080/metrics | head -20

# Check logs
tail -f /opt/atomiq/logs/atomiq.log

# Check disk space
df -h /opt/atomiq/data

# Check memory
free -h

# Check process
ps aux | grep atomiq
```

### Automated Monitoring

Create `/opt/atomiq/scripts/healthcheck.sh`:

```bash
#!/bin/bash

# Check if service is running
if ! systemctl is-active --quiet atomiq; then
    echo "ERROR: Atomiq service is not running"
    # Send alert (email, slack, etc.)
    exit 1
fi

# Check API health
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:8080/health)
if [ "$HTTP_CODE" != "200" ]; then
    echo "ERROR: Health check failed (HTTP $HTTP_CODE)"
    exit 1
fi

# Check disk space (alert if > 80%)
DISK_USAGE=$(df /opt/atomiq/data | tail -1 | awk '{print $5}' | sed 's/%//')
if [ "$DISK_USAGE" -gt 80 ]; then
    echo "WARNING: Disk usage is ${DISK_USAGE}%"
fi

echo "OK: All checks passed"
```

Run every 5 minutes:

```bash
*/5 * * * * /opt/atomiq/scripts/healthcheck.sh
```

## Troubleshooting

### Service Won't Start

```bash
# Check logs
sudo journalctl -u atomiq -n 50

# Check configuration
./bin/atomiq-unified --help

# Verify permissions
ls -la /opt/atomiq
sudo chown -R atomiq:atomiq /opt/atomiq
```

### High Memory Usage

```bash
# Check current usage
ps aux | grep atomiq-unified
free -h

# Adjust RocksDB settings in atomiq.toml
[storage]
write_buffer_size_mb = 32
max_write_buffer_number = 2
```

### Slow Performance

```bash
# Check metrics
curl http://localhost:8080/metrics | grep atomiq_transactions_per_second

# Check system resources
htop
iostat -x 1

# Optimize storage
[storage]
compression_type = "Lz4"  # Faster than Zstd
```

### Database Corruption

```bash
# Backup current DB
cp -r /opt/atomiq/data/blockchain /opt/atomiq/data/blockchain.backup

# Try RocksDB repair
# (requires compilation with rocksdb tools)

# If all else fails, resync from genesis
rm -rf /opt/atomiq/data/blockchain
sudo systemctl restart atomiq
```

## Upgrade Procedure

1. **Backup Current State**

   ```bash
   ./scripts/backup.sh
   ```

2. **Stop Service**

   ```bash
   sudo systemctl stop atomiq
   ```

3. **Replace Binaries**

   ```bash
   cp target/release/atomiq-unified /opt/atomiq/bin/
   cp target/release/atomiq-api /opt/atomiq/bin/
   ```

4. **Update Configuration** (if needed)

   ```bash
   cp atomiq.toml /opt/atomiq/config/
   ```

5. **Start Service**

   ```bash
   sudo systemctl start atomiq
   ```

6. **Verify**
   ```bash
   sudo systemctl status atomiq
   curl http://localhost:8080/health
   ```

## Rollback Procedure

```bash
# Stop service
sudo systemctl stop atomiq

# Restore backup
rm -rf /opt/atomiq/data/blockchain
tar -xzf /backup/atomiq/blockchain_YYYYMMDD_HHMMSS.tar.gz -C /opt/atomiq/data/blockchain

# Restore old binary (keep versioned backups)
cp /opt/atomiq/bin/atomiq-unified.v1.0.0 /opt/atomiq/bin/atomiq-unified

# Start service
sudo systemctl start atomiq
```

## Production Checklist

Before going live:

- [ ] TLS/SSL configured and tested
- [ ] Firewall configured
- [ ] Monitoring and alerts set up
- [ ] Backup strategy implemented and tested
- [ ] Health checks automated
- [ ] Resource limits configured
- [ ] Documentation updated
- [ ] Incident response plan ready
- [ ] Team trained on operations
- [ ] Tested rollback procedure

## Support

For deployment issues:

- Check logs: `/opt/atomiq/logs/`
- Review configuration: `/opt/atomiq/config/atomiq.toml`
- Test health: `curl http://localhost:8080/health`
- Contact: support@yourorg.com
