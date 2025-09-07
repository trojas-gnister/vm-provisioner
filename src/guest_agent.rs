use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::io::Write;
use std::process::Command;
use std::time::Duration;
use std::thread;

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

/// Tracks application windows in the VM
pub struct GuestAgent {
    host_socket: UnixStream,
    windows: HashMap<u32, WindowInfo>,
    next_window_id: u32,
}

#[derive(Debug, Clone)]
struct WindowInfo {
    id: u32,
    title: String,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    app_name: String,
    pid: u32,
}

impl GuestAgent {
    pub fn new(socket_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let host_socket = UnixStream::connect(socket_path)?;
        
        Ok(Self {
            host_socket,
            windows: HashMap::new(),
            next_window_id: 1,
        })
    }
    
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🪟 Guest Agent started - monitoring application windows");
        
        // Start monitoring processes
        let socket_clone = self.host_socket.try_clone()?;
        thread::spawn(move || {
            Self::monitor_processes(socket_clone);
        });
        
        // Main loop - monitor X11 windows (applications run in Xwayland)
        loop {
            self.scan_windows()?;
            thread::sleep(Duration::from_millis(500));
        }
    }
    
    fn scan_windows(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Use xwininfo to get window list
        let output = Command::new("xwininfo")
            .args(&["-root", "-tree"])
            .output();
            
        let window_list = match output {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
            _ => {
                // Fallback: try wmctrl if available
                return self.scan_windows_wmctrl();
            }
        };
        
        let current_windows = self.parse_xwininfo_output(&window_list)?;
        
        // Detect new windows
        for window in &current_windows {
            if !self.windows.contains_key(&window.id) {
                println!("📱 New window detected: {} ({})", window.title, window.app_name);
                self.send_window_created(&window)?;
                self.windows.insert(window.id, window.clone());
            }
        }
        
        // Detect closed windows
        let current_ids: Vec<u32> = current_windows.iter().map(|w| w.id).collect();
        let closed_windows: Vec<u32> = self.windows.keys()
            .filter(|id| !current_ids.contains(id))
            .cloned()
            .collect();
            
        for window_id in closed_windows {
            println!("🗑️  Window closed: {}", window_id);
            self.send_window_destroyed(window_id)?;
            self.windows.remove(&window_id);
        }
        
        // Detect window changes (title, size, position)
        for current_window in &current_windows {
            let needs_update = if let Some(old_window) = self.windows.get(&current_window.id) {
                let title_changed = old_window.title != current_window.title;
                let size_changed = old_window.width != current_window.width || old_window.height != current_window.height;
                let pos_changed = old_window.x != current_window.x || old_window.y != current_window.y;
                
                // Send change notifications
                if title_changed {
                    self.send_window_title_changed(current_window.id, &current_window.title)?;
                }
                if size_changed {
                    self.send_window_resized(current_window.id, current_window.width, current_window.height)?;
                }
                if pos_changed {
                    self.send_window_moved(current_window.id, current_window.x, current_window.y)?;
                }
                
                title_changed || size_changed || pos_changed
            } else {
                false
            };
            
            // Update stored window info if there were changes
            if needs_update {
                self.windows.insert(current_window.id, current_window.clone());
            }
        }
        
        Ok(())
    }
    
    fn scan_windows_wmctrl(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let output = Command::new("wmctrl")
            .args(&["-l", "-G"])
            .output()?;
            
        if !output.status.success() {
            return Ok(()); // No window manager available
        }
        
        let window_list = String::from_utf8_lossy(&output.stdout);
        let _current_windows = self.parse_wmctrl_output(&window_list)?;
        
        // Same logic as scan_windows for detecting changes
        // ... (implement similar logic)
        
        Ok(())
    }
    
    fn parse_xwininfo_output(&self, output: &str) -> Result<Vec<WindowInfo>, Box<dyn std::error::Error>> {
        let mut windows = Vec::new();
        
        for line in output.lines() {
            if line.contains("children:") || line.trim().is_empty() {
                continue;
            }
            
            // Parse xwininfo tree output
            // Format: "     0x1c00001 \"LibreWolf\": (\"librewolf\" \"LibreWolf\")  800x600+100+50  +100+50"
            if let Some(window_info) = self.parse_xwininfo_line(line) {
                windows.push(window_info);
            }
        }
        
        Ok(windows)
    }
    
    fn parse_xwininfo_line(&self, line: &str) -> Option<WindowInfo> {
        // Extract window ID, title, dimensions and position from xwininfo format
        if let Some(id_start) = line.find("0x") {
            if let Some(id_end) = line[id_start..].find(' ') {
                let id_str = &line[id_start..id_start + id_end];
                if let Ok(id) = u32::from_str_radix(&id_str[2..], 16) {
                    
                    // Extract title
                    if let Some(title_start) = line.find('"') {
                        if let Some(title_end) = line[title_start + 1..].find('"') {
                            let title = line[title_start + 1..title_start + 1 + title_end].to_string();
                            
                            // Extract dimensions and position
                            if let Some(geom_match) = self.extract_geometry(line) {
                                return Some(WindowInfo {
                                    id,
                                    title: title.clone(),
                                    width: geom_match.width,
                                    height: geom_match.height,
                                    x: geom_match.x,
                                    y: geom_match.y,
                                    app_name: self.get_app_name_from_title(&title),
                                    pid: 0, // Will be filled later if needed
                                });
                            }
                        }
                    }
                }
            }
        }
        None
    }
    
    fn parse_wmctrl_output(&self, output: &str) -> Result<Vec<WindowInfo>, Box<dyn std::error::Error>> {
        let mut windows = Vec::new();
        
        for line in output.lines() {
            // wmctrl format: "0x01c00001  0 100 50 800 600 hostname LibreWolf"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 7 {
                if let Ok(id) = u32::from_str_radix(&parts[0][2..], 16) {
                    if let (Ok(x), Ok(y), Ok(width), Ok(height)) = (
                        parts[2].parse::<i32>(),
                        parts[3].parse::<i32>(),
                        parts[4].parse::<u32>(),
                        parts[5].parse::<u32>(),
                    ) {
                        let title = parts[7..].join(" ");
                        windows.push(WindowInfo {
                            id,
                            title: title.clone(),
                            width,
                            height,
                            x,
                            y,
                            app_name: self.get_app_name_from_title(&title),
                            pid: 0,
                        });
                    }
                }
            }
        }
        
        Ok(windows)
    }
    
    fn extract_geometry(&self, line: &str) -> Option<Geometry> {
        // Look for pattern like "800x600+100+50" or "800x600-100-50"
        use regex::Regex;
        let re = Regex::new(r"(\d+)x(\d+)([\+\-]\d+)([\+\-]\d+)").ok()?;
        
        if let Some(caps) = re.captures(line) {
            let width = caps.get(1)?.as_str().parse().ok()?;
            let height = caps.get(2)?.as_str().parse().ok()?;
            let x = caps.get(3)?.as_str().parse().ok()?;
            let y = caps.get(4)?.as_str().parse().ok()?;
            
            return Some(Geometry { width, height, x, y });
        }
        
        None
    }
    
    fn get_app_name_from_title(&self, title: &str) -> String {
        // Extract application name from window title
        match title {
            t if t.contains("LibreWolf") => "librewolf".to_string(),
            t if t.contains("Firefox") => "firefox".to_string(),
            t if t.contains("Chromium") => "chromium".to_string(),
            t if t.contains("LibreOffice") => "libreoffice".to_string(),
            t if t.contains("Visual Studio Code") => "code".to_string(),
            _ => "unknown".to_string(),
        }
    }
    
    fn monitor_processes(mut socket: UnixStream) {
        loop {
            // Monitor process starts/stops
            let output = Command::new("pgrep")
                .args(&["-f", "librewolf|firefox|chromium|libreoffice|code"])
                .output();
                
            if let Ok(out) = output {
                let pids = String::from_utf8_lossy(&out.stdout);
                for pid_str in pids.lines() {
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        // Send application started message
                        let msg = WindowMessage::ApplicationStarted {
                            app_name: "detected".to_string(),
                            pid,
                        };
                        let _ = Self::send_message(&mut socket, &msg);
                    }
                }
            }
            
            thread::sleep(Duration::from_secs(2));
        }
    }
    
    // Message sending methods
    fn send_window_created(&mut self, window: &WindowInfo) -> Result<(), Box<dyn std::error::Error>> {
        let msg = WindowMessage::WindowCreated {
            id: window.id,
            title: window.title.clone(),
            width: window.width,
            height: window.height,
            x: window.x,
            y: window.y,
            app_name: window.app_name.clone(),
        };
        Self::send_message(&mut self.host_socket, &msg)
    }
    
    fn send_window_destroyed(&mut self, id: u32) -> Result<(), Box<dyn std::error::Error>> {
        let msg = WindowMessage::WindowDestroyed { id };
        Self::send_message(&mut self.host_socket, &msg)
    }
    
    fn send_window_moved(&mut self, id: u32, x: i32, y: i32) -> Result<(), Box<dyn std::error::Error>> {
        let msg = WindowMessage::WindowMoved { id, x, y };
        Self::send_message(&mut self.host_socket, &msg)
    }
    
    fn send_window_resized(&mut self, id: u32, width: u32, height: u32) -> Result<(), Box<dyn std::error::Error>> {
        let msg = WindowMessage::WindowResized { id, width, height };
        Self::send_message(&mut self.host_socket, &msg)
    }
    
    fn send_window_title_changed(&mut self, id: u32, title: &str) -> Result<(), Box<dyn std::error::Error>> {
        let msg = WindowMessage::WindowTitleChanged { 
            id, 
            title: title.to_string() 
        };
        Self::send_message(&mut self.host_socket, &msg)
    }
    
    fn send_message(socket: &mut UnixStream, msg: &WindowMessage) -> Result<(), Box<dyn std::error::Error>> {
        let data = bincode::serialize(msg)?;
        let len = data.len() as u32;
        
        // Send length prefix followed by data
        socket.write_all(&len.to_le_bytes())?;
        socket.write_all(&data)?;
        
        Ok(())
    }
}

#[derive(Debug)]
struct Geometry {
    width: u32,
    height: u32,
    x: i32,
    y: i32,
}

// Main function for guest agent binary
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/vm-window-proxy.sock".to_string());
    
    let mut agent = GuestAgent::new(&socket_path)?;
    agent.run()
}