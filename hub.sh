#!/bin/bash
export PATH=$PATH:/usr/bin:/usr/local/bin:/system/bin:/system/xbin:/data/local/tmp
echo "=========================================="
echo "    🚀 JOSIE-SINGULARITY WEB HUB 🚀       "
echo "        User: BossmanCryto Active         "
echo "=========================================="

# Bypassing local permission blocks by calling direct paths
PORT=$(/system/bin/getprop service.adb.tls.port 2>/dev/null)

if [ -z "$PORT" ] || [ "$PORT" = "0" ]; then
    PORT=$(/system/bin/logcat -d 2>/dev/null | grep "adbd" | sed -n 's/.*listening on port \([0-9]*\).*/\1/p' | tail -n 1)
fi

if [ -n "$PORT" ]; then
    echo "✅ Web-Port Detected: $PORT"
    /system/bin/adb connect 127.0.0.1:$PORT
else
    echo "❌ Handshake Latency: Toggle 'Wireless Debugging' to wake the port."
fi

# Automatic GitHub Repo Status
echo "[*] Checking Web-Sync Status..."
gh auth status
