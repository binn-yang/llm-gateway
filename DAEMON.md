# Running LLM Gateway as a Daemon

## Recommended Approach: Use Process Managers

For production deployments, we **strongly recommend** using a process manager instead of the built-in `--daemon` flag:

### systemd (Linux)

Create `/etc/systemd/system/llm-gateway.service`:

```ini
[Unit]
Description=LLM Gateway Service
After=network.target

[Service]
Type=simple
User=llm-gateway
WorkingDirectory=/opt/llm-gateway
ExecStart=/opt/llm-gateway/llm-gateway start
Restart=on-failure
RestartSec=10

# Environment
Environment=LLM_GATEWAY__SERVER__PORT=8080

[Install]
WantedBy=multi-user.target
```

Then:
```bash
sudo systemctl daemon-reload
sudo systemctl enable llm-gateway
sudo systemctl start llm-gateway
sudo systemctl status llm-gateway
```

### launchd (macOS)

Create `~/Library/LaunchAgents/com.llmgateway.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.llmgateway</string>

    <key>Program</key>
    <string>/usr/local/bin/llm-gateway</string>

    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/llm-gateway</string>
        <string>start</string>
    </array>

    <key>WorkingDirectory</key>
    <string>/opt/llm-gateway</string>

    <key>KeepAlive</key>
    <true/>

    <key>StandardOutPath</key>
    <string>/var/log/llm-gateway.log</string>

    <key>StandardErrorPath</key>
    <string>/var/log/llm-gateway.err.log</string>
</dict>
</plist>
```

Then:
```bash
launchctl load ~/Library/LaunchAgents/com.llmgateway.plist
launchctl start com.llmgateway
launchctl list | grep llmgateway
```

## Built-in Daemon Mode (Not Supported on macOS)

⚠️ **Important**: The built-in `--daemon` flag **does not work on macOS** due to tokio runtime limitations with fork().

### macOS Limitation

Tokio (the async runtime) does not support fork() because:
- File descriptors created before fork() become invalid in the child process
- The I/O driver fails with "Bad file descriptor" errors

**Solution for macOS**: Use launchd (see above) instead of `--daemon` mode.

### Linux

The `--daemon` flag works on Linux:
```bash
./llm-gateway start --daemon
./llm-gateway stop
./llm-gateway reload
```

**Note**: Even on Linux, systemd is recommended for production deployments.

## Commands

### Start Server
```bash
# Foreground (recommended for systemd/launchd)
./llm-gateway start

# Daemon mode (development only, see notes above)
./llm-gateway start --daemon

# Custom PID file
./llm-gateway start --daemon --pid-file /var/run/gateway.pid
```

### Stop Server
```bash
# Graceful shutdown (30s timeout)
./llm-gateway stop

# Force kill after timeout
./llm-gateway stop --force --timeout 60

# Custom PID file
./llm-gateway stop --pid-file /var/run/gateway.pid
```

### Reload Configuration
```bash
# Send SIGHUP for zero-downtime config reload
./llm-gateway reload

# Custom PID file
./llm-gateway reload --pid-file /var/run/gateway.pid
```

### Test Configuration
```bash
./llm-gateway test
```

## Logging

### Foreground Mode
Logs go to stdout/stderr (captured by systemd/launchd).

### Daemon Mode
- stdout: `./logs/gateway.out.log`
- stderr: `./logs/gateway.err.log`

## PID File Locations

The gateway automatically selects a writable PID file location:

1. `/var/run/llm-gateway.pid` (if writable)
2. `./run/llm-gateway.pid` (fallback)
3. `./llm-gateway.pid` (last resort)

Override with `--pid-file` flag.

## Troubleshooting

### macOS fork() crashes

**Symptoms**: Process starts but immediately crashes with `objc_initializeAfterForkError`.

**Solution**: Set environment variable before starting:
```bash
export OBJC_DISABLE_INITIALIZE_FORK_SAFETY=YES
```

Or add to your shell profile (`~/.zshrc` or `~/.bashrc`).

### Port already in use

Check if another instance is running:
```bash
lsof -i :8080
```

Kill the process or change the port in `config.toml`.

### Permission denied on PID file

Run with sudo or use a custom PID file location:
```bash
./llm-gateway start --daemon --pid-file ./run/gateway.pid
```

## Production Checklist

- [ ] Use systemd (Linux) or launchd (macOS) instead of `--daemon`
- [ ] Configure automatic restarts on failure
- [ ] Set up log rotation
- [ ] Use a dedicated user account
- [ ] Configure firewall rules
- [ ] Set appropriate file permissions
- [ ] Monitor with `systemctl status` or `launchctl list`
- [ ] Test config reload: `./llm-gateway reload`

