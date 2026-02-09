use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState},
};
use std::sync::atomic::Ordering;
use std::time::Duration;

use super::app::{App, NetworkChoice, Screen};
use vm_provisioner::config::NetworkMode;

const SPINNER_CHARS: &[char] = &['|', '/', '-', '\\'];

pub fn render(frame: &mut Frame, app: &App) {
    match &app.screen {
        Screen::Dashboard => render_dashboard(frame, app),
        Screen::Detail(idx) => render_detail(frame, app, *idx),
        Screen::ConfirmDestroy(idx) => {
            render_dashboard(frame, app);
            render_confirm_destroy(frame, app, *idx);
        }
        Screen::Create => render_create(frame, app),
        Screen::Provisioning => render_provisioning(frame, app),
    }
}

fn render_dashboard(frame: &mut Frame, app: &App) {
    let [title_area, table_area, status_area] = frame.area().layout(&Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ]));

    frame.render_widget(
        "vm-provisioner".bold().into_centered_line(),
        title_area,
    );

    let header = Row::new(["Name", "State", "Mem", "vCPU", "Net"])
        .cyan()
        .bold()
        .bottom_margin(1);

    let rows: Vec<Row> = app
        .vm_list
        .iter()
        .map(|vm| {
            let state_style = match vm.state.as_str() {
                "running" => Style::new().green(),
                "shut off" => Style::new().red(),
                _ => Style::new().yellow(),
            };

            let (mem, vcpus, net) = if let Some(cfg) = &vm.config {
                (
                    format!("{}", cfg.memory_mb),
                    format!("{}", cfg.vcpus),
                    match &cfg.network_mode {
                        NetworkMode::Nat => "NAT".into(),
                        NetworkMode::None => "None".into(),
                        NetworkMode::Bridge(b) => format!("BR:{b}"),
                    },
                )
            } else {
                ("-".into(), "-".into(), "-".into())
            };

            Row::new([
                Cell::from(vm.name.clone()),
                Cell::from(vm.state.clone()).style(state_style),
                Cell::from(mem),
                Cell::from(vcpus),
                Cell::from(net),
            ])
        })
        .collect();

    let widths = [
        Constraint::Min(16),
        Constraint::Length(10),
        Constraint::Length(6),
        Constraint::Length(5),
        Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("VMs"))
        .row_highlight_style(Style::new().reversed())
        .highlight_symbol("> ");

    let mut table_state = TableState::default();
    if !app.vm_list.is_empty() {
        table_state.select(Some(app.selected));
    }
    frame.render_stateful_widget(table, table_area, &mut table_state);

    let status_text = if let Some((msg, instant)) = &app.status_message {
        if instant.elapsed() < Duration::from_secs(5) {
            msg.clone()
        } else {
            default_help()
        }
    } else {
        default_help()
    };

    frame.render_widget(status_text.dark_gray(), status_area);
}

fn default_help() -> String {
    " c:create  s:start  x:stop  d:destroy  Enter:details  q:quit".into()
}

fn render_detail(frame: &mut Frame, app: &App, idx: usize) {
    let vm = match app.vm_list.get(idx) {
        Some(v) => v,
        None => return,
    };

    let state_style = if vm.state == "running" {
        Style::new().green()
    } else {
        Style::new().red()
    };

    let mut lines = vec![
        Line::from(vec!["Name: ".bold(), vm.name.clone().into()]),
        Line::from(vec!["State: ".bold(), Span::styled(vm.state.clone(), state_style)]),
    ];

    if let Some(ip) = &vm.ip {
        lines.push(Line::from(vec!["IP: ".bold(), ip.clone().into()]));
    }

    if let Some(cfg) = &vm.config {
        lines.push(Line::default());
        lines.push(Line::from(vec![
            "Memory: ".bold(),
            format!("{} MB", cfg.memory_mb).into(),
        ]));
        lines.push(Line::from(vec![
            "vCPUs: ".bold(),
            format!("{}", cfg.vcpus).into(),
        ]));
        lines.push(Line::from(vec![
            "Disk: ".bold(),
            format!("{} GB", cfg.disk_size_gb).into(),
        ]));

        lines.push(Line::from(vec![
            "Network: ".bold(),
            Span::from(match &cfg.network_mode {
                NetworkMode::Nat => "NAT".into(),
                NetworkMode::None => "None (airgapped)".into(),
                NetworkMode::Bridge(b) => format!("Bridge ({b})"),
            }),
        ]));

        lines.push(Line::from(vec![
            "Graphics: ".bold(),
            format!("{:?}", cfg.graphics_backend).into(),
        ]));

        if !cfg.system_packages.is_empty() {
            lines.push(Line::default());
            lines.push(Line::from("System Packages:".bold()));
            lines.push(Line::from(format!("  {}", cfg.system_packages.join(", "))));
        }

        if !cfg.flatpak_packages.is_empty() {
            lines.push(Line::from("Flatpak Packages:".bold()));
            lines.push(Line::from(format!(
                "  {}",
                cfg.flatpak_packages.join(", ")
            )));
        }

        if !cfg.usb_devices.is_empty() {
            lines.push(Line::default());
            lines.push(Line::from("USB Devices:".bold()));
            for dev in &cfg.usb_devices {
                lines.push(Line::from(format!(
                    "  {}:{} - {}",
                    dev.vendor_id, dev.product_id, dev.description
                )));
            }
        }

        if !cfg.shared_folders.is_empty() {
            lines.push(Line::default());
            lines.push(Line::from("Shared Folders:".bold()));
            for sf in &cfg.shared_folders {
                let ro = if sf.readonly { " (ro)" } else { "" };
                lines.push(Line::from(format!(
                    "  {} -> {}{ro}",
                    sf.host_path, sf.guest_path
                )));
            }
        }
    }

    let [main_area, help_area] = frame
        .area()
        .layout(&Layout::vertical([Constraint::Min(0), Constraint::Length(1)]));

    let detail = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("VM: {}", vm.name)),
    );
    frame.render_widget(detail, main_area);
    frame.render_widget(" s:start  x:stop  Esc/q:back".dark_gray(), help_area);
}

