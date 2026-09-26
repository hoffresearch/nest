//! The interactive installer: state, input and the event loop. Five steps
//! in an inline viewport: splash, scan, plan, install, verify. The scan
//! starts on launch, so it is usually done by the time the splash is
//! dismissed; the install runs on the `job` worker and this loop only
//! folds its events into state. drawing lives in `view`.

use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui_cheese::spinner::{SpinnerState, SpinnerType};

use super::job::{self, Ev};
use super::plan::{self, Item, Opts, Task};
use super::scan::Scan;
use crate::cmd::health;
use crate::cmd::tone::{self, Depth};
use crate::tui::fx::Fx;
use crate::tui::hud::Badge;
use crate::tui::term::Clock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Splash,
    Scan,
    Plan,
    Run,
    Done,
}

pub struct RunRow {
    pub task: Task,
    pub badge: Option<Badge>,
    pub note: String,
    pub bytes: u64,
    pub total: Option<u64>,
    pub result: Option<String>,
}

pub struct Ui {
    pub step: Step,
    pub since: Instant,
    pub opts: Opts,
    pub scan: Option<Scan>,
    scan_rx: Option<Receiver<Scan>>,
    /// facts revealed so far on the scan screen (they land one by one).
    pub shown: usize,
    /// facts already drawn (the ones past it get the landing effect).
    pub drawn: usize,
    pub plan: Vec<Item>,
    pub cursor: usize,
    pub rows: Vec<RunRow>,
    pub log: Vec<String>,
    /// `None` follows the tail of the log.
    pub scroll: Option<usize>,
    pub health: Vec<health::Row>,
    pub code: i32,
    job: Option<Receiver<Ev>>,
    pub finished: bool,
    /// the view adds the entry effects the first frame after a step change.
    pub entered: bool,
    /// tasks that just finished; the view flashes their rows.
    pub flashes: Vec<(Task, bool)>,
    /// the run row the traveling glint is on.
    pub glint_on: Option<usize>,
    pub spinner: SpinnerState,
    pub fx: Fx,
    pub clock: Clock,
    pub depth: Depth,
    pub(super) quit: Option<i32>,
}

