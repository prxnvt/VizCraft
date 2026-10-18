//! Performance trace: where frame time and memory go, written while the app runs so a slow or
//! memory-hungry session can be read afterwards.
//!
//! Off by default and nearly free when off (one atomic load per span). [`start`] turns it on and
//! writes two files into a directory:
//!
//! - `<name>.trace.json`: Chrome trace events (a JSON array, readable in Perfetto or
//!   `chrome://tracing`). Every frame is one `frame` event; frames slower than the detail
//!   threshold also carry their nested spans (commands, model sync, routing, UI sections), so the
//!   slow ones show as a flame graph. Memory counters and marks are events too. The array is
//!   written as it goes and left open, which both viewers accept, so a killed app still leaves a
//!   readable file.
//! - `<name>.summary.json`: rewritten every few seconds and at exit: frame time percentiles, the
//!   gap between frames, per-span count, total, mean, p95 and worst, the slowest frames with their
//!   span trees, repaint causes, memory over time and marks. Small enough to read in one go.
//!
//! Spans nest per thread. A span opened on the thread that called [`frame_begin`] belongs to the
//! current frame; spans on other threads, or outside a frame, are written as they close.
//! WebAssembly has no clock or files here, so every function is a no-op there.

use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Whether tracing is on.
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// An open span; it closes and records its duration when dropped.
#[must_use = "a span measures until it is dropped"]
pub struct Span {
    open: bool,
}

impl Drop for Span {
    fn drop(&mut self) {
        if self.open {
            imp::close();
        }
    }
}

/// Open a span with a fixed name.
pub fn span(name: &'static str) -> Span {
    span_cow(Cow::Borrowed(name))
}

/// Open a span whose name is built at run time (a command id). The closure runs only when
/// tracing is on, so the name costs nothing otherwise.
pub fn span_with(name: impl FnOnce() -> String) -> Span {
    if !enabled() {
        return Span { open: false };
    }
    span_cow(Cow::Owned(name()))
}

fn span_cow(name: Cow<'static, str>) -> Span {
    if !enabled() {
        return Span { open: false };
    }
    Span { open: imp::open(name) }
}

/// Start of a UI frame on this thread.
pub fn frame_begin() {
    if enabled() {
        imp::frame_begin();
    }
}

/// End of the UI frame; `args` (input event count, previous frame's CPU time and the like) go on
/// the frame's event.
pub fn frame_end(args: Value) {
    if enabled() {
        imp::frame_end(args);
    }
}

/// A set of counter values at this moment (memory, document size), drawn as graphs in the
/// viewer and kept as a time series in the summary.
pub fn counter(name: &'static str, values: Value) {
    if enabled() {
        imp::counter(name, values);
    }
}

/// Reasons the UI asked to be drawn again this frame (`file:line reason`), counted in the summary
/// so a loop that never lets the app go idle shows up.
pub fn repaint_causes(causes: impl IntoIterator<Item = String>) {
    if enabled() {
        imp::repaint_causes(causes);
    }
}

/// A duration measured elsewhere (eframe's CPU time for a frame), kept with the span statistics
/// under `name`.
pub fn sample_ms(name: &'static str, ms: f64) {
    if enabled() && ms.is_finite() && ms >= 0.0 {
        imp::sample_us(name, (ms * 1000.0).min(3.6e9) as u64);
    }
}

/// A labelled moment ("it lagged here"), on the timeline and listed in the summary.
pub fn mark(label: &str) {
    if enabled() {
        imp::mark(label);
    }
}

/// Write the summary and flush the trace now (also done every few seconds while running).
pub fn flush() {
    if enabled() {
        imp::flush(true);
    }
}

/// Turn tracing on, writing into `dir` (created if missing). Returns the trace file's path.
#[cfg(not(target_arch = "wasm32"))]
pub fn start(dir: &std::path::Path, detail_ms: f64) -> Result<std::path::PathBuf, String> {
    let path = imp::start(dir, detail_ms)?;
    ENABLED.store(true, Ordering::Relaxed);
    Ok(path)
}

