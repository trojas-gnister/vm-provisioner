#!/bin/bash
# Start Xvfb in background with large resolution for dynamic resizing
/usr/bin/Xvfb :100 -screen 0 8192x4096x24 +extension RANDR &
XVFB_PID=$!
sleep 2

# Start Openbox window manager (auto-maximizes windows)
export DISPLAY=:100
openbox &
sleep 1

# Note: Apps are managed by systemd user services, not this script
# They start automatically via 'loginctl enable-linger' and WantedBy=default.target

# Start Selkies (foreground) with resize support
exec /opt/selkies-gstreamer/selkies-gstreamer-run \
    --addr=0.0.0.0 \
    --port={{PORT}} \
    --enable_resize=true \
    --enable_clipboard=true \
    --framerate=60 \
    --video_bitrate=8000 \
    --audio_bitrate=128000
