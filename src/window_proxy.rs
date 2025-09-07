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

/// Messages sent from guest to host about window state
#[derive(Debug, Serialize, Deserialize)]
pub enum WindowMessage {
    // Window lifecycle
    WindowCreated { 
        id: u32, 
        title: String,
        width: u32,
        height: u32,
        x: i32,
        y: i32,
        app_name: String,
    },
    WindowDestroyed { 
        id: u32 
    },
    WindowMoved { 
        id: u32, 
        x: i32, 
        y: i32 
    },
    WindowResized { 
        id: u32, 
        width: u32, 
        height: u32 
    },
    WindowTitleChanged { 
        id: u32, 
        title: String 
    },
    WindowFocusChanged { 
        id: u32, 
        focused: bool 
    },
    
    // Application lifecycle
    ApplicationStarted { 
        app_name: String,
        pid: u32,
    },
    ApplicationStopped { 
        app_name: String,
        pid: u32,
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
            WindowMessage::WindowCreated { id, title, width, height, x, y, app_name } => {
                println!("🪟 Creating native window for VM window {} '{}' ({}x{}+{}+{}) [{}]", 
                         id, title, width, height, x, y, app_name);
                
                // TODO: Create actual Wayland surface and XDG toplevel
                // For now, just print the window info
                let proxied_window = ProxiedWindow {
                    vm_window_id: id,
                    surface: unsafe { std::mem::zeroed() }, // Placeholder
                    xdg_surface: unsafe { std::mem::zeroed() }, // Placeholder  
                    xdg_toplevel: unsafe { std::mem::zeroed() }, // Placeholder
                    width,
                    height,
                    title: title.clone(),
                };
                
                windows.lock().unwrap().insert(id, proxied_window);
                
                // TODO: Actually create the native window here
                println!("   → Native window created for {}", title);
            }
            
            WindowMessage::WindowDestroyed { id } => {
                println!("🗑️  Destroying native window for VM window {}", id);
                windows.lock().unwrap().remove(&id);
            }
            
            WindowMessage::WindowResized { id, width, height } => {
                if let Some(window) = windows.lock().unwrap().get_mut(&id) {
                    println!("📏 Resizing window {} to {}x{}", id, width, height);
                    window.width = width;
                    window.height = height;
                    // TODO: Resize the actual Wayland surface
                }
            }
            
            WindowMessage::WindowTitleChanged { id, title } => {
                if let Some(window) = windows.lock().unwrap().get_mut(&id) {
                    println!("📝 Window {} title changed to '{}'", id, title);
                    window.title = title.clone();
                    // TODO: Update the actual window title
                    // window.xdg_toplevel.set_title(title);
                }
            }
            
            WindowMessage::WindowMoved { id, x, y } => {
                println!("📍 Window {} moved to position ({}, {})", id, x, y);
                // TODO: Update window position if supported
            }
            
            WindowMessage::WindowFocusChanged { id, focused } => {
                println!("🎯 Window {} focus changed: {}", id, focused);
                // TODO: Update window focus state
            }
            
            WindowMessage::ApplicationStarted { app_name, pid } => {
                println!("🚀 Application started: {} (PID: {})", app_name, pid);
            }
            
            WindowMessage::ApplicationStopped { app_name, pid } => {
                println!("⏹️  Application stopped: {} (PID: {})", app_name, pid);
            }
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
        
        // TCP port for VM communication
        let port = "0.0.0.0:9999";
        
        // Create TCP server
        use std::net::TcpListener;
        let listener = TcpListener::bind(port)?;
        println!("   Listening on TCP port: {}", port);
        
        // Start window proxy server
        let vm_name = self.vm_name.clone();
        std::thread::spawn(move || {
            Self::run_socket_server(listener, vm_name);
        });
        
        println!("✅ VM Integration running");
        println!("   Waiting for guest agent connection...");
        
        // Keep main thread alive
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    }
    
    fn run_socket_server(listener: std::net::TcpListener, vm_name: String) {
        println!("🔌 TCP server started for VM: {} on port 9999", vm_name);
        
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    println!("📡 Guest agent connected from: {:?}!", stream.peer_addr());
                    
                    // Spawn a thread to handle this connection
                    let vm_name_clone = vm_name.clone();
                    std::thread::spawn(move || {
                        if let Err(e) = Self::handle_guest_connection(stream, vm_name_clone) {
                            eprintln!("Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    eprintln!("Connection failed: {}", e);
                }
            }
        }
    }
    
    fn handle_guest_connection(
        mut stream: std::net::TcpStream, 
        vm_name: String
    ) -> Result<(), Box<dyn std::error::Error>> {
        use std::io::Read;
        
        println!("🔄 Handling connection for VM: {}", vm_name);
        let mut buffer = vec![0u8; 4096];
        
        loop {
            // Read message length first
            let mut len_buf = [0u8; 4];
            match stream.read_exact(&mut len_buf) {
                Ok(()) => {
                    let len = u32::from_le_bytes(len_buf) as usize;
                    if len > buffer.len() {
                        buffer.resize(len, 0);
                    }
                    
                    // Read the actual message
                    stream.read_exact(&mut buffer[..len])?;
                    
                    // Deserialize and handle message
                    if let Ok(msg) = bincode::deserialize::<WindowMessage>(&buffer[..len]) {
                        println!("📨 Received message: {:?}", msg);
                        
                        // Handle the message (for now just print)
                        match msg {
                            WindowMessage::WindowCreated { id, title, width, height, x, y, app_name } => {
                                println!("🪟 VM window created: {} '{}' ({}x{}+{}+{}) [{}]", 
                                         id, title, width, height, x, y, app_name);
                                // TODO: Create native Wayland window
                            }
                            WindowMessage::WindowDestroyed { id } => {
                                println!("🗑️  VM window destroyed: {}", id);
                                // TODO: Destroy native window
                            }
                            WindowMessage::ApplicationStarted { app_name, pid } => {
                                println!("🚀 Application started in VM: {} (PID: {})", app_name, pid);
                            }
                            _ => {
                                println!("📦 Other message: {:?}", msg);
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("🔌 Guest agent disconnected: {}", e);
                    break;
                }
            }
        }
        
        Ok(())
    }
}
