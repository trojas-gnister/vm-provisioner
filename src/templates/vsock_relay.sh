# ===== Vsock Configuration for Network-Disabled VM =====
echo "=== Configuring vsock relay for host-guest communication ==="

# Ensure vsock modules load at boot
echo "vsock" >> /etc/modules-load.d/vsock.conf
echo "virtio_vsockets" >> /etc/modules-load.d/vsock.conf

# Load modules now
modprobe vsock || true
modprobe virtio_vsockets || true

# Install socat (required for vsock SSH relay)
dnf install -y socat || echo "Warning: socat installation failed"

# Create systemd service to relay vsock:22 to sshd
cat > /etc/systemd/system/vsock-ssh-relay.service << 'VSOCK_SERVICE_EOF'
[Unit]
Description=Vsock to SSH Relay
After=sshd.service network.target
Requires=sshd.service

[Service]
Type=simple
ExecStart=/usr/bin/socat VSOCK-LISTEN:22,reuseaddr,fork TCP:127.0.0.1:22
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
VSOCK_SERVICE_EOF

systemctl daemon-reload
systemctl enable vsock-ssh-relay.service
systemctl start vsock-ssh-relay.service

echo "Vsock SSH relay configured on port 22"