impl Ui {
    pub(super) fn new(opts: Opts) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(Scan::probe());
        });
        Ui {
            step: Step::Splash,
            since: Instant::now(),
            opts,
            scan: None,
            scan_rx: Some(rx),
            shown: 0,
            drawn: 0,
            plan: Vec::new(),
            cursor: 0,
            rows: Vec::new(),
            log: Vec::new(),
            scroll: None,
            health: Vec::new(),
            code: 0,
            job: None,
            finished: false,
            entered: true,
            flashes: Vec::new(),
            glint_on: None,
            spinner: SpinnerState::new(SpinnerType::Dot),
            fx: Fx::default(),
            clock: Clock::new(),
            depth: tone::depth(),
            quit: None,
        }
    }

    fn go(&mut self, step: Step) {
        self.step = step;
        self.since = Instant::now();
        self.entered = true;
    }

    pub fn in_step(&self) -> Duration {
        self.since.elapsed()
    }

    /// Pulls whatever the scan and the worker produced since last frame.
    pub(super) fn pump(&mut self) {
        if let Some(rx) = &self.scan_rx
            && let Ok(scan) = rx.try_recv()
        {
            self.plan = plan::plan(&scan, &self.opts);
            self.scan = Some(scan);
            self.scan_rx = None;
        }
        if self.step == Step::Scan
            && let Some(scan) = &self.scan
        {
            let due = (self.in_step().as_millis() / 140) as usize;
            self.shown = due.min(scan.facts().len());
            if self.shown == scan.facts().len()
                && self.in_step() > Duration::from_millis(140 * self.shown as u64 + 900)
            {
                self.go(Step::Plan);
            }
        }
        loop {
            let ev = match self.job.as_ref().map(|rx| rx.try_recv()) {
                Some(Ok(ev)) => ev,
                Some(Err(TryRecvError::Disconnected)) => {
                    self.job = None;
                    break;
                }
                _ => break,
            };
            self.apply(ev);
        }
        if self.finished && self.step == Step::Run && self.in_step() > Duration::from_millis(400) {
            let since_end = self.rows.iter().all(|r| r.result.is_some());
            if since_end {
                self.go(Step::Done);
            }
        }
    }

    fn row(&mut self, task: Task) -> Option<&mut RunRow> {
        self.rows.iter_mut().find(|r| r.task == task)
    }

    fn apply(&mut self, ev: Ev) {
        match ev {
            Ev::Start(t) => {
                if let Some(r) = self.row(t) {
                    r.badge = Some(Badge::Busy);
                }
            }
            Ev::Bytes(t, n, total) => {
                if let Some(r) = self.row(t) {
                    r.bytes = n;
                    r.total = total;
                }
            }
            Ev::Note(t, note) => {
                if let Some(r) = self.row(t) {
                    r.note = note;
                }
            }
            Ev::Log(line) => self.log.push(crate::tui::txt::home(&line)),
            Ev::Health(rows) => self.health = rows,
            Ev::Done(t, res) => {
                let ok = res.is_ok();
                if let Err((c, _)) = &res
                    && self.code == 0
                {
                    self.code = *c;
                }
                if let Some(r) = self.row(t) {
                    r.badge = Some(if ok { Badge::Ok } else { Badge::Fail });
                    r.result = Some(match res {
                        Ok(m) => m,
                        Err((_, m)) => m,
                    });
                }
                self.flashes.push((t, ok));
            }
            Ev::End => {
                self.finished = true;
                self.since = Instant::now();
            }
        }
    }

    fn start(&mut self) {
        let Some(scan) = self.scan.clone() else {
            return;
        };
        self.code = plan::blocked_code(&self.plan);
        let tasks: Vec<Task> = self
            .plan
            .iter()
            .filter(|i| i.runs())
            .map(|i| i.task)
            .collect();
        self.rows = tasks
            .iter()
            .map(|&task| RunRow {
                task,
                badge: None,
                note: "queued".into(),
                bytes: 0,
                total: None,
                result: None,
            })
            .collect();
        let version = self
            .opts
            .version
            .clone()
            .unwrap_or_else(|| scan.version.to_string());
        self.job = Some(job::spawn(scan, tasks, version));
        self.go(Step::Run);
    }

    pub(super) fn key(&mut self, code: KeyCode, mods: KeyModifiers) {
        if code == KeyCode::Char('c') && mods.contains(KeyModifiers::CONTROL) {
            self.quit = Some(130);
            return;
        }
        let quit = matches!(code, KeyCode::Char('q') | KeyCode::Esc);
        match self.step {
            Step::Splash => match code {
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Right => self.go(Step::Scan),
                _ if quit => self.quit = Some(130),
                _ => {}
            },
            Step::Scan => match code {
                KeyCode::Enter if self.scan.is_some() => self.go(Step::Plan),
                _ if quit => self.quit = Some(130),
                _ => {}
            },
            Step::Plan => match code {
                KeyCode::Up | KeyCode::Char('k') => self.cursor = self.cursor.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => {
                    self.cursor = (self.cursor + 1).min(self.plan.len().saturating_sub(1))
                }
                KeyCode::Char(' ') | KeyCode::Char('x') => {
                    if let Some(i) = self.plan.get_mut(self.cursor)
                        && !i.locked
                        && i.blocked.is_none()
                    {
                        i.on = !i.on;
                    }
                }
                KeyCode::Enter => self.start(),
                _ if quit => self.quit = Some(130),
                _ => {}
            },
            Step::Run => match code {
                KeyCode::Up | KeyCode::Char('k') => {
                    let top = self.scroll.unwrap_or(self.log.len());
                    self.scroll = Some(top.saturating_sub(1));
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.scroll = self.scroll.map(|s| s + 1).filter(|s| *s < self.log.len());
                }
                KeyCode::End => self.scroll = None,
                _ => {}
            },
            Step::Done => {
                if quit || matches!(code, KeyCode::Enter) {
                    self.quit = Some(self.code);
                }
            }
        }
    }
}