fn render_confirm_destroy(frame: &mut Frame, app: &App, idx: usize) {
    let name = app
        .vm_list
        .get(idx)
        .map(|v| v.name.as_str())
        .unwrap_or("?");

    let popup = Paragraph::new(format!("Destroy VM '{name}'? y/N"))
        .red()
        .bold()
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title("Confirm"));

    let area = frame
        .area()
        .centered(Constraint::Length(40), Constraint::Length(5));
    frame.render_widget(Clear, area);
    frame.render_widget(popup, area);
}

fn render_create(frame: &mut Frame, app: &App) {
    let form = &app.create_form;

    let [main_area, status_area, help_area] = frame.area().layout(&Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ]));

    let mut lines: Vec<Line> = Vec::new();
    let fields: Vec<(&str, String, bool)> = {
        let mut f = vec![
            ("Name", form.name.clone(), true),
            ("Memory (MB)", form.memory.clone(), true),
            ("vCPUs", form.vcpus.clone(), true),
            ("Disk (GB)", form.disk.clone(), true),
            ("Sys Packages", form.system_packages.clone(), true),
            ("Flatpak Pkgs", form.flatpak_packages.clone(), true),
            (
                "Headless",
                if form.headless { "[x]" } else { "[ ]" }.into(),
                false,
            ),
            (
                "Graphics",
                format!("< {} >", form.graphics.label()),
                false,
            ),
            (
                "Network",
                format!("< {} >", form.network.label()),
                false,
            ),
        ];
        if form.network == NetworkChoice::Bridge {
            f.push(("Bridge Name", form.bridge_name.clone(), true));
        }
        f
    };

    for (i, (label, value, is_text)) in fields.iter().enumerate() {
        let focused = i == form.focused_field;
        let style = if focused {
            Style::new().reversed()
        } else {
            Style::default()
        };

        let display = if *is_text {
            if focused {
                format!("  {:<14} [{}|]", label, value)
            } else {
                format!("  {:<14} [{}]", label, value)
            }
        } else {
            format!("  {:<14} {}", label, value)
        };

        lines.push(Line::styled(display, style));
        lines.push(Line::default());
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Create VM");
    let para = Paragraph::new(lines).block(block);
    frame.render_widget(para, main_area);

    // Status message
    let status_text = if let Some((msg, instant)) = &app.status_message {
        if instant.elapsed() < Duration::from_secs(5) {
            Span::styled(format!(" {}", msg), Style::new().red())
        } else {
            Span::default()
        }
    } else {
        Span::default()
    };
    frame.render_widget(status_text, status_area);

    frame.render_widget(
        " Tab:next  Shift+Tab:prev  Ctrl+S:create  Esc:cancel".dark_gray(),
        help_area,
    );
}

fn render_provisioning(frame: &mut Frame, app: &App) {
    let prov = match &app.provisioning {
        Some(p) => p,
        None => return,
    };

    let is_done = prov.done.load(Ordering::SeqCst);
    let has_error = prov
        .error
        .lock()
        .map(|e| e.is_some())
        .unwrap_or(false);

    let status = if is_done {
        if has_error { "Failed" } else { "Done" }
    } else {
        let idx = (app.last_refresh.elapsed().as_millis() / 250) as usize % SPINNER_CHARS.len();
        // We can't return a temporary, so use a static approach
        match idx % 4 {
            0 => "|",
            1 => "/",
            2 => "-",
            _ => "\\",
        }
    };

    let title = format!("Provisioning: {} [{}]", prov.vm_name, status);

    let [main_area, help_area] = frame.area().layout(&Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
    ]));

    let mut log_text: Vec<Line> = if let Ok(buf) = app.log_lines.lock() {
        buf.iter()
            .flat_map(|l| l.split('\n').map(|s| Line::raw(s.to_string())))
            .collect()
    } else {
        vec![]
    };

    // Append error message if provisioning failed
    if is_done {
        if let Ok(err) = prov.error.lock() {
            if let Some(msg) = err.as_ref() {
                log_text.push(Line::default());
                // Replace literal \n sequences and split into individual lines
                let cleaned = msg.replace("\\n", "\n");
                for line in cleaned.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    log_text.push(Line::styled(
                        trimmed.to_string(),
                        Style::new().red(),
                    ));
                }
            }
        }
    }

    let total_lines = log_text.len() as u16;
    let visible_height = main_area.height.saturating_sub(2); // borders
    let max_scroll = total_lines.saturating_sub(visible_height);
    // Auto-scroll to bottom when not done, respect manual scroll otherwise
    let scroll = if !is_done {
        max_scroll
    } else {
        prov.scroll_offset.min(max_scroll)
    };

    let block = Block::default().borders(Borders::ALL).title(title);
    let para = Paragraph::new(log_text)
        .block(block)
        .wrap(ratatui::widgets::Wrap { trim: false })
        .scroll((scroll, 0));
    frame.render_widget(para, main_area);

    let help = if is_done {
        " Esc:back"
    } else {
        " Provisioning..."
    };
    frame.render_widget(help.dark_gray(), help_area);
}
