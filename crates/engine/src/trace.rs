//! Performance trace: where frame time and memory go, and where the main thread is when no frame
//! runs, written while the app runs so a slow or memory-hungry session can be read afterwards.
//!
//! Off by default and nearly free when off (one atomic load per span). [`start`] turns it on and
//! writes into a directory:
//!
//! - `<name>.trace.json`: Chrome trace events (a JSON array, readable in Perfetto or
//!   `chrome://tracing`). Every frame is one `frame` event; frames slower than the detail
//!   threshold also carry their nested spans (commands, model sync, routing, UI sections), so the
//!   slow ones show as a flame graph. Stalls, memory counters, notes and marks are events too.
//!   The array is written as it goes and closed by [`finish`]; both viewers accept the open array
//!   a killed app leaves behind.
//! - `<name>.summary.json`: rewritten every few seconds and at exit: the session header (build,
//!   GPU adapter, display), frame time percentiles, the gap between frames split by whether the
//!   previous frame asked for an immediate repaint, per-span count, total, mean, p95 and worst,
//!   the slowest frames with their span trees, the longest stalls with the frame before them and
//!   the input that arrived after, repaint causes, control-channel notes, memory over time and
//!   marks. Small enough to read in one go.
//! - `<name>-stall-<n>.txt` (macOS): the process's native stacks, captured with the system
//!   `sample` tool while the main thread was stalled, so time spent outside the app's own code
//!   (event loop, compositor, platform calls) has a stack to look at.
//!
//! A stall is a gap in which the UI asked to be drawn again and no frame began within
//! [`Options::stall_ms`]; a watchdog thread notices it, and the next frame closes it. A frame that
//! arrives carrying more than [`Options::pileup_events`] pointer moves is recorded as a stall too:
//! the pointer kept moving while no frame ran. Each stall records what woke the frame after it:
//! `immediate` (the UI had asked for a frame at once: a real stall), `external` (a [`note`] was
//! written since the last frame ended: a control-channel request, a native menu event or a
//! Finder open request woke the app, so the gap was somebody else's pause) or `timer` (a delayed
//! repaint request, whose delay egui does not disclose, so the gap may have been a legitimate
//! wait); only `immediate` counts as confirmed.
//!
//! Spans nest per thread. A span opened on the thread that called [`frame_begin`] belongs to the
//! current frame; spans on other threads, or outside a frame, are written as they close.
//! WebAssembly has no clock, threads or files here, so every function is a no-op there.

use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;

static ENABLED: AtomicBool = AtomicBool::new(false);

/// How the trace behaves; [`start`] takes one.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// Frames at least this slow keep their nested spans in the trace file.
    pub detail_ms: f64,
    /// A pending repaint that no frame answers within this long is a stall; `0` turns the
    /// watchdog off.
    pub stall_ms: f64,
    /// On macOS, capture the process's stacks with the system `sample` tool when a stall is
    /// noticed (at most `max_samples` times per run).
    pub sample_stalls: bool,
    /// Most stack captures per run.
    pub max_samples: usize,
    /// A frame with more pointer moves than this was late: the pointer moved several times while
    /// no frame ran.
    pub pileup_events: u64,
}

impl Default for Options {
    fn default() -> Self {
        Options { detail_ms: 8.0, stall_ms: 100.0, sample_stalls: true, max_samples: 10, pileup_events: 4 }
    }
}

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

/// Start of a UI frame on this thread. `immediate_requested_last_pass` says whether the previous
/// frame asked for an immediate repaint (egui's `requested_repaint_last_pass`): the gap before
/// this frame is then expected to be one display refresh, and anything longer is a stall. `None`
/// when the host cannot tell.
pub fn frame_begin(immediate_requested_last_pass: Option<bool>) {
    if enabled() {
        imp::frame_begin(immediate_requested_last_pass);
    }
}

/// End of the UI frame; `args` go on the frame's event. The trace reads `repaintPending` (the UI
/// asked to be drawn again, so the watchdog may call the next gap a stall) and
/// `inputKinds.pointerMoved` (pointer pile-up) from them; `input`, `inputKinds`, `synthetic`,
/// `prevCpuMs` and the like are the host's to add.
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
/// so a loop that never lets the app go idle shows up, and kept with the frame for stall reports.
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

