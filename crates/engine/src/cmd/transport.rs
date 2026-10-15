//! Transport commands: playback control (play, pause, stop, seek, tempo) for time-based crafts.

use serde_json::{Value, json};

use crate::{CmdResult, CommandSpec, Session, p};

/// Return all transport commands.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("transport.togglePlay", "Play / Pause", "Transport", toggle_play).key("Space"),
        CommandSpec::new("transport.play", "Play", "Transport", |s, _| {
            s.transport.playing = true;
            Ok(json!({"playing": true}))
        }),
        CommandSpec::new("transport.pause", "Pause", "Transport", |s, _| {
            s.transport.playing = false;
            Ok(json!({"playing": false}))
        }),
        CommandSpec::new("transport.stop", "Stop", "Transport", |s, _| {
            s.transport.playing = false;
            s.transport.time_ms = 0;
            Ok(json!({"playing": false, "timeMs": 0}))
        }),
        CommandSpec::new("transport.seek", "Seek", "Transport", seek).params(r#"{"timeMs": n}"#),
        CommandSpec::new("transport.toggleLoop", "Toggle Loop", "Transport", |s, _| {
            s.transport.looping = !s.transport.looping;
            Ok(json!({"looping": s.transport.looping}))
        }),
        CommandSpec::new("transport.setTempo", "Tempo", "Transport", set_tempo).params(r#"{"tempo": n}"#),
    ]
}

fn toggle_play(s: &mut Session, _: &Value) -> CmdResult {
    s.transport.playing = !s.transport.playing;
    Ok(json!({"playing": s.transport.playing}))
}

fn seek(s: &mut Session, v: &Value) -> CmdResult {
    let time = p::u64(v, "timeMs").unwrap_or(0);
    s.transport.time_ms = time;
    Ok(json!({"timeMs": time}))
}

fn set_tempo(s: &mut Session, v: &Value) -> CmdResult {
    let tempo = p::f32(v, "tempo").unwrap_or(120.0).clamp(20.0, 999.0);
    s.transport.tempo = tempo;
    Ok(json!({"tempo": tempo}))
}
