# Configure one-way PulseAudio tunnel for playback only
echo "=== Configuring audio for native xpra mode (one-way SSH tunnel) ==="
cat > /usr/lib/systemd/user-preset/99-disable-audio.preset << 'AUDIO_PRESET_EOF'
# Disable local audio services - using SSH-forwarded audio instead
disable pipewire.service
disable pipewire.socket
disable pipewire-pulse.service
disable pipewire-pulse.socket
disable wireplumber.service
AUDIO_PRESET_EOF

# Configure PulseAudio to use SSH tunnel for playback only
mkdir -p /home/user/.config/pulse
cat > /home/user/.config/pulse/default.pa << 'PULSE_CONFIG_EOF'
# Include system defaults
.include /etc/pulse/default.pa

# Create a tunnel sink that sends audio to host via SSH socket
# This is OUTPUT ONLY - VM apps play audio through this to host speakers
# The SSH tunnel at /run/user/1000/pulse/native is forwarded by xpra
load-module module-tunnel-sink-new server=unix:/run/user/1000/pulse/native sink_name=ssh_output

# Set the SSH tunnel as the default output device
set-default-sink ssh_output

# IMPORTANT: Do NOT create a tunnel source (module-tunnel-source-new)
# This is playback only - no audio input through SSH tunnel
PULSE_CONFIG_EOF

chown -R user:user /home/user/.config/pulse
echo "Audio configured: VM output -> SSH tunnel -> host speakers (one-way)"
