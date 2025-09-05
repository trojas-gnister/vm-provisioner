use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::os::unix::net::UnixStream;
use std::io::{Read, Write};

use wayland_client::{Connection, Dispatch, QueueHandle, protocol::{
    wl_compositor, wl_surface, wl_shm, wl_seat, wl_keyboard, wl_pointer,
    wl_registry, wl_output,
}};
use wayland_protocols::xdg::shell::client::{
    xdg_wm_base, xdg_surface, xdg_toplevel,
};

use serde::{Serialize, Deserialize};

/// Messages exchanged between VM and host
#[derive(Debug, Serialize, Deserialize)]
pub enum WindowMessage {
    // From VM to Host
    CreateWindow { 
        id: u32, 
        title: String,
        width: u32,
        height: u32,
    },
    DestroyWindow { 
        id: u32 
    },
    UpdateTitle { 
        id: u32, 
        title: String 
    },
    ResizeWindow { 
        id: u32, 
        width: u32, 
        height: u32 
    },
    UpdateBuffer { 
        id: u32, 
        buffer_data: Vec<u8> 
    },
    
    // From Host to VM
    WindowResized { 
        id: u32, 
        width: u32, 
        height: u32 
    },
    MouseMove { 
        id: u32, 
        x: f64, 
        y: f64 
    },
    MouseButton { 
        id: u32, 
        button: u32, 
        pressed: bool 
    },
    KeyEvent { 
        id: u32, 
        key: u32, 
        pressed: bool 
    },
    WindowClosed { 
        id: u32 
    },
}

/// Represents a proxied window from a VM
pub struct ProxiedWindow {
    vm_window_id: u32,
    surface: wl_surface::WlSurface,
    xdg_surface: xdg_surface::XdgSurface,
    xdg_toplevel: xdg_toplevel::XdgToplevel,
    width: u32,
    height: u32,
    title: String,
}

/// Main window proxy that manages VM windows on the host
pub struct WindowProxy {
    connection: Connection,
    windows: Arc<Mutex<HashMap<u32, ProxiedWindow>>>,
    vm_connection: Arc<Mutex<UnixStream>>,
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    xdg_wm_base: Option<xdg_wm_base::XdgWmBase>,
}

impl WindowProxy {
    pub fn new(vm_socket_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        // Connect to host Wayland compositor
        let connection = Connection::connect_to_env()?;
        
        // Connect to VM via Unix socket (or virtio channel)
        let vm_connection = UnixStream::connect(vm_socket_path)?;
        
        Ok(Self {
            connection,
            windows: Arc::new(Mutex::new(HashMap::new())),
            vm_connection: Arc::new(Mutex::new(vm_connection)),
            compositor: None,
            shm: None,
            xdg_wm_base: None,
        })
    }
    
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🪟 Window Proxy started");
        
        // Setup Wayland globals
        self.setup_wayland()?;
        
        // Spawn thread to handle VM messages
        let windows = self.windows.clone();
        let vm_conn = self.vm_connection.clone();
        let compositor = self.compositor.clone();
        let xdg_wm_base = self.xdg_wm_base.clone();
        
        std::thread::spawn(move || {
            Self::handle_vm_messages(vm_conn, windows, compositor, xdg_wm_base);
        });
        
        // Main Wayland event loop
        loop {
            self.connection.flush()?;
            
            // Process Wayland events
            let mut event_queue = self.connection.new_event_queue();
            event_queue.blocking_dispatch(&mut AppState::default())?;
        }
    }
    
    fn setup_wayland(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let display = self.connection.display();
        let mut event_queue = self.connection.new_event_queue();
        let qh = event_queue.handle();
        
        // Get registry and bind globals
        let _registry = display.get_registry(&qh, ());
        
        // This would normally bind compositor, shm, xdg_wm_base, etc.
        // Simplified for example
        
        event_queue.blocking_dispatch(&mut AppState::default())?;
        
        Ok(())
    }
    
    fn handle_vm_messages(
        vm_conn: Arc<Mutex<UnixStream>>,
        windows: Arc<Mutex<HashMap<u32, ProxiedWindow>>>,
        compositor: Option<wl_compositor::WlCompositor>,
        xdg_wm_base: Option<xdg_wm_base::XdgWmBase>,
    ) {
        let mut buffer = [0u8; 4096];
        
        loop {
            let mut conn = vm_conn.lock().unwrap();
            
            match conn.read(&mut buffer) {
                Ok(n) if n > 0 => {
                    // Parse message from VM
                    if let Ok(msg) = bincode::deserialize::<WindowMessage>(&buffer[..n]) {
                        Self::handle_vm_message(msg, &windows, &compositor, &xdg_wm_base);
                    }
                }
                _ => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }
    }
    
    fn handle_vm_message(
        msg: WindowMessage,
        windows: &Arc<Mutex<HashMap<u32, ProxiedWindow>>>,
        compositor: &Option<wl_compositor::WlCompositor>,
        xdg_wm_base: &Option<xdg_wm_base::XdgWmBase>,
    ) {
        match msg {
            WindowMessage::CreateWindow { id, title, width, height } => {
                println!("Creating window {} '{}' ({}x{})", id, title, width, height);
                
                // Create Wayland surface and XDG toplevel
                // This is simplified - actual implementation would properly create surfaces
                
                // Store in windows map
                // windows.lock().unwrap().insert(id, proxied_window);
            }
            
            WindowMessage::UpdateBuffer { id, buffer_data } => {
                // Update the window's buffer with new frame data from VM
                if let Some(window) = windows.lock().unwrap().get_mut(&id) {
                    // Create SHM buffer and attach to surface
                    // Copy buffer_data to shared memory
                    // window.surface.attach(buffer)
                    // window.surface.commit()
                }
            }
            
            WindowMessage::ResizeWindow { id, width, height } => {
                if let Some(window) = windows.lock().unwrap().get_mut(&id) {
                    window.width = width;
                    window.height = height;
                    // Notify XDG surface of size change
                }
            }
            
            WindowMessage::UpdateTitle { id, title } => {
                if let Some(window) = windows.lock().unwrap().get_mut(&id) {
                    window.title = title.clone();
                    window.xdg_toplevel.set_title(title);
                }
            }
            
            WindowMessage::DestroyWindow { id } => {
                println!("Destroying window {}", id);
                windows.lock().unwrap().remove(&id);
            }
            
            _ => {} // Host->VM messages, not handled here
        }
    }
    
    fn send_to_vm(&self, msg: WindowMessage) -> Result<(), Box<dyn std::error::Error>> {
        let data = bincode::serialize(&msg)?;
        self.vm_connection.lock().unwrap().write_all(&data)?;
        Ok(())
    }
}

