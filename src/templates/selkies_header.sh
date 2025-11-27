# ===== Selkies-GStreamer WebRTC Streaming Configuration =====
echo "=== Configuring Selkies-GStreamer web streaming on port {{PORT}} ==="

# Install GStreamer dependencies
dnf install -y gstreamer1-plugins-base gstreamer1-plugins-good \
    gstreamer1-plugins-bad-free gstreamer1-plugins-ugly-free \
    python3-pip python3-gobject libXtst libXdamage libXfixes \
    xorg-x11-server-Xvfb xorg-x11-utils pipewire pipewire-pulseaudio || true

# Install RPM Fusion for additional codecs
dnf install -y https://mirrors.rpmfusion.org/free/fedora/rpmfusion-free-release-41.noarch.rpm || true
dnf install -y gstreamer1-plugins-bad-freeworld x264 || true

# Download and install Selkies-GStreamer portable distribution
echo "Downloading Selkies-GStreamer..."
SELKIES_VERSION="1.6.2"
mkdir -p /opt/selkies-gstreamer
curl -fsSL -o /tmp/selkies.tar.gz \
    "https://github.com/selkies-project/selkies-gstreamer/releases/download/v${SELKIES_VERSION}/selkies-gstreamer-portable-v${SELKIES_VERSION}_amd64.tar.gz" || \
curl -fsSL -o /tmp/selkies.tar.gz \
    "https://github.com/selkies-project/selkies/releases/download/v${SELKIES_VERSION}/selkies-gstreamer-portable-v${SELKIES_VERSION}_amd64.tar.gz"
tar -xzf /tmp/selkies.tar.gz -C /opt/selkies-gstreamer --strip-components=1 || true
rm -f /tmp/selkies.tar.gz

# Allow web port and WebRTC ports through firewall
firewall-offline-cmd --add-port={{PORT}}/tcp || firewall-cmd --permanent --add-port={{PORT}}/tcp || true
firewall-offline-cmd --add-port=49152-65535/udp || firewall-cmd --permanent --add-port=49152-65535/udp || true
firewall-offline-cmd --add-port=49152-65535/tcp || firewall-cmd --permanent --add-port=49152-65535/tcp || true

# Create systemd user services for each flatpak app with auto-restart
mkdir -p /home/user/.config/systemd/user

# Fix ownership of .config directory (created as root during kickstart)
chown -R user:user /home/user/.config

{{SYSTEMD_SERVICES}}

# Enable user lingering so services start on boot even without login
loginctl enable-linger user || true

# Enable and start all app services
{{SYSTEMD_ENABLE_COMMANDS}}
