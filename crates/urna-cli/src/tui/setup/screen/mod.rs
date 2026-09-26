//! Drawing the installer: the frame (header with the stepper, footer with
//! the keys) around one screen per step. every frame ends the same way:
//! effects over the finished buffer, then the palette folded to the
//! terminal's color depth.

mod done;
mod plan;
mod run;
mod scan;
mod splash;

use std::time::Duration;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;
use ratatui_cheese::help::{Binding, Help, HelpStyles};

use super::ui::{Step, Ui};
use crate::tui::hud::{self};
use crate::tui::{fx, pal};

pub fn draw(f: &mut Frame, ui: &mut Ui, dt: Duration) {
    let area = f.area();
    let buf = f.buffer_mut();
    pal::ground(buf, area);
    if area.height < 6 || area.width < 30 {
        hud::put(
            buf,
            area.x,
            area.y,
            "urna setup: terminal too small",
            pal::err(),
            area.width,
        );
        return;
    }
    header(buf, area, ui);
    let body = Rect::new(area.x, area.y + 1, area.width, area.height - 2);
    footer(
        buf,
        Rect::new(area.x, area.bottom() - 1, area.width, 1),
        ui.step,
    );
    match ui.step {
        Step::Splash => splash::draw(buf, body, ui),
        Step::Scan => scan::draw(buf, body, ui),
        Step::Plan => plan::draw(buf, body, ui),
        Step::Run => run::draw(buf, body, ui),
        Step::Done => done::draw(buf, body, ui),
    }
    if ui.entered {
        ui.entered = false;
        if ui.step != Step::Splash {
            ui.fx.add_unique_effect("page", fx::page_in(body));
        }
        // the symbol's breathing belongs to the screens that draw it.
        if matches!(ui.step, Step::Plan | Step::Run) {
            ui.fx.cancel_unique_effect("shimmer");
        }
    }
    ui.fx.process_effects(dt.into(), buf, area);
    pal::fit(buf, area, ui.depth);
}

fn header(buf: &mut Buffer, area: Rect, ui: &Ui) {
    let y = area.y;
    let x = hud::spans(
        buf,
        area.x + 1,
        y,
        &[("⣾⠃⠘⣷ ", pal::accent()), ("urna setup", pal::title())],
        area.width,
    );
    let steps = [
        ("scan", Step::Scan),
        ("plan", Step::Plan),
        ("install", Step::Run),
        ("verify", Step::Done),
    ];
    let at = steps.iter().position(|(_, s)| *s == ui.step);
    let mut cx = x + 4;
    for (i, (name, _)) in steps.iter().enumerate() {
        let (dot, st) = match at {
            Some(a) if i < a => ("■ ", pal::ok()),
            Some(a) if i == a => ("● ", pal::key()),
            _ => ("○ ", pal::faint()),
        };
        let label = if Some(i) == at {
            pal::key()
        } else {
            pal::faint()
        };
        cx = hud::spans(
            buf,
            cx,
            y,
            &[(dot, st), (name, label)],
            area.right().saturating_sub(cx),
        );
        if i + 1 < steps.len() {
            cx = hud::put(buf, cx + 1, y, "──", Style::new().fg(pal::LINE), 2) + 1;
        }
    }
    let v = format!("v{} ", env!("CARGO_PKG_VERSION"));
    let vx = area.right().saturating_sub(v.len() as u16);
    if vx > cx + 1 {
        hud::put(buf, vx, y, &v, pal::faint(), v.len() as u16);
    }
}

fn footer(buf: &mut Buffer, area: Rect, step: Step) {
    let b = |k: &str, d: &str| Binding::new(k, d);
    let keys = match step {
        Step::Splash => vec![b("enter", "begin"), b("q", "quit")],
        Step::Scan => vec![b("enter", "skip ahead"), b("q", "quit")],
        Step::Plan => vec![
            b("↑↓", "move"),
            b("space", "toggle"),
            b("enter", "install"),
            b("q", "quit"),
        ],
        Step::Run => vec![
            b("↑↓", "scroll log"),
            b("end", "follow"),
            b("ctrl+c", "abort"),
        ],
        Step::Done => vec![b("enter", "finish")],
    };
    let styles = HelpStyles {
        ellipsis: pal::faint(),
        short_key: pal::key(),
        short_desc: pal::faint(),
        short_separator: Style::new().fg(pal::LINE),
        full_key: pal::key(),
        full_desc: pal::faint(),
        full_separator: Style::new().fg(pal::LINE),
    };
    let help = Help::default()
        .bindings(keys)
        .styles(styles)
        .short_separator(" · ");
    (&help).render(
        Rect::new(area.x + 1, area.y, area.width.saturating_sub(2), 1),
        buf,
    );
}