/// Count one occurrence of `name` in the current frame (a viewport command sent, a platform call
/// made); the counts land in the frame event's `ticks`.
pub fn tick(name: &'static str) {
    if enabled() {
        imp::tick(name.to_string());
    }
}

/// [`tick`] with a name built at run time; the closure runs only when tracing is on.
pub fn tick_with(name: impl FnOnce() -> String) {
    if enabled() {
        imp::tick(name());
    }
}

/// A labelled instant on the timeline that is counted per label in the summary (`control:
/// ui.click`), without the flush or the marks-list entry of [`mark`]. For frequent events.
pub fn note(label: &str) {
    if enabled() {
        imp::note(label);
    }
}

/// A labelled moment ("it lagged here"), on the timeline and listed in the summary.
pub fn mark(label: &str) {
    if enabled() {
        imp::mark(label);
    }
}

/// Facts about this run (build, GPU adapter, display, control channel) for the summary's
/// `session` header; an object whose fields merge with those given before.
pub fn session(info: Value) {
    if enabled() {
        imp::session(info);
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
pub fn start(dir: &std::path::Path, options: Options) -> Result<std::path::PathBuf, String> {
    let path = imp::start(dir, &options)?;
    ENABLED.store(true, Ordering::Relaxed);
    imp::start_watchdog(&options);
    Ok(path)
}

/// Write the summary, close the trace file and turn tracing off.
pub fn finish() {
    if enabled() {
        imp::finish();
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
    pub fn frame_begin(_: Option<bool>) {}
    pub fn frame_end(_: Value) {}
    pub fn counter(_: &'static str, _: Value) {}
    pub fn repaint_causes(_: impl IntoIterator<Item = String>) {}
    pub fn mark(_: &str) {}
    pub fn note(_: &str) {}
    pub fn tick(_: String) {}
    pub fn session(_: Value) {}
    pub fn sample_us(_: &'static str, _: u64) {}
    pub fn flush(_: bool) {}
    pub fn finish() {}
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

    use super::Options;

    /// Durations kept per span name for percentiles.
    const RECENT: usize = 2048;
    /// Slowest frames and longest stalls kept with their span trees.
    const SLOWEST: usize = 25;
    /// Most spans one frame keeps (a runaway loop of spans must not eat memory).
    const MAX_FRAME_SPANS: usize = 4096;
    /// Memory samples kept in the summary.
    const SERIES: usize = 600;
    /// Distinct span names, repaint causes, ticks and notes tracked; later ones are folded into
    /// one entry.
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

    /// A gap in which the UI wanted a frame and got none, or a frame that arrived with the
    /// pointer's backlog.
    struct Stall {
        at_s: f64,
        dur_us: u64,
        /// `pending` (watchdog) or `pileup` (pointer moves queued on the next frame).
        kind: &'static str,
        /// What ended the wait, as the frame after the stall reports it: `immediate` (the UI had
        /// asked for a frame at once and it came late: a real stall), `external` (a note since the
        /// last frame: a control-channel request, menu event or open request woke the app, so the
        /// gap was somebody else's pause), `timer` (a delayed repaint request, so the gap may have
        /// been a legitimate wait) or `unknown`.
        wake: &'static str,
        /// Stack capture written for it, if any.
        sample: Option<String>,
        /// Span tree of the frame before the stall.
        tree: Vec<String>,
        /// Repaint causes of the frame before the stall.
        causes: Vec<String>,
        /// Input kinds carried by the frame after the stall.
        next_input: Value,
    }

    impl Stall {
        fn json(&self) -> Value {
            json!({
                "atS": (self.at_s * 10.0).round() / 10.0,
                "ms": ms(self.dur_us),
                "kind": self.kind,
                "wake": self.wake,
                "sample": self.sample,
                "precedingFrame": self.tree,
                "repaintCauses": self.causes,
                "nextInput": self.next_input,
            })
        }
    }

    struct Recorder {
        epoch: Instant,
        started_unix: u64,
        dir: PathBuf,
        stem: String,
        trace_path: PathBuf,
        summary_path: PathBuf,
        out: BufWriter<File>,
        first_event: bool,
        detail_us: u64,
        stall_us: u64,
        sample_stalls: bool,
        max_samples: usize,
        pileup_events: u64,
        frame_tid: Option<u64>,
        frame_start: Option<Instant>,
        frame_spans: Vec<Done>,
        frame_causes: Vec<String>,
        ticks: BTreeMap<String, u64>,
        /// Notes written since the last frame ended (between frames or during this one): something
        /// outside the UI thread's own requests woke the app.
        frame_notes: u64,
        /// The gap before this frame and whether egui had an immediate repaint outstanding when
        /// it began; classified at `frame_end`, once the frame has shown what woke it.
        frame_gap: Option<(u64, Option<bool>)>,
        frames: u64,
        idle_frames: u64,
        last_frame_start: Option<Instant>,
        last_frame_end: Option<Instant>,
        last_frame_dur_us: u64,
        /// The last frame asked to be drawn again (immediately or after a delay).
        pending_after_last: bool,
        last_tree: Vec<String>,
        last_causes: Vec<String>,
        frame_stat: Stat,
        gap_stat: Stat,
        gap_pending_stat: Stat,
        gap_idle_stat: Stat,
        stats: HashMap<String, Stat>,
        slowest: Vec<SlowFrame>,
        /// Noticed by the watchdog, closed by the next frame: when the gap began and the stack
        /// capture requested for it.
        open_stall: Option<(Instant, Option<String>)>,
        /// Closed by `frame_begin`, waiting for that frame's input kinds at `frame_end`.
        stall_awaiting_input: Option<Stall>,
        stalls: Vec<Stall>,
        stall_count: u64,
        unconfirmed_stalls: u64,
        pileups: u64,
        samples_taken: usize,
        repaints: HashMap<String, u64>,
        notes: BTreeMap<String, u64>,
        counters: BTreeMap<&'static str, VecDeque<Value>>,
        marks: Vec<Value>,
        session: serde_json::Map<String, Value>,
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

    fn to_us(ms: f64, max: f64) -> u64 {
        if ms.is_finite() && ms >= 0.0 { (ms * 1000.0).min(max) as u64 } else { 0 }
    }

    pub fn start(dir: &Path, options: &Options) -> Result<PathBuf, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let unix = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let stem = format!("vizcraft-{unix}-{}", std::process::id());
        let trace_path = dir.join(format!("{stem}.trace.json"));
        let summary_path = dir.join(format!("{stem}.summary.json"));
        let file = File::create(&trace_path).map_err(|e| format!("{}: {e}", trace_path.display()))?;
        let mut out = BufWriter::new(file);
        out.write_all(b"[\n").map_err(|e| e.to_string())?;
        let now = Instant::now();
        let rec = Recorder {
            epoch: now,
            started_unix: unix,
            dir: dir.to_path_buf(),
            stem,
            trace_path: trace_path.clone(),
            summary_path,
            out,
            first_event: true,
            detail_us: if options.detail_ms.is_finite() && options.detail_ms >= 0.0 { to_us(options.detail_ms, 60_000_000.0) } else { 8_000 },
            stall_us: to_us(options.stall_ms, 60_000_000.0),
            sample_stalls: options.sample_stalls,
            max_samples: options.max_samples,
            pileup_events: options.pileup_events,
            frame_tid: None,
            frame_start: None,
            frame_spans: Vec::new(),
            frame_causes: Vec::new(),
            ticks: BTreeMap::new(),
            frame_notes: 0,
            frame_gap: None,
            frames: 0,
            idle_frames: 0,
            last_frame_start: None,
            last_frame_end: None,
            last_frame_dur_us: 0,
            pending_after_last: false,
            last_tree: Vec::new(),
            last_causes: Vec::new(),
            frame_stat: Stat::default(),
            gap_stat: Stat::default(),
            gap_pending_stat: Stat::default(),
            gap_idle_stat: Stat::default(),
            stats: HashMap::new(),
            slowest: Vec::new(),
            open_stall: None,
            stall_awaiting_input: None,
            stalls: Vec::new(),
            stall_count: 0,
            unconfirmed_stalls: 0,
            pileups: 0,
            samples_taken: 0,
            repaints: HashMap::new(),
            notes: BTreeMap::new(),
            counters: BTreeMap::new(),
            marks: Vec::new(),
            session: serde_json::Map::new(),
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

    /// The stall watchdog: wakes a few times per stall threshold, asks the recorder whether the
    /// UI has been waiting for a frame too long, and runs the stack capture it hands back. Exits
    /// when tracing is turned off. The capture runs as a child process so the UI thread never
    /// waits for it.
    pub fn start_watchdog(options: &Options) {
        let stall_us = to_us(options.stall_ms, 60_000_000.0);
        if stall_us == 0 {
            return;
        }
        let tick = Duration::from_micros((stall_us / 2).clamp(1_000, 50_000));
        let pid = std::process::id().to_string();
        // A thread that cannot be spawned only means no stall detection; the trace goes on.
        let _ = std::thread::Builder::new().name("vizcraft-trace-watchdog".into()).spawn(move || {
            let mut children: Vec<std::process::Child> = Vec::new();
            while super::enabled() {
                std::thread::sleep(tick);
                children.retain_mut(|c| matches!(c.try_wait(), Ok(None)));
                let Some(Some(path)) = with(|r| r.watch(Instant::now())) else { continue };
                let child = std::process::Command::new("/usr/bin/sample")
                    .args([pid.as_str(), "1", "1", "-mayDie", "-file", path.as_str()])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
                if let Ok(c) = child {
                    children.push(c);
                }
            }
            for mut c in children {
                let _ = c.wait();
            }
        });
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

    pub fn frame_begin(immediate: Option<bool>) {
        let now = Instant::now();
        let tid = TID.with(|t| *t);
        with(|r| {
            r.frame_gap = r.last_frame_start.map(|prev| (us_since(prev, now), immediate));
            if let Some((since, sample)) = r.open_stall.take() {
                let dur_us = us_since(since, now);
                let stall = Stall {
                    at_s: us_since(r.epoch, since) as f64 / 1e6,
                    dur_us,
                    kind: "pending",
                    wake: wake_name(immediate, 0, 0),
                    sample,
                    tree: r.last_tree.clone(),
                    causes: r.last_causes.clone(),
                    next_input: Value::Null,
                };
                r.write_stall(&stall, us_since(r.epoch, since));
                r.stall_awaiting_input = Some(stall);
            }
            r.last_frame_start = Some(now);
            r.frame_start = Some(now);
            r.frame_tid = Some(tid);
            r.frame_spans.clear();
            r.frame_causes.clear();
            r.ticks.clear();
        });
    }

    /// What woke the frame after a gap. egui reports an immediate request when any
    /// `request_repaint` ran since the last pass ended, including one from outside the frame (the
    /// control channel's thread, a menu event); a note since the last frame or synthetic input in
    /// this one tells those apart.
    fn wake_name(immediate: Option<bool>, notes: u64, synthetic: u64) -> &'static str {
        if notes > 0 || synthetic > 0 {
            "external"
        } else {
            match immediate {
                Some(true) => "immediate",
                Some(false) => "timer",
                None => "unknown",
            }
        }
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
                if !r.ticks.is_empty() {
                    o.insert("ticks".into(), json!(r.ticks));
                }
            }
            r.event(&json!({"name": "frame", "cat": "frame", "ph": "X", "ts": start_us, "dur": dur_us, "pid": 1, "tid": tid, "args": fargs}));
            let spans = std::mem::take(&mut r.frame_spans);
            if dur_us >= r.detail_us {
                for s in &spans {
                    r.write_span(s, "span");
                }
            }
            // The tree is kept for every frame: the next stall report needs the frame before it.
            let mut ordered: Vec<&Done> = spans.iter().collect();
            ordered.sort_by_key(|s| (s.start_us, std::cmp::Reverse(s.dur_us)));
            let tree: Vec<String> = ordered
                .iter()
                .filter(|s| s.dur_us >= 100 || s.depth == 0)
                .take(200)
                .map(|s| format!("{}{} {}ms", "  ".repeat(s.depth.min(20)), s.name, ms(s.dur_us)))
                .collect();
            let slow_enough = r.slowest.len() < SLOWEST || r.slowest.last().is_some_and(|f| f.dur_us < dur_us);
            if slow_enough {
                r.slowest.push(SlowFrame { n, at_s: start_us as f64 / 1e6, dur_us, args: args.clone(), tree: tree.clone() });
                r.slowest.sort_by_key(|f| std::cmp::Reverse(f.dur_us));
                r.slowest.truncate(SLOWEST);
            }
            let input_kinds = args.get("inputKinds").cloned().unwrap_or(Value::Null);
            let synthetic = args.get("synthetic").and_then(Value::as_u64).unwrap_or(0);
            if let Some((gap, immediate)) = r.frame_gap.take() {
                r.gap_stat.add(gap);
                if wake_name(immediate, r.frame_notes, synthetic) == "immediate" {
                    r.gap_pending_stat.add(gap);
                    // What eframe, winit, the compositor and the OS took between our frames when
                    // we asked for the next one at once: the present wait the CPU figure hides.
                    let turnaround = gap.saturating_sub(r.last_frame_dur_us);
                    r.stats.entry("eframe.turnaround".to_string()).or_default().add(turnaround);
                } else {
                    r.gap_idle_stat.add(gap);
                }
            }
            if let Some(mut stall) = r.stall_awaiting_input.take() {
                stall.next_input = input_kinds.clone();
                if stall.wake == "immediate" {
                    stall.wake = wake_name(Some(true), r.frame_notes, synthetic);
                }
                r.push_stall(stall);
            }
            let moves = input_kinds.get("pointerMoved").and_then(Value::as_u64).unwrap_or(0);
            if moves > r.pileup_events
                && let Some(prev_end) = r.last_frame_end
            {
                let stall = Stall {
                    at_s: us_since(r.epoch, prev_end) as f64 / 1e6,
                    dur_us: us_since(prev_end, start),
                    kind: "pileup",
                    wake: wake_name(Some(true), r.frame_notes, synthetic),
                    sample: None,
                    tree: r.last_tree.clone(),
                    causes: r.last_causes.clone(),
                    next_input: input_kinds,
                };
                r.write_stall(&stall, us_since(r.epoch, prev_end));
                r.pileups += 1;
                r.push_stall(stall);
            }
            r.last_tree = tree;
            r.last_causes = std::mem::take(&mut r.frame_causes);
            r.last_frame_end = Some(end);
            r.last_frame_dur_us = dur_us;
            r.pending_after_last = args.get("repaintPending").and_then(Value::as_bool).unwrap_or(false);
            r.frame_notes = 0;
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
                if r.frame_causes.len() < 64 {
                    r.frame_causes.push(c.clone());
                }
                let key = if r.repaints.len() < MAX_NAMES || r.repaints.contains_key(&c) { c } else { "(other causes)".into() };
                *r.repaints.entry(key).or_default() += 1;
            }
        });
    }

    pub fn sample_us(name: &'static str, us: u64) {
        with(|r| r.stats.entry(name.to_string()).or_default().add(us));
    }

    pub fn tick(name: String) {
        with(|r| {
            let key = if r.ticks.len() < MAX_NAMES || r.ticks.contains_key(&name) { name } else { "(other ticks)".into() };
            *r.ticks.entry(key).or_default() += 1;
        });
    }

    pub fn note(label: &str) {
        let now = Instant::now();
        let label: String = label.chars().take(200).collect();
        with(|r| {
            let ts = us_since(r.epoch, now);
            r.event(&json!({"name": format!("note: {label}"), "ph": "i", "s": "t", "ts": ts, "pid": 1, "tid": r.frame_tid.unwrap_or(1)}));
            r.frame_notes += 1;
            let key = if r.notes.len() < MAX_NAMES || r.notes.contains_key(&label) { label } else { "(other notes)".into() };
            *r.notes.entry(key).or_default() += 1;
        });
    }

    pub fn mark(label: &str) {
        let now = Instant::now();
        let label: String = label.chars().take(200).collect();
        with(|r| {
            let ts = us_since(r.epoch, now);
            r.event(&json!({"name": format!("mark: {label}"), "ph": "i", "s": "g", "ts": ts, "pid": 1, "tid": r.frame_tid.unwrap_or(1)}));
            r.push_mark(ts, label);
            r.flush_now();
        });
    }

    pub fn session(info: Value) {
        let now = Instant::now();
        with(|r| {
            if let Value::Object(o) = info {
                for (k, v) in o {
                    r.session.insert(k, v);
                }
            }
            let ts = us_since(r.epoch, now);
            let session = Value::Object(r.session.clone());
            r.event(&json!({"name": "session", "ph": "i", "s": "g", "ts": ts, "pid": 1, "tid": 1, "args": session}));
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

    pub fn finish() {
        with(|r| {
            r.write_summary();
            let _ = r.out.write_all(b"\n]\n");
            r.flush_now();
        });
    }

    impl Recorder {
        fn event(&mut self, v: &Value) {
            // Errors writing the trace are dropped: tracing must never disturb the session.
            if !self.first_event {
                let _ = self.out.write_all(b",\n");
            }
            self.first_event = false;
            let _ = serde_json::to_writer(&mut self.out, v);
        }

        fn write_span(&mut self, s: &Done, cat: &str) {
            let v = json!({"name": s.name, "cat": cat, "ph": "X", "ts": s.start_us, "dur": s.dur_us, "pid": 1, "tid": s.tid});
            self.event(&v);
        }

        fn write_stall(&mut self, s: &Stall, ts: u64) {
            let v = json!({"name": format!("stall ({})", s.kind), "cat": "stall", "ph": "X", "ts": ts, "dur": s.dur_us, "pid": 1, "tid": self.frame_tid.unwrap_or(1),
                "args": {"kind": s.kind, "sample": s.sample}});
            self.event(&v);
        }

        fn push_mark(&mut self, ts: u64, label: String) {
            if self.marks.len() < MAX_MARKS {
                self.marks.push(json!({"atS": (ts as f64 / 1e5).round() / 10.0, "label": label}));
            }
        }

        /// Record a closed stall: counts, the longest-stalls list and a mark on the timeline.
        fn push_stall(&mut self, stall: Stall) {
            self.stall_count += 1;
            if stall.wake != "immediate" {
                self.unconfirmed_stalls += 1;
            }
            let label = format!("stall {}ms ({}, {} wake)", ms(stall.dur_us), stall.kind, stall.wake);
            self.push_mark((stall.at_s * 1e6) as u64, label);
            let worth_keeping = self.stalls.len() < SLOWEST || self.stalls.last().is_some_and(|s| s.dur_us < stall.dur_us);
            if worth_keeping {
                self.stalls.push(stall);
                self.stalls.sort_by_key(|s| std::cmp::Reverse(s.dur_us));
                self.stalls.truncate(SLOWEST);
            }
        }

        /// Called by the watchdog: is the UI waiting for a frame that is not coming? Returns the
        /// path for a stack capture when one should be taken now.
        fn watch(&mut self, now: Instant) -> Option<String> {
            if self.frame_start.is_some() || self.open_stall.is_some() || !self.pending_after_last {
                return None;
            }
            let end = self.last_frame_end?;
            if us_since(end, now) < self.stall_us {
                return None;
            }
            let sample = if self.sample_stalls && cfg!(target_os = "macos") && self.samples_taken < self.max_samples {
                self.samples_taken += 1;
                Some(self.dir.join(format!("{}-stall-{}.txt", self.stem, self.samples_taken)).display().to_string())
            } else {
                None
            };
            self.open_stall = Some((end, sample.clone()));
            sample
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
                "stallThresholdMs": ms(self.stall_us),
                "session": self.session,
                "frames": self.frames,
                "framesWithNoInput": self.idle_frames,
                "framesPerSecond": if elapsed > 0.0 { (self.frames as f64 / elapsed * 10.0).round() / 10.0 } else { 0.0 },
                "frameMs": self.frame_stat.json("frame"),
                "frameGapMs": self.gap_stat.json("gap between frame starts"),
                "frameGapPendingMs": self.gap_pending_stat.json("gap after a frame that asked for an immediate repaint (one refresh is normal)"),
                "frameGapIdleMs": self.gap_idle_stat.json("gap after a frame that did not ask for an immediate repaint (idle or timer)"),
                "stalls": {
                    "count": self.stall_count,
                    "unconfirmed": self.unconfirmed_stalls,
                    "pileups": self.pileups,
                    "samples": self.samples_taken,
                },
                "longestStalls": self.stalls.iter().map(Stall::json).collect::<Vec<_>>(),
                "spans": spans.iter().take(120).map(|(n, s)| s.json(n)).collect::<Vec<_>>(),
                "slowestFrames": self.slowest.iter().map(|f| json!({"frame": f.n, "atS": (f.at_s * 10.0).round() / 10.0, "ms": ms(f.dur_us), "args": f.args, "spans": f.tree})).collect::<Vec<_>>(),
                "repaintCauses": repaints.iter().take(30).map(|(c, n)| json!({"cause": c, "frames": n})).collect::<Vec<_>>(),
                "notes": self.notes,
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
        fn nested_spans_stalls_and_the_summary() {
            let dir = std::env::temp_dir().join(format!("vizcraft-trace-test-{}", std::process::id()));
            // A tiny stall threshold so the watchdog notices within the test; no stack captures.
            let options = Options { detail_ms: 0.0, stall_ms: 5.0, sample_stalls: false, max_samples: 0, pileup_events: 4 };
            let path = crate::trace::start(&dir, options).unwrap();
            crate::trace::session(json!({"commit": "abc123", "debugAssertions": true}));
            crate::trace::session(json!({"adapter": "test gpu"}));
            crate::trace::frame_begin(None);
            {
                let _a = crate::trace::span("outer");
                let _b = crate::trace::span_with(|| "inner".to_string());
                crate::trace::tick("viewportCmd.Title");
                crate::trace::tick_with(|| "viewportCmd.Title".to_string());
                // Long enough for the slow-frame tree, which leaves out spans under 0.1 ms.
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            crate::trace::counter("memory", json!({"rssMb": 12.5}));
            crate::trace::repaint_causes(["lib.rs:10 animation".to_string()]);
            crate::trace::mark("lagged here");
            crate::trace::note("control: ui.click");
            crate::trace::note("control: ui.click");
            // The frame asks to be drawn again at once, then nothing comes for a while: a stall.
            crate::trace::frame_end(json!({"input": 0, "repaintPending": true, "inputKinds": {}}));
            std::thread::sleep(std::time::Duration::from_millis(40));
            crate::trace::frame_begin(Some(true));
            // Nine pointer moves in one frame: the pointer kept moving while no frame ran.
            crate::trace::frame_end(json!({"input": 9, "repaintPending": false, "inputKinds": {"pointerMoved": 9}}));
            crate::trace::finish();
            assert!(!crate::trace::enabled());
            let trace_text = std::fs::read_to_string(&path).unwrap();
            assert!(
                trace_text.starts_with("[\n")
                    && trace_text.contains("\"outer\"")
                    && trace_text.contains("\"inner\"")
                    && trace_text.contains("mark: lagged here")
            );
            let trace: Value = serde_json::from_str(&trace_text).expect("a finished trace is valid JSON");
            let events = trace.as_array().unwrap();
            let frame = events.iter().find(|e| e["cat"] == "frame").unwrap();
            assert_eq!(frame["args"]["ticks"]["viewportCmd.Title"], 2);
            assert_eq!(events.iter().filter(|e| e["cat"] == "stall").count(), 2);
            assert!(events.iter().any(|e| e["name"] == "session" && e["args"]["adapter"] == "test gpu"));
            let summary_path = path.to_string_lossy().replace(".trace.json", ".summary.json");
            let summary: Value = serde_json::from_str(&std::fs::read_to_string(summary_path).unwrap()).unwrap();
            assert_eq!(summary["frames"], 2);
            assert_eq!(summary["framesWithNoInput"], 1);
            assert_eq!(summary["session"]["commit"], "abc123");
            assert_eq!(summary["session"]["adapter"], "test gpu");
            assert_eq!(summary["frameGapPendingMs"]["count"], 1);
            assert_eq!(summary["frameGapIdleMs"]["count"], 0);
            assert_eq!(summary["notes"]["control: ui.click"], 2);
            let names: Vec<&str> = summary["spans"].as_array().unwrap().iter().filter_map(|s| s["name"].as_str()).collect();
            assert!(names.contains(&"outer") && names.contains(&"inner") && names.contains(&"eframe.turnaround"), "{names:?}");
            let tree = summary["slowestFrames"][0]["spans"].as_array().unwrap();
            assert!(tree.iter().any(|l| l.as_str().is_some_and(|l| l.starts_with("  inner"))), "{tree:?}");
            assert_eq!(summary["stalls"]["count"], 2);
            assert_eq!(summary["stalls"]["pileups"], 1);
            assert_eq!(summary["stalls"]["samples"], 0);
            let stalls = summary["longestStalls"].as_array().unwrap();
            assert_eq!(stalls.len(), 2);
            let pending = stalls.iter().find(|s| s["kind"] == "pending").unwrap();
            assert_eq!(pending["wake"], "immediate");
            assert!(pending["ms"].as_f64().unwrap() >= 5.0, "{pending}");
            assert!(pending["precedingFrame"].as_array().unwrap().iter().any(|l| l.as_str().is_some_and(|l| l.starts_with("outer"))), "{pending}");
            assert_eq!(pending["repaintCauses"][0], "lib.rs:10 animation");
            assert_eq!(pending["nextInput"]["pointerMoved"], 9);
            let pileup = stalls.iter().find(|s| s["kind"] == "pileup").unwrap();
            assert_eq!(pileup["nextInput"]["pointerMoved"], 9);
            assert_eq!(pileup["wake"], "immediate");
            assert!(summary["marks"].as_array().unwrap().iter().any(|m| m["label"] == "lagged here"));
            assert!(summary["marks"].as_array().unwrap().iter().any(|m| m["label"].as_str().is_some_and(|l| l.starts_with("stall "))));
            assert_eq!(summary["latest"]["memory"]["rssMb"], 12.5);
            assert_eq!(summary["repaintCauses"][0]["frames"], 1);
            // A frame woken from outside (a control-channel request noted on its thread before the
            // frame): its gap is the script's pause, not a stall the platform caused.
            let options = Options { detail_ms: 0.0, stall_ms: 5.0, sample_stalls: false, max_samples: 0, pileup_events: 4 };
            let dir2 = dir.join("second");
            let path = crate::trace::start(&dir2, options).unwrap();
            crate::trace::frame_begin(None);
            crate::trace::frame_end(json!({"input": 0, "repaintPending": true, "inputKinds": {}}));
            std::thread::sleep(std::time::Duration::from_millis(40));
            crate::trace::note("control: ui.click");
            crate::trace::frame_begin(Some(true));
            crate::trace::frame_end(json!({"input": 2, "synthetic": 0, "repaintPending": false, "inputKinds": {"pointerMoved": 2}}));
            crate::trace::finish();
            let summary_path = path.to_string_lossy().replace(".trace.json", ".summary.json");
            let summary: Value = serde_json::from_str(&std::fs::read_to_string(summary_path).unwrap()).unwrap();
            assert_eq!(summary["longestStalls"][0]["wake"], "external", "{}", summary["longestStalls"]);
            assert_eq!(summary["stalls"]["unconfirmed"], 1);
            assert_eq!(summary["frameGapPendingMs"]["count"], 0);
            assert_eq!(summary["frameGapIdleMs"]["count"], 1);
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
