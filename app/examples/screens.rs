//! Headless screenshots of every screen using Slint's software renderer.
//! cargo run -p groupkeep --example screens -- <out_dir> [height]

use groupkeep_app::{controller::Controller, AppWindow};
use groupkeep_core::samples;
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{Platform, WindowAdapter};
use slint::ComponentHandle;
use std::rc::Rc;

struct Headless(Rc<MinimalSoftwareWindow>);
impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

const SCALE: f32 = 2.0;

fn shot(win: &MinimalSoftwareWindow, w: u32, h: u32, path: &str) {
    slint::platform::update_timers_and_animations();
    win.request_redraw();
    let (pw, ph) = ((w as f32 * SCALE) as usize, (h as f32 * SCALE) as usize);
    let mut buf = vec![PremultipliedRgbaColor::default(); pw * ph];
    win.draw_if_needed(|r| {
        r.render(&mut buf, pw);
    });
    let mut img = image::RgbaImage::new(pw as u32, ph as u32);
    for (i, px) in buf.iter().enumerate() {
        img.put_pixel(
            (i % pw) as u32,
            (i / pw) as u32,
            image::Rgba([px.red, px.green, px.blue, 255]),
        );
    }
    img.save(path).unwrap();
    println!("wrote {path}");
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let h: u32 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(860);
    let w = 430u32;
    let win = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless(win.clone()))).unwrap();
    win.dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged {
        scale_factor: SCALE,
    });
    win.set_size(slint::PhysicalSize::new(
        (w as f32 * SCALE) as u32,
        (h as f32 * SCALE) as u32,
    ));

    let ui = AppWindow::new().unwrap();
    let ctl = Controller::setup(&ui);
    ui.show().unwrap();

    for (name, g) in [
        ("vac", samples::vacation()),
        ("meal", samples::meal()),
        ("comm", samples::community()),
    ] {
        ctl.set_group(g.clone(), None);
        for (i, page) in ["group", "people", "expenses", "overrides", "summary"]
            .iter()
            .enumerate()
        {
            ui.set_page(i as i32);
            shot(&win, w, h, &format!("{out}/{name}-{i}-{page}.png"));
        }
        // Open each expense editor.
        for (k, e) in g.expenses.iter().enumerate() {
            ui.invoke_open_expense(e.id as i32);
            shot(&win, w, h, &format!("{out}/{name}-edit{k}.png"));
            ui.invoke_close_expense();
        }
    }
    // Override sheet.
    ctl.set_group(samples::vacation(), None);
    ui.set_page(3);
    ui.invoke_new_override(false);
    shot(&win, w, h, &format!("{out}/vac-ov-sheet.png"));
    ui.invoke_save_override();
    shot(&win, w, h, &format!("{out}/vac-ov-sheet-error.png"));
}
