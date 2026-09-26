//! The ask tab: `urna ask` with a screen. The query embeds offline through
//! the same routed embedder and model gate (`embed_gate::embed_and_search`)
//! on a worker thread; hits come back with their stored canonical text
//! (tier-1, the bytes `cite` returns). left: hits with the exact-rerank
//! score as a bar; right: the selected hit's text, scrollable.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use ratatui_cheese::input::InputState;

use crate::cmd::{agent, embed_gate};

pub struct Answer {
    pub citation: String,
    pub source: String,
    pub score: f32,
    pub text: String,
}

type Reply = Result<(Vec<Answer>, f64), String>;

pub struct Ask {
    pub input: InputState,
    pub answers: Vec<Answer>,
    pub sel: usize,
    pub scroll: u16,
    pub asked: Option<String>,
    pub took_ms: f64,
    /// the last query's error, in words a person can act on.
    pub failed: Option<String>,
    pub(super) pending: Option<(Receiver<Reply>, Instant)>,
}

impl Default for Ask {
    fn default() -> Self {
        let mut input = InputState::new();
        input.set_focused(true);
        Self {
            input,
            answers: Vec::new(),
            sel: 0,
            scroll: 0,
            asked: None,
            took_ms: 0.0,
            failed: None,
            pending: None,
        }
    }
}

impl Ask {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }

    /// Starts a query against `file` on a worker thread.
    pub fn submit(&mut self, file: PathBuf) {
        let q = self.input.value().trim().to_string();
        if q.is_empty() || self.busy() {
            return;
        }
        let (tx, rx) = channel();
        let query = q.clone();
        std::thread::spawn(move || {
            let t0 = Instant::now();
            let _ =
                tx.send(run_query(&file, &query).map(|a| (a, t0.elapsed().as_secs_f64() * 1e3)));
        });
        self.asked = Some(q);
        self.pending = Some((rx, Instant::now()));
    }

    /// Folds a finished query in; returns the error text when it failed.
    pub fn poll(&mut self) -> Option<Result<usize, String>> {
        let (rx, _) = self.pending.as_ref()?;
        let reply = rx.try_recv().ok()?;
        self.pending = None;
        if let Err(e) = &reply {
            self.failed = Some(e.clone());
            self.answers.clear();
        }
        Some(reply.map(|(answers, ms)| {
            self.failed = None;
            self.answers = answers;
            self.took_ms = ms;
            self.sel = 0;
            self.scroll = 0;
            self.answers.len()
        }))
    }
}

fn run_query(file: &PathBuf, query: &str) -> Result<Vec<Answer>, String> {
    (|| -> anyhow::Result<Vec<Answer>> {
        let rt = urna_runtime::MmapUrnaFile::open(file)?;
        let result = embed_gate::embed_and_search(&rt, query, 10, None, None, None)?;
        let texts = agent::retrieve::canonical_texts(file)?;
        let by_id: std::collections::HashMap<&str, &str> = texts
            .iter()
            .map(|(i, t)| (i.as_str(), t.as_str()))
            .collect();
        Ok(result
            .hits
            .iter()
            .map(|h| Answer {
                citation: h.citation_id.clone(),
                source: h.source_uri.clone(),
                score: h.score,
                text: by_id
                    .get(h.chunk_id.as_str())
                    .copied()
                    .unwrap_or("")
                    .to_string(),
            })
            .collect())
    })()
    .map_err(|e| explain(&format!("{e:#}")))
}

/// The engine's error, turned toward what fixes it from this screen.
pub fn explain(err: &str) -> String {
    let fix = "esc, then s runs setup";
    if err.contains("embedder script not found") {
        format!("the offline embedder is not installed here ({fix})")
    } else if err.contains("No module named") || err.contains("failed to spawn embedder") {
        format!("the python env cannot run the embedder: numpy + tokenizers missing ({fix})")
    } else {
        err.lines().next().unwrap_or(err).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    #[test]
    fn the_hit_list_scrolls_to_keep_the_selection_visible() {
        let answers = (0..10)
            .map(|i| Answer {
                citation: format!("urna://c/{i}"),
                source: "s".into(),
                score: 1.0 - i as f32 / 10.0,
                text: format!("hit number {i}"),
            })
            .collect();
        let mut ask = Ask {
            answers,
            sel: 9,
            ..Ask::default()
        };
        let area = Rect::new(0, 0, 100, 16);
        let mut buf = Buffer::empty(area);
        super::super::hits::render(&mut buf, area, &mut ask, "·");
        let screen: String = (0..16)
            .map(|y| {
                (0..100)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect();
        assert!(screen.contains("hit number 9"));
        assert!(!screen.contains("hit number 0"));
    }

    #[test]
    fn explain_points_at_setup_for_install_errors() {
        assert!(
            explain("embedder script not found: x (override with --embedder)").contains("setup")
        );
        assert!(
            explain("embedder failed: ModuleNotFoundError: No module named 'numpy'")
                .contains("numpy")
        );
        assert_eq!(
            explain("model_hash mismatch: a\nb"),
            "model_hash mismatch: a"
        );
    }

    #[test]
    fn a_query_without_an_embedder_fails_with_words_not_a_panic() {
        let golden = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../urna-format/tests/fixtures/golden_v1_minimal.urna");
        let mut ask = Ask::default();
        ask.input.set_value("anything".into());
        ask.submit(golden);
        let mut got = None;
        for _ in 0..500 {
            if let Some(r) = ask.poll() {
                got = Some(r);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // the golden fixture carries a placeholder model, so the gate or the
        // embedder refuses it; either way the ask tab gets a message.
        assert!(got.is_some());
        let area = Rect::new(0, 0, 100, 30);
        let mut buf = Buffer::empty(area);
        super::super::hits::render(&mut buf, area, &mut ask, "·");
    }
}
