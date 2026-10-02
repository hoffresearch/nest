//! The model picker: a checkbox per model the payload's catalog offers,
//! with what it installs and downloads, then every model it leaves out and
//! why. nothing here downloads; the run does, after enter.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::tui::hud;
use crate::tui::pal;
use crate::tui::setup::plan;
use crate::tui::setup::ui::Ui;

pub fn draw(buf: &mut Buffer, body: Rect, ui: &mut Ui) {
    let offered = ui.offered();
    let excluded = ui
        .scan
        .as_ref()
        .and_then(|s| s.kit.as_ref())
        .map(|k| k.catalog.excluded.clone())
        .unwrap_or_default();
    let n_on = offered.iter().filter(|e| ui.chosen(&e.name)).count();
    let area = Rect::new(
        body.x + 1,
        body.y,
        body.width.saturating_sub(2),
        body.height,
    );
    let inner = hud::panel(
        buf,
        area,
        "models",
        &format!("{n_on} of {} chosen · a picks all", offered.len()),
        true,
    );
    let w = inner.width.saturating_sub(6);
    let mut y = inner.y;
    for (i, e) in offered.iter().enumerate() {
        if y + 1 >= inner.bottom() {
            break;
        }
        let state = hud::Check {
            checked: ui.chosen(&e.name),
            locked: false,
            cursor: i == ui.pick,
        };
        hud::checkbox(buf, Rect::new(inner.x, y, inner.width, 1), &e.name, state);
        let what = plan::describe(e);
        let what = what
            .split_once(": ")
            .map_or(what.as_str(), |(_, rest)| rest);
        hud::put(buf, inner.x + 6, y + 1, what, pal::faint(), w);
        y += 2;
        if e.remote_code {
            let allowed = ui.opts.allow_remote_code.contains(&e.name);
            let (note, st) = if allowed {
                ("its repo code may run (r withdraws)", pal::ok())
            } else {
                (
                    "runs code from its model repo: r allows it, separately",
                    pal::err(),
                )
            };
            hud::put(buf, inner.x + 6, y, note, st, w);
            y += 1;
        }
        y += 1;
    }
    if excluded.is_empty() || y + 1 >= inner.bottom() {
        return;
    }
    hud::put(buf, inner.x, y, "not offered", pal::dim(), inner.width);
    y += 1;
    for x in &excluded {
        if y >= inner.bottom() {
            break;
        }
        let line = format!("{}: {}", x.name, x.reason);
        let w = inner.width.saturating_sub(2);
        hud::put(buf, inner.x + 2, y, &line, pal::faint(), w);
        y += 1;
    }
}
