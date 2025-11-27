# Create Openbox config directory and configuration
mkdir -p /home/user/.config/openbox

# Openbox rc.xml with auto-maximize and right-click menu binding
cat > /home/user/.config/openbox/rc.xml << 'OPENBOX_RC_EOF'
<?xml version="1.0" encoding="UTF-8"?>
<openbox_config xmlns="http://openbox.org/3.4/rc">
  <applications>
    <application class="*">
      <maximized>yes</maximized>
    </application>
  </applications>
  <mouse>
    <context name="Root">
      <mousebind button="Right" action="Press">
        <action name="ShowMenu">
          <menu>root-menu</menu>
        </action>
      </mousebind>
    </context>
  </mouse>
</openbox_config>
OPENBOX_RC_EOF

# Openbox menu.xml with terminal option
cat > /home/user/.config/openbox/menu.xml << 'OPENBOX_MENU_EOF'
<?xml version="1.0" encoding="UTF-8"?>
<openbox_menu xmlns="http://openbox.org/3.4/rc">
  <menu id="root-menu" label="Applications">
{{MENU_ITEMS}}
    <separator />
    <item label="Terminal">
      <action name="Execute">
        <execute>xterm</execute>
      </action>
    </item>
  </menu>
</openbox_menu>
OPENBOX_MENU_EOF

chown -R user:user /home/user/.config/openbox

# Create systemd service for Selkies-GStreamer
cat > /etc/systemd/system/selkies-web.service << 'SELKIES_SERVICE_EOF'
[Unit]
Description=Selkies-GStreamer WebRTC Streaming
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=user
Environment=DISPLAY=:100
Environment=XDG_RUNTIME_DIR=/run/user/1000
Environment=PULSE_SERVER=unix:/run/user/1000/pulse/native
Environment=SELKIES_ENCODER=x264enc
Environment=SELKIES_BASIC_AUTH_USER=user
Environment=SELKIES_BASIC_AUTH_PASSWORD={{PASSWORD}}
ExecStart=/home/user/selkies-wrapper.sh
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
SELKIES_SERVICE_EOF

# Enable user lingering for PipeWire
loginctl enable-linger user || true

# Enable the Selkies service to start on boot
systemctl daemon-reload
systemctl enable selkies-web.service

echo "Selkies WebRTC streaming configured on port {{PORT}}"
echo "Access via browser: http://<vm-ip>:{{PORT}}/"
echo "Login: user / {{PASSWORD}}"
