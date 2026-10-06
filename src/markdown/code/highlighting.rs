use super::*;
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub(super) struct ColorSpan {
    pub range: Range<usize>,
    pub color: Color32,
}

type ColoredRows = Vec<Vec<ColorSpan>>;
type Key = (u64, String, u8);

struct Request {
    id: u64,
    code: String,
    language: String,
    theme: AppTheme,
    ctx: egui::Context,
}

struct Batch {
    id: u64,
    first: usize,
    rows: ColoredRows,
}

struct State {
    key: Key,
    id: u64,
    rows: Arc<ColoredRows>,
}

struct Worker {
    sender: Sender<()>,
    pending: Arc<Mutex<Option<Request>>>,
    receiver: Receiver<Batch>,
    latest: Arc<AtomicU64>,
    state: Mutex<Option<State>>,
}

impl Worker {
    fn new() -> Self {
        let (sender, requests) = bounded::<()>(1);
        let pending = Arc::new(Mutex::new(None::<Request>));
        let worker_pending = pending.clone();
        let (results, receiver) = unbounded();
        let latest = Arc::new(AtomicU64::new(0));
        let worker_latest = latest.clone();
        std::thread::spawn(move || {
            while requests.recv().is_ok() {
                let Some(request) = worker_pending
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .take()
                else {
                    continue;
                };
                if request.id != worker_latest.load(Ordering::Acquire) {
                    continue;
                }
                let syntax_set = get_syntax_set();
                let themes = get_theme_set();
                let theme_name = if request.theme == AppTheme::Dark {
                    "base16-eighties.dark"
                } else {
                    "InspiredGitHub"
                };
                let syntax = find_syntax_by_lang(&request.language, syntax_set);
                let mut highlighter = HighlightLines::new(syntax, &themes.themes[theme_name]);
                let limit = if request.code.len() > 300 * 1024 {
                    200
                } else {
                    2000
                };
                let mut batch = Vec::new();
                let mut first = 0;
                for (index, line) in request.code.split_inclusive('\n').take(limit).enumerate() {
                    if request.id != worker_latest.load(Ordering::Acquire) {
                        break;
                    }
                    let mut offset = 0;
                    let spans = highlighter
                        .highlight_line(line, syntax_set)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|(style, text)| {
                            let range = offset..offset + text.len();
                            offset = range.end;
                            ColorSpan {
                                range,
                                color: Color32::from_rgb(
                                    style.foreground.r,
                                    style.foreground.g,
                                    style.foreground.b,
                                ),
                            }
                        })
                        .collect();
                    batch.push(spans);
                    // Publish the first screen promptly, then amortize updates.
                    if batch.len() == 32 || index + 1 == limit {
                        if request.id != worker_latest.load(Ordering::Acquire) {
                            break;
                        }
                        let count = batch.len();
                        if results
                            .send(Batch {
                                id: request.id,
                                first,
                                rows: std::mem::take(&mut batch),
                            })
                            .is_err()
                        {
                            return;
                        }
                        first += count;
                        request.ctx.request_repaint();
                    }
                }
                if !batch.is_empty() && request.id == worker_latest.load(Ordering::Acquire) {
                    if results
                        .send(Batch {
                            id: request.id,
                            first,
                            rows: batch,
                        })
                        .is_err()
                    {
                        break;
                    }
                    request.ctx.request_repaint();
                }
            }
        });
        Self {
            sender,
            pending,
            receiver,
            latest,
            state: Mutex::new(None),
        }
    }

    fn snapshot(
        &self,
        ctx: &egui::Context,
        hash: u64,
        code: &str,
        language: &str,
        theme: AppTheme,
    ) -> Arc<ColoredRows> {
        let key = (hash, language.to_string(), theme as u8);
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.as_ref().is_none_or(|old| old.key != key) {
            let id = self.latest.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
            *state = Some(State {
                key,
                id,
                rows: Arc::new(Vec::new()),
            });
            *self
                .pending
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = Some(Request {
                id,
                code: code.to_string(),
                language: language.to_string(),
                theme,
                ctx: ctx.clone(),
            });
            let _ = self.sender.try_send(());
        }
        let state = state.as_mut().unwrap();
        for batch in self.receiver.try_iter() {
            if batch.id == state.id && batch.first == state.rows.len() {
                Arc::make_mut(&mut state.rows).extend(batch.rows);
            }
        }
        state.rows.clone()
    }
}

pub(super) fn snapshot(
    ctx: &egui::Context,
    hash: u64,
    code: &str,
    language: &str,
    theme: AppTheme,
) -> Arc<ColoredRows> {
    let id = egui::Id::new("flash-md-code-highlighter");
    let worker = ctx
        .data(|store| store.get_temp::<Arc<Worker>>(id))
        .unwrap_or_else(|| {
            let worker = Arc::new(Worker::new());
            ctx.data_mut(|store| store.insert_temp(id, worker.clone()));
            worker
        });
    worker.snapshot(ctx, hash, code, language, theme)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_colors_keep_multiline_context_and_reject_obsolete_documents() {
        let worker = Worker::new();
        let ctx = egui::Context::default();
        let old = "let obsolete = 1;\n".repeat(2000);
        let code = "/* start\ninside comment\n*/ let value = 1;\n";
        let _ = worker.snapshot(
            &ctx,
            crate::parsers::content_hash(&old),
            &old,
            "rs",
            AppTheme::Light,
        );
        let hash = crate::parsers::content_hash(code);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let rows = loop {
            let rows = worker.snapshot(&ctx, hash, code, "rs", AppTheme::Dark);
            if rows.len() == 3 {
                break rows;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "background syntax did not complete"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        let syntax = find_syntax_by_lang("rs", get_syntax_set());
        let mut reference =
            HighlightLines::new(syntax, &get_theme_set().themes["base16-eighties.dark"]);
        for (index, line) in code.split_inclusive('\n').enumerate() {
            let expected = reference.highlight_line(line, get_syntax_set()).unwrap();
            assert_eq!(rows[index].len(), expected.len());
            for (span, (style, text)) in rows[index].iter().zip(expected) {
                assert_eq!(
                    span.color,
                    Color32::from_rgb(style.foreground.r, style.foreground.g, style.foreground.b)
                );
                assert_eq!(&line[span.range.clone()], text);
            }
        }
    }
}
