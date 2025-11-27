# Configure SSH and Xpra for seamless window mode with SSH audio forwarding
echo "=== Configuring SSH, Xpra, and audio forwarding ==="

# Install xpra from updates repo (not available during kickstart %packages phase)
echo "Installing xpra from updates repository..."
dnf install -y xpra xorg-x11-server-Xvfb git tar || echo "Warning: xpra installation failed"

# Install xpra-html5 web client (required for browser access)
if [ ! -d /usr/share/xpra/www ]; then
    echo "Installing xpra-html5 web client..."
    mkdir -p /usr/share/xpra/www
    cd /tmp && git clone --depth 1 https://github.com/Xpra-org/xpra-html5.git
    cp -r /tmp/xpra-html5/html5/* /usr/share/xpra/www/
    rm -rf /tmp/xpra-html5
fi

# SSH key for passwordless authentication
mkdir -p /home/user/.ssh
chmod 700 /home/user/.ssh
cat > /home/user/.ssh/authorized_keys << 'SSH_KEY_EOF'
{{SSH_KEY}}
SSH_KEY_EOF
chmod 600 /home/user/.ssh/authorized_keys
chown -R user:user /home/user/.ssh

# Configure SSH server for Unix socket forwarding
cat >> /etc/ssh/sshd_config << 'SSHD_CONFIG_EOF'

# Enable Unix socket forwarding for PulseAudio
StreamLocalBindUnlink yes
AllowStreamLocalForwarding yes
SSHD_CONFIG_EOF

# Enable and start SSH server
systemctl enable sshd
systemctl start sshd

# Allow SSH through firewall
firewall-cmd --permanent --add-service=ssh
firewall-cmd --reload

{{AUDIO_CONFIG}}

# No auto-login needed - Xpra starts its own X server on demand
systemctl set-default multi-user.target
{{WEB_STREAMING_CONFIG}}
{{VIRTIOFS_CONFIG}}
{{VSOCK_CONFIG}}
