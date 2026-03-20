use anyhow::Result;
use std::time::{Duration, Instant};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow},
    window::WindowId,
};

// ── daemon state ──────────────────────────────────────────────────────────────

fn daemon_pid() -> Option<i32> {
    let home = std::env::var("HOME").ok()?;
    let path = std::path::PathBuf::from(home)
        .join(".local/share/wisprnito/wisprnito.pid");
    let s = std::fs::read_to_string(path).ok()?;
    let pid: i32 = s.trim().parse().ok()?;
    let alive = unsafe { libc::kill(pid, 0) == 0 };
    if alive { Some(pid) } else { None }
}

fn daemon_toggle() {
    let exe = std::env::current_exe().unwrap_or_else(|_| "/usr/local/bin/wisprnito".into());
    if daemon_pid().is_some() {
        let _ = std::process::Command::new(&exe).arg("stop").spawn();
    } else {
        let _ = std::process::Command::new(&exe).arg("start").spawn();
    }
}

// ── icon generation ───────────────────────────────────────────────────────────

fn make_icon(running: bool) -> Icon {
    let size = 22u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let (r, g, b): (u8, u8, u8) = if running { (50, 205, 100) } else { (150, 150, 150) };
    let cx = size as f32 / 2.0;
    let cy = size as f32 / 2.0;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let i = ((y * size + x) * 4) as usize;
            if dist < 7.0 {
                // filled circle (body)
                rgba[i] = r;
                rgba[i + 1] = g;
                rgba[i + 2] = b;
                rgba[i + 3] = 255;
            } else if dist < 8.5 {
                // anti-aliased edge
                let alpha = ((8.5 - dist) / 1.5 * 255.0) as u8;
                rgba[i] = r;
                rgba[i + 1] = g;
                rgba[i + 2] = b;
                rgba[i + 3] = alpha;
            }
        }
    }
    Icon::from_rgba(rgba, size, size).expect("icon creation failed")
}

// ── app struct ────────────────────────────────────────────────────────────────

struct MenuBarApp {
    tray: Option<TrayIcon>,
    status_item: Option<MenuItem>,
    toggle_item: Option<MenuItem>,
    toggle_id: Option<tray_icon::menu::MenuId>,
    quit_id: Option<tray_icon::menu::MenuId>,
    last_check: Instant,
    running: bool,
}

impl MenuBarApp {
    fn new() -> Self {
        Self {
            tray: None,
            status_item: None,
            toggle_item: None,
            toggle_id: None,
            quit_id: None,
            last_check: Instant::now() - Duration::from_secs(10),
            running: false,
        }
    }

    fn refresh_state(&mut self) {
        let now_running = daemon_pid().is_some();
        if now_running != self.running {
            self.running = now_running;
            if let Some(item) = &self.status_item {
                item.set_text(if now_running {
                    "● Running"
                } else {
                    "○ Stopped"
                });
            }
            if let Some(item) = &self.toggle_item {
                item.set_text(if now_running { "Stop" } else { "Start" });
            }
            if let Some(tray) = &self.tray {
                let _ = tray.set_icon(Some(make_icon(now_running)));
                let _ = tray.set_tooltip(Some(if now_running {
                    "Wisprnito — Running"
                } else {
                    "Wisprnito — Stopped"
                }));
            }
        }
        self.last_check = Instant::now();
    }
}

impl ApplicationHandler for MenuBarApp {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        if self.tray.is_some() {
            return;
        }
        let running = daemon_pid().is_some();
        self.running = running;

        let status_item = MenuItem::new(
            if running { "● Running" } else { "○ Stopped" },
            false,
            None,
        );
        let toggle_item = MenuItem::new(
            if running { "Stop" } else { "Start" },
            true,
            None,
        );
        let quit_item = MenuItem::new("Quit", true, None);

        self.toggle_id = Some(toggle_item.id().clone());
        self.quit_id = Some(quit_item.id().clone());

        let menu = Menu::new();
        let _ = menu.append_items(&[
            &status_item,
            &PredefinedMenuItem::separator(),
            &toggle_item,
            &PredefinedMenuItem::separator(),
            &quit_item,
        ]);

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon(make_icon(running))
            .with_tooltip(if running {
                "Wisprnito — Running"
            } else {
                "Wisprnito — Stopped"
            })
            .build()
            .expect("failed to create tray icon");

        self.tray = Some(tray);
        self.status_item = Some(status_item);
        self.toggle_item = Some(toggle_item);
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Handle menu clicks
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if Some(&event.id) == self.toggle_id.as_ref() {
                daemon_toggle();
                // Force an immediate state refresh after a brief delay
                self.last_check = Instant::now() - Duration::from_secs(10);
            } else if Some(&event.id) == self.quit_id.as_ref() {
                event_loop.exit();
                return;
            }
        }

        // Poll daemon state every 2s
        if self.last_check.elapsed() > Duration::from_secs(2) {
            self.refresh_state();
        }

        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(200),
        ));
    }
}

// ── entry point ───────────────────────────────────────────────────────────────

pub fn run() -> Result<()> {
    use winit::event_loop::EventLoop;
    use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};

    let event_loop = EventLoop::builder()
        .with_activation_policy(ActivationPolicy::Accessory) // no dock icon
        .build()?;

    let mut app = MenuBarApp::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
