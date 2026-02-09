use std::sync::atomic::Ordering;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::{App, NetworkChoice, Screen};

pub fn handle_key(app: &mut App, key: KeyEvent) {
    match &app.screen {
        Screen::Dashboard => handle_dashboard(app, key),
        Screen::Detail(idx) => {
            let idx = *idx;
            handle_detail(app, key, idx);
        }
        Screen::ConfirmDestroy(idx) => {
            let idx = *idx;
            handle_confirm_destroy(app, key, idx);
        }
        Screen::Create => handle_create(app, key),
        Screen::Provisioning => handle_provisioning(app, key),
    }
}

fn handle_dashboard(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => app.running = false,
        KeyCode::Up | KeyCode::Char('k') => app.select_prev(),
        KeyCode::Down | KeyCode::Char('j') => app.select_next(),
        KeyCode::Enter => {
            if !app.vm_list.is_empty() {
                app.screen = Screen::Detail(app.selected);
            }
        }
        KeyCode::Char('c') => {
            app.reset_create_form();
            app.screen = Screen::Create;
        }
        KeyCode::Char('s') | KeyCode::Char('S') => app.start_selected(),
        KeyCode::Char('x') | KeyCode::Char('X') => app.stop_selected(),
        KeyCode::Char('d') => {
            if !app.vm_list.is_empty() {
                app.screen = Screen::ConfirmDestroy(app.selected);
            }
        }
        _ => {}
    }
}

fn handle_detail(app: &mut App, key: KeyEvent, _idx: usize) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => app.screen = Screen::Dashboard,
        KeyCode::Char('s') | KeyCode::Char('S') => app.start_selected(),
        KeyCode::Char('x') | KeyCode::Char('X') => app.stop_selected(),
        _ => {}
    }
}

fn handle_confirm_destroy(app: &mut App, key: KeyEvent, idx: usize) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            app.destroy_vm(idx);
            app.screen = Screen::Dashboard;
        }
        _ => {
            app.screen = Screen::Dashboard;
        }
    }
}

fn handle_create(app: &mut App, key: KeyEvent) {
    let field = app.create_form.focused_field;
    let field_count = app.create_form.field_count();

    // Ctrl+S to submit
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
        app.start_provisioning();
        return;
    }

    match key.code {
        KeyCode::Esc => {
            app.screen = Screen::Dashboard;
        }
        KeyCode::Tab | KeyCode::Down | KeyCode::Char('j')
            if !matches!(key.code, KeyCode::Char('j') if is_text_field(field, &app.create_form)) =>
        {
            // In text fields, j types; in non-text fields, j navigates
            app.create_form.focused_field = (field + 1) % field_count;
        }
        KeyCode::BackTab | KeyCode::Up | KeyCode::Char('k')
            if !matches!(key.code, KeyCode::Char('k') if is_text_field(field, &app.create_form)) =>
        {
            app.create_form.focused_field = if field == 0 { field_count - 1 } else { field - 1 };
        }
        KeyCode::Tab => {
            app.create_form.focused_field = (field + 1) % field_count;
        }
        KeyCode::BackTab => {
            app.create_form.focused_field = if field == 0 { field_count - 1 } else { field - 1 };
        }
        _ => {
            // Field-specific handling
            // 0=Name, 1=Memory, 2=vCPUs, 3=Disk, 4=SystemPkgs, 5=FlatpakPkgs,
            // 6=Headless, 7=Graphics, 8=Network, 9=BridgeName
            match field {
                0..=5 => handle_text_input(key, get_text_field_mut(field, &mut app.create_form)),
                6 => {
                    if key.code == KeyCode::Char(' ') {
                        app.create_form.headless = !app.create_form.headless;
                    }
                }
                7 => {
                    // Graphics cycle
                    match key.code {
                        KeyCode::Left | KeyCode::Char('h') => {
                            app.create_form.graphics = app.create_form.graphics.prev();
                        }
                        KeyCode::Right | KeyCode::Char('l') => {
                            app.create_form.graphics = app.create_form.graphics.next();
                        }
                        _ => {}
                    }
                }
                8 => {
                    // Network cycle
                    match key.code {
                        KeyCode::Left | KeyCode::Char('h') => {
                            app.create_form.network = app.create_form.network.prev();
                        }
                        KeyCode::Right | KeyCode::Char('l') => {
                            app.create_form.network = app.create_form.network.next();
                        }
                        _ => {}
                    }
                }
                9 => {
                    handle_text_input(key, &mut app.create_form.bridge_name);
                }
                _ => {}
            }
        }
    }
}

fn is_text_field(field: usize, form: &super::app::CreateForm) -> bool {
    matches!(field, 0..=5) || (field == 9 && form.network == NetworkChoice::Bridge)
}

fn get_text_field_mut(field: usize, form: &mut super::app::CreateForm) -> &mut String {
    match field {
        0 => &mut form.name,
        1 => &mut form.memory,
        2 => &mut form.vcpus,
        3 => &mut form.disk,
        4 => &mut form.system_packages,
        5 => &mut form.flatpak_packages,
        9 => &mut form.bridge_name,
        _ => unreachable!(),
    }
}

fn handle_text_input(key: KeyEvent, text: &mut String) {
    match key.code {
        KeyCode::Char(c) => text.push(c),
        KeyCode::Backspace => { text.pop(); }
        _ => {}
    }
}

fn handle_provisioning(app: &mut App, key: KeyEvent) {
    let prov = match &mut app.provisioning {
        Some(p) => p,
        None => return,
    };

    let is_done = prov.done.load(Ordering::SeqCst);

    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            prov.scroll_offset = prov.scroll_offset.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            prov.scroll_offset = prov.scroll_offset.saturating_add(1);
        }
        KeyCode::Esc | KeyCode::Char('q') if is_done => {
            app.screen = Screen::Dashboard;
            app.refresh_vm_list();
        }
        _ => {}
    }
}