/// Write the summary, close the trace and turn tracing off.
pub fn finish() {
    if enabled() {
        imp::flush(true);
        ENABLED.store(false, Ordering::Relaxed);
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::borrow::Cow;

    use serde_json::Value;

    pub fn open(_: Cow<'static, str>) -> bool {
        false
    }
    pub fn close() {}
    pub fn frame_begin() {}
    pub fn frame_end(_: Value) {}
    pub fn counter(_: &'static str, _: Value) {}
    pub fn repaint_causes(_: impl IntoIterator<Item = String>) {}
    pub fn mark(_: &str) {}
    pub fn sample_us(_: &'static str, _: u64) {}
    pub fn flush(_: bool) {}
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::borrow::Cow;
    use std::cell::RefCell;
    use std::collections::{BTreeMap, HashMap, VecDeque};
    use std::fs::File;
    use std::io::{BufWriter, Write};
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use serde_json::{Value, json};

    /// Durations kept per span name for percentiles.
    const RECENT: usize = 2048;
    /// Slowest frames kept with their span trees.
    const SLOWEST: usize = 25;
    /// Most spans one frame keeps (a runaway loop of spans must not eat memory).
    const MAX_FRAME_SPANS: usize = 4096;
    /// Memory samples kept in the summary.
    const SERIES: usize = 600;
    /// Distinct span names and repaint causes tracked; later ones are folded into one entry.
    const MAX_NAMES: usize = 2048;
    /// Marks kept in the summary.
    const MAX_MARKS: usize = 200;
    const SUMMARY_EVERY: Duration = Duration::from_secs(5);
    const FLUSH_EVERY: Duration = Duration::from_secs(1);

    static REC: Mutex<Option<Recorder>> = Mutex::new(None);
    static NEXT_TID: AtomicU64 = AtomicU64::new(1);

    thread_local! {
        static STACK: RefCell<Vec<(Cow<'static, str>, Instant)>> = const { RefCell::new(Vec::new()) };
        static TID: u64 = NEXT_TID.fetch_add(1, Ordering::Relaxed);
    }

    struct Done {
        name: Cow<'static, str>,
        start_us: u64,
        dur_us: u64,
        depth: usize,
        tid: u64,
    }

    #[derive(Default)]
    struct Stat {
        count: u64,
        total_us: u64,
        max_us: u64,
        recent: VecDeque<u64>,
    }

    impl Stat {
        fn add(&mut self, us: u64) {
            self.count += 1;
            self.total_us = self.total_us.saturating_add(us);
            self.max_us = self.max_us.max(us);
            if self.recent.len() >= RECENT {
                self.recent.pop_front();
            }
            self.recent.push_back(us);
        }

        fn json(&self, name: &str) -> Value {
            let mut v: Vec<u64> = self.recent.iter().copied().collect();
            v.sort_unstable();
            let pct = |p: usize| v.get((v.len().saturating_sub(1)) * p / 100).copied().unwrap_or(0);
            json!({
                "name": name,
                "count": self.count,
                "totalMs": ms(self.total_us),
                "meanMs": ms(self.total_us.checked_div(self.count).unwrap_or(0)),
                "p50Ms": ms(pct(50)),
                "p95Ms": ms(pct(95)),
                "maxMs": ms(self.max_us),
            })
        }
    }

    struct SlowFrame {
        n: u64,
        at_s: f64,
        dur_us: u64,
        args: Value,
        tree: Vec<String>,
    }

    struct Recorder {
        epoch: Instant,
        started_unix: u64,
        trace_path: PathBuf,
        summary_path: PathBuf,
        out: BufWriter<File>,
        detail_us: u64,
        frame_tid: Option<u64>,
        frame_start: Option<Instant>,
        frame_spans: Vec<Done>,
        frames: u64,
        idle_frames: u64,
        last_frame_start: Option<Instant>,
        frame_stat: Stat,
        gap_stat: Stat,
        stats: HashMap<String, Stat>,
        slowest: Vec<SlowFrame>,
        repaints: HashMap<String, u64>,
        counters: BTreeMap<&'static str, VecDeque<Value>>,
        marks: Vec<Value>,
        last_flush: Instant,
        last_summary: Instant,
    }

    fn ms(us: u64) -> f64 {
        (us as f64 / 10.0).round() / 100.0
    }

    fn us_since(epoch: Instant, t: Instant) -> u64 {
        u64::try_from(t.saturating_duration_since(epoch).as_micros()).unwrap_or(u64::MAX)
    }

    fn with<R>(f: impl FnOnce(&mut Recorder) -> R) -> Option<R> {
        let mut g = REC.lock().unwrap_or_else(|e| e.into_inner());
        g.as_mut().map(f)
    }

    pub fn start(dir: &Path, detail_ms: f64) -> Result<PathBuf, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let unix = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let stem = format!("vizcraft-{unix}-{}", std::process::id());
        let trace_path = dir.join(format!("{stem}.trace.json"));
        let summary_path = dir.join(format!("{stem}.summary.json"));
        let file = File::create(&trace_path).map_err(|e| format!("{}: {e}", trace_path.display()))?;
        let mut out = BufWriter::new(file);
        out.write_all(b"[\n").map_err(|e| e.to_string())?;
        let now = Instant::now();
        let detail_us = if detail_ms.is_finite() && detail_ms >= 0.0 { (detail_ms * 1000.0).min(60_000_000.0) as u64 } else { 8_000 };
        let rec = Recorder {
            epoch: now,
            started_unix: unix,
            trace_path: trace_path.clone(),
            summary_path,
            out,
            detail_us,
            frame_tid: None,
            frame_start: None,
            frame_spans: Vec::new(),
            frames: 0,
            idle_frames: 0,
            last_frame_start: None,
            frame_stat: Stat::default(),
            gap_stat: Stat::default(),
            stats: HashMap::new(),
            slowest: Vec::new(),
            repaints: HashMap::new(),
            counters: BTreeMap::new(),
            marks: Vec::new(),
            last_flush: now,
            last_summary: now,
        };
        let mut g = REC.lock().unwrap_or_else(|e| e.into_inner());
        *g = Some(rec);
        drop(g);
        with(|r| {
            r.event(&json!({"name": "process_name", "ph": "M", "pid": 1, "args": {"name": "VizCraft"}}));
        });
        Ok(trace_path)
    }

    pub fn open(name: Cow<'static, str>) -> bool {
        STACK.with(|s| s.borrow_mut().push((name, Instant::now())));
        true
    }

    pub fn close() {
        let end = Instant::now();
        let Some((name, start, depth)) = STACK.with(|s| {
            let mut s = s.borrow_mut();
            let (name, start) = s.pop()?;
            Some((name, start, s.len()))
        }) else {
            return;
        };
        let tid = TID.with(|t| *t);
        with(|r| {
            let dur_us = us_since(start, end);
            let key = if r.stats.len() < MAX_NAMES || r.stats.contains_key(name.as_ref()) { name.to_string() } else { "(other spans)".to_string() };
            r.stats.entry(key).or_default().add(dur_us);
            let done = Done { start_us: us_since(r.epoch, start), name, dur_us, depth, tid };
            if r.frame_start.is_some() && r.frame_tid == Some(tid) {
                if r.frame_spans.len() < MAX_FRAME_SPANS {
                    r.frame_spans.push(done);
                }
            } else if dur_us >= r.detail_us {
                r.write_span(&done, "span");
            }
        });
    }

    pub fn frame_begin() {
        let now = Instant::now();
        let tid = TID.with(|t| *t);
        with(|r| {
            if let Some(prev) = r.last_frame_start {
                r.gap_stat.add(us_since(prev, now));
            }
            r.last_frame_start = Some(now);
            r.frame_start = Some(now);
            r.frame_tid = Some(tid);
            r.frame_spans.clear();
        });
    }

    pub fn frame_end(args: Value) {
        let end = Instant::now();
        with(|r| {
            let Some(start) = r.frame_start.take() else { return };
            let dur_us = us_since(start, end);
            r.frames += 1;
            if args.get("input").and_then(Value::as_u64) == Some(0) {
                r.idle_frames += 1;
            }
            r.frame_stat.add(dur_us);
            let start_us = us_since(r.epoch, start);
            let n = r.frames;
            let tid = r.frame_tid.unwrap_or(1);
            let mut fargs = args.clone();
            if let Some(o) = fargs.as_object_mut() {
                o.insert("frame".into(), json!(n));
            }
            r.event(&json!({"name": "frame", "cat": "frame", "ph": "X", "ts": start_us, "dur": dur_us, "pid": 1, "tid": tid, "args": fargs}));
            let spans = std::mem::take(&mut r.frame_spans);
            if dur_us >= r.detail_us {
                for s in &spans {
                    r.write_span(s, "span");
                }
            }
            let slow_enough = r.slowest.len() < SLOWEST || r.slowest.last().is_some_and(|f| f.dur_us < dur_us);
            if slow_enough {
                let mut ordered: Vec<&Done> = spans.iter().collect();
                ordered.sort_by_key(|s| (s.start_us, std::cmp::Reverse(s.dur_us)));
                let tree = ordered
                    .iter()
                    .filter(|s| s.dur_us >= 100 || s.depth == 0)
                    .take(200)
                    .map(|s| format!("{}{} {}ms", "  ".repeat(s.depth.min(20)), s.name, ms(s.dur_us)))
                    .collect();
                r.slowest.push(SlowFrame { n, at_s: start_us as f64 / 1e6, dur_us, args, tree });
                r.slowest.sort_by_key(|f| std::cmp::Reverse(f.dur_us));
                r.slowest.truncate(SLOWEST);
            }
            r.frame_spans = spans;
            r.frame_spans.clear();
            r.maybe_flush(end);
        });
    }

    pub fn counter(name: &'static str, values: Value) {
        let now = Instant::now();
        with(|r| {
            let ts = us_since(r.epoch, now);
            r.event(&json!({"name": name, "ph": "C", "ts": ts, "pid": 1, "args": values}));
            let series = r.counters.entry(name).or_default();
            if series.len() >= SERIES {
                series.pop_front();
            }
            let mut v = values;
            if let Some(o) = v.as_object_mut() {
                o.insert("t".into(), json!((ts as f64 / 1e5).round() / 10.0));
            }
            series.push_back(v);
            r.maybe_flush(now);
        });
    }

    pub fn repaint_causes(causes: impl IntoIterator<Item = String>) {
        with(|r| {
            for c in causes {
                let key = if r.repaints.len() < MAX_NAMES || r.repaints.contains_key(&c) { c } else { "(other causes)".into() };
                *r.repaints.entry(key).or_default() += 1;
            }
        });
    }

    pub fn sample_us(name: &'static str, us: u64) {
        with(|r| r.stats.entry(name.to_string()).or_default().add(us));
    }

    pub fn mark(label: &str) {
        let now = Instant::now();
        let label: String = label.chars().take(200).collect();
        with(|r| {
            let ts = us_since(r.epoch, now);
            r.event(&json!({"name": format!("mark: {label}"), "ph": "i", "s": "g", "ts": ts, "pid": 1, "tid": r.frame_tid.unwrap_or(1)}));
            if r.marks.len() < MAX_MARKS {
                r.marks.push(json!({"atS": (ts as f64 / 1e5).round() / 10.0, "label": label}));
            }
            r.flush_now();
        });
    }

    pub fn flush(summary: bool) {
        with(|r| {
            r.flush_now();
            if summary {
                r.write_summary();
            }
        });
    }

    impl Recorder {
        fn event(&mut self, v: &Value) {
            // Errors writing the trace are dropped: tracing must never disturb the session.
            let _ = serde_json::to_writer(&mut self.out, v);
            let _ = self.out.write_all(b",\n");
        }

        fn write_span(&mut self, s: &Done, cat: &str) {
            let v = json!({"name": s.name, "cat": cat, "ph": "X", "ts": s.start_us, "dur": s.dur_us, "pid": 1, "tid": s.tid});
            self.event(&v);
        }

        fn maybe_flush(&mut self, now: Instant) {
            if now.saturating_duration_since(self.last_flush) >= FLUSH_EVERY {
                self.flush_now();
            }
            if now.saturating_duration_since(self.last_summary) >= SUMMARY_EVERY {
                self.write_summary();
            }
        }

        fn flush_now(&mut self) {
            let _ = self.out.flush();
            self.last_flush = Instant::now();
        }

        fn write_summary(&mut self) {
            self.last_summary = Instant::now();
            let mut spans: Vec<(&String, &Stat)> = self.stats.iter().collect();
            spans.sort_by_key(|(_, s)| std::cmp::Reverse(s.total_us));
            let mut repaints: Vec<(&String, &u64)> = self.repaints.iter().collect();
            repaints.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
            let elapsed = self.epoch.elapsed().as_secs_f64();
            let latest: BTreeMap<&str, Value> = self.counters.iter().filter_map(|(k, v)| Some((*k, v.back()?.clone()))).collect();
            let summary = json!({
                "trace": self.trace_path.display().to_string(),
                "startedUnix": self.started_unix,
                "elapsedS": (elapsed * 10.0).round() / 10.0,
                "detailThresholdMs": ms(self.detail_us),
                "frames": self.frames,
                "framesWithNoInput": self.idle_frames,
                "framesPerSecond": if elapsed > 0.0 { (self.frames as f64 / elapsed * 10.0).round() / 10.0 } else { 0.0 },
                "frameMs": self.frame_stat.json("frame"),
                "frameGapMs": self.gap_stat.json("gap between frame starts"),
                "spans": spans.iter().take(120).map(|(n, s)| s.json(n)).collect::<Vec<_>>(),
                "slowestFrames": self.slowest.iter().map(|f| json!({"frame": f.n, "atS": (f.at_s * 10.0).round() / 10.0, "ms": ms(f.dur_us), "args": f.args, "spans": f.tree})).collect::<Vec<_>>(),
                "repaintCauses": repaints.iter().take(30).map(|(c, n)| json!({"cause": c, "frames": n})).collect::<Vec<_>>(),
                "latest": latest,
                "series": self.counters,
                "marks": self.marks,
            });
            let tmp = self.summary_path.with_extension("json.tmp");
            let written = File::create(&tmp).and_then(|f| {
                let mut w = BufWriter::new(f);
                serde_json::to_writer_pretty(&mut w, &summary)?;
                w.flush()
            });
            if written.is_ok() {
                let _ = std::fs::rename(&tmp, &self.summary_path);
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn nested_spans_land_in_the_frame_and_the_summary() {
            let dir = std::env::temp_dir().join(format!("vizcraft-trace-test-{}", std::process::id()));
            let path = crate::trace::start(&dir, 0.0).unwrap();
            crate::trace::frame_begin();
            {
                let _a = crate::trace::span("outer");
                let _b = crate::trace::span_with(|| "inner".to_string());
                // Long enough for the slow-frame tree, which leaves out spans under 0.1 ms.
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            crate::trace::counter("memory", json!({"rssMb": 12.5}));
            crate::trace::repaint_causes(["lib.rs:10 animation".to_string()]);
            crate::trace::mark("lagged here");
            crate::trace::frame_end(json!({"input": 0}));
            crate::trace::finish();
            let trace = std::fs::read_to_string(&path).unwrap();
            assert!(trace.starts_with("[\n") && trace.contains("\"outer\"") && trace.contains("\"inner\"") && trace.contains("mark: lagged here"));
            let summary_path = path.to_string_lossy().replace(".trace.json", ".summary.json");
            let summary: Value = serde_json::from_str(&std::fs::read_to_string(summary_path).unwrap()).unwrap();
            assert_eq!(summary["frames"], 1);
            assert_eq!(summary["framesWithNoInput"], 1);
            let names: Vec<&str> = summary["spans"].as_array().unwrap().iter().filter_map(|s| s["name"].as_str()).collect();
            assert!(names.contains(&"outer") && names.contains(&"inner"), "{names:?}");
            let tree = summary["slowestFrames"][0]["spans"].as_array().unwrap();
            assert!(tree.iter().any(|l| l.as_str().is_some_and(|l| l.starts_with("  inner"))), "{tree:?}");
            assert_eq!(summary["latest"]["memory"]["rssMb"], 12.5);
            assert_eq!(summary["repaintCauses"][0]["frames"], 1);
            assert_eq!(summary["marks"][0]["label"], "lagged here");
            assert!(!crate::trace::enabled());
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