// Simplified Wayland state for event handling
#[derive(Default)]
struct AppState {
    // Would contain actual Wayland state
}

impl Dispatch<wl_registry::WlRegistry, ()> for AppState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        // Handle registry events
    }
}

/// Clipboard proxy for sharing clipboard between host and VM
pub struct ClipboardProxy {
    host_clipboard: Arc<Mutex<String>>,
    vm_connection: Arc<Mutex<UnixStream>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ClipboardMessage {
    SetClipboard(String),
    GetClipboard,
    ClipboardContent(String),
}

impl ClipboardProxy {
    pub fn new(vm_socket_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let vm_connection = UnixStream::connect(vm_socket_path)?;
        
        Ok(Self {
            host_clipboard: Arc::new(Mutex::new(String::new())),
            vm_connection: Arc::new(Mutex::new(vm_connection)),
        })
    }
    
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("📋 Clipboard Proxy started");
        
        // Monitor host clipboard changes using wl-clipboard
        let host_clip = self.host_clipboard.clone();
        std::thread::spawn(move || {
            Self::monitor_host_clipboard(host_clip);
        });
        
        // Handle VM clipboard requests
        let mut buffer = [0u8; 65536]; // Larger buffer for clipboard data
        loop {
            let mut conn = self.vm_connection.lock().unwrap();
            
            match conn.read(&mut buffer) {
                Ok(n) if n > 0 => {
                    if let Ok(msg) = bincode::deserialize::<ClipboardMessage>(&buffer[..n]) {
                        match msg {
                            ClipboardMessage::SetClipboard(content) => {
                                // Set host clipboard
                                *self.host_clipboard.lock().unwrap() = content.clone();
                                Self::set_host_clipboard(&content);
                            }
                            ClipboardMessage::GetClipboard => {
                                // Send current clipboard to VM
                                let content = self.host_clipboard.lock().unwrap().clone();
                                let response = ClipboardMessage::ClipboardContent(content);
                                let data = bincode::serialize(&response).unwrap();
                                let _ = conn.write_all(&data);
                            }
                            _ => {}
                        }
                    }
                }
                _ => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
    }
    
    fn monitor_host_clipboard(clipboard: Arc<Mutex<String>>) {
        // Use wl-paste to monitor clipboard changes
        loop {
            if let Ok(output) = std::process::Command::new("wl-paste")
                .output()
            {
                if output.status.success() {
                    let content = String::from_utf8_lossy(&output.stdout).to_string();
                    *clipboard.lock().unwrap() = content;
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
    
    fn set_host_clipboard(content: &str) {
        // Use wl-copy to set clipboard
        let mut child = std::process::Command::new("wl-copy")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .expect("Failed to start wl-copy");
            
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(content.as_bytes());
        }
        
        let _ = child.wait();
    }
}

/// Main entry point for the host-side VM integration
pub struct VMIntegrationHost {
    window_proxy: Option<WindowProxy>,
    clipboard_proxy: Option<ClipboardProxy>,
    vm_name: String,
}

impl VMIntegrationHost {
    pub fn new(vm_name: String) -> Self {
        Self {
            window_proxy: None,
            clipboard_proxy: None,
            vm_name,
        }
    }
    
    pub fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 Starting VM Integration for: {}", self.vm_name);
        
        // Socket paths for VM communication
        let window_socket = format!("/tmp/{}-window.sock", self.vm_name);
        let clipboard_socket = format!("/tmp/{}-clipboard.sock", self.vm_name);
        
        // Start window proxy in separate thread
        let window_socket_clone = window_socket.clone();
        std::thread::spawn(move || {
            let mut proxy = WindowProxy::new(&window_socket_clone).unwrap();
            proxy.run().unwrap();
        });
        
        // Start clipboard proxy if enabled
        let clipboard_socket_clone = clipboard_socket.clone();
        std::thread::spawn(move || {
            let mut proxy = ClipboardProxy::new(&clipboard_socket_clone).unwrap();
            proxy.run().unwrap();
        });
        
        println!("✅ VM Integration running");
        println!("   Window socket: {}", window_socket);
        println!("   Clipboard socket: {}", clipboard_socket);
        
        // Keep main thread alive
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    }
}
