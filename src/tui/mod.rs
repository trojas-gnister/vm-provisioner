mod app;
mod event;
mod handler;
mod logger;
mod ui;

use vm_provisioner::error::Result;

pub fn run() -> Result<()> {
    let log_lines = logger::init();
    let mut terminal = ratatui::init();
    let mut app = app::App::new(log_lines);
    app.refresh_vm_list();
    let result = event::run_loop(&mut terminal, &mut app);
    ratatui::restore();

    // Print provisioning logs and error after TUI restores terminal
    if let Ok(buf) = app.log_lines.lock() {
        if !buf.is_empty() {
            eprintln!("\n--- Provisioning Log ---");
            for line in buf.iter() {
                eprintln!("{}", line);
            }
        }
    }
    if let Some(prov) = &app.provisioning {
        if let Ok(err) = prov.error.lock() {
            if let Some(msg) = err.as_ref() {
                eprintln!("\nProvisioning error: {}", msg);
            }
        }
    }

    result
}
