//! `urna tui`: the corpus explorer, full screen. four tabs (home, corpus,
//! ask, health), a file picker overlay, toasts, and tachyonfx on every
//! change of screen. slow work (opening a corpus, asking it, the doctor
//! checks) runs on worker threads; this loop only draws and routes input.
//! `s` hands the terminal to `urna setup` and comes back when it ends.

pub mod ask;
mod chrome;
pub mod corpus;
mod draw;
mod health;
mod hits;
mod home;
mod keys;
mod offer;
mod pick;

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use ratatui::DefaultTerminal;
use ratatui::layout::Rect;
use ratatui_cheese::spinner::{SpinnerState, SpinnerType};
use ratatui_explorer::FileExplorer;

use crate::cmd::pyenv;
use crate::cmd::tone::{self, Depth};
use crate::tui::fx::{self, Fx};
use crate::tui::setup;
use crate::tui::term::{self, Clock};
use crate::tui::toast::{Kind, Toasts};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Home,
    Corpus,
    Ask,
    Health,
}

enum Exit {
    Quit,
    Setup,
}

type Loaded = Result<corpus::Corpus, String>;

struct App {
    tab: Tab,
    entered: bool,
    corpus: Option<corpus::Corpus>,
    loading: Option<(Receiver<Loaded>, PathBuf)>,
    sec_scroll: usize,
    ask: ask::Ask,
    health: health::Health,
    picker: Option<FileExplorer>,
    picker_new: bool,
    files: Vec<(PathBuf, u64)>,
    cwd: PathBuf,
    tab_hits: Vec<(Tab, Rect)>,
    toasts: Toasts,
    fx: Fx,
    clock: Clock,
    spinner: SpinnerState,
    depth: Depth,
    booted: bool,
    area: Rect,
    leaving: Option<(Instant, Exit)>,
}

impl App {
    fn new(file: Option<PathBuf>) -> Self {
        let cwd = std::env::current_dir().unwrap_or_default();
        let mut app = App {
            tab: Tab::Home,
            entered: true,
            corpus: None,
            loading: None,
            sec_scroll: 0,
            ask: ask::Ask::default(),
            health: health::Health::default(),
            picker: None,
            picker_new: false,
            files: home::recent(&cwd),
            cwd,
            tab_hits: Vec::new(),
            toasts: Toasts::default(),
            fx: Fx::default(),
            clock: Clock::new(),
            spinner: SpinnerState::new(SpinnerType::Dot),
            depth: tone::depth(),
            booted: false,
            area: Rect::default(),
            leaving: None,
        };
        app.health.refresh();
        if let Some(f) = file {
            app.open(f);
        }
        app
    }

    fn go(&mut self, tab: Tab) {
        if tab != self.tab {
            self.tab = tab;
            self.entered = true;
            if tab == Tab::Health && !self.health.busy() {
                self.health.refresh();
            }
        }
    }

    fn open(&mut self, path: PathBuf) {
        let (tx, rx) = channel();
        let p = path.clone();
        std::thread::spawn(move || {
            let _ = tx.send(corpus::load(&p).map_err(|e| format!("{e:#}")));
        });
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.toasts.push(format!("opening {name}"), Kind::Info);
        self.loading = Some((rx, path));
    }

    fn leave(&mut self, how: Exit) {
        if self.leaving.is_none() {
            self.leaving = Some((Instant::now(), how));
            self.fx.add_unique_effect("leave", fx::leave(self.area));
        }
    }

    fn pump(&mut self) {
        if let Some((rx, path)) = &self.loading
            && let Ok(res) = rx.try_recv()
        {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            match res {
                Ok(c) => {
                    let kind = if c.valid.is_ok() {
                        Kind::Ok
                    } else {
                        Kind::Warn
                    };
                    self.toasts.push(
                        format!(
                            "{name}: {} chunks · {}",
                            c.facts[0].1,
                            corpus::human(c.size)
                        ),
                        kind,
                    );
                    self.corpus = Some(c);
                    self.sec_scroll = 0;
                    self.ask = ask::Ask::default();
                    self.go(Tab::Corpus);
                }
                Err(e) => self.toasts.push(format!("{name}: {e}"), Kind::Err),
            }
            self.loading = None;
        }
        match self.ask.poll() {
            Some(Ok(0)) => self.toasts.push("no hits", Kind::Warn),
            // a model the catalog offers opens the install panel instead of
            // a toast that would sit on top of it.
            Some(Err(e)) => match e.need.and_then(offer::Offer::new) {
                Some(o) => self.ask.offer = Some(o),
                None => self.toasts.push(e.text, Kind::Err),
            },
            _ => {}
        }
        let installed = self.ask.offer.as_mut().and_then(offer::Offer::poll);
        if let Some(offer::Outcome::Installed(msg)) = installed {
            self.ask.offer = None;
            self.toasts
                .push(format!("installed {msg}; asking again"), Kind::Ok);
            if let Some(c) = &self.corpus {
                self.ask.retry(c.path.clone());
            }
        }
        if self.health.poll() && self.tab == Tab::Health {
            let n = self
                .health
                .rows
                .iter()
                .filter(|r| matches!(r.level, crate::cmd::health::Level::Fail(_)))
                .count();
            if n > 0 {
                self.toasts
                    .push(format!("{n} check(s) failing: s runs setup"), Kind::Warn);
            }
        }
    }

    fn run(mut self, terminal: &mut DefaultTerminal) -> anyhow::Result<(Exit, Option<PathBuf>)> {
        loop {
            self.pump();
            let dt = self.clock.tick();
            self.spinner.tick(dt);
            self.draw(terminal, dt)?;
            if let Some((t0, _)) = &self.leaving
                && t0.elapsed() > Duration::from_millis(380)
            {
                let how = self.leaving.take().map(|(_, h)| h).unwrap_or(Exit::Quit);
                return Ok((how, self.corpus.map(|c| c.path)));
            }
            if let Some(ev) = term::next_event()? {
                keys::route(&mut self, ev);
            }
        }
    }
}

/// Runs the explorer; returns the exit code.
pub fn run(file: Option<PathBuf>) -> anyhow::Result<i32> {
    pyenv::set_quiet(true);
    let mut file = file;
    loop {
        let mut terminal = term::fullscreen()?;
        let res = App::new(file.take()).run(&mut terminal);
        term::leave_fullscreen()?;
        match res? {
            (Exit::Quit, _) => return Ok(0),
            (Exit::Setup, open) => {
                setup::run(setup::Opts::default())?;
                file = open;
            }
        }
    }
}
