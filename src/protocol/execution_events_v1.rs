//! Decode the CLI's bounded, source-bound NDJSON stream. The browser only
//! receives projections selected by this decoder.

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read};

const SCHEMA: &str = "marqdo.execution-events";
const MAX_LINE: usize = 4 * 1024 * 1024;
const MAX_CHUNK: usize = 64 * 1024;
const MAX_TRANSPORT: usize = 16 * 1024 * 1024;

pub struct Binding {
    pub digest: String,
    pub root: String,
    pub entry: String,
    pub function: String,
    pub mode: String,
    pub producer: String,
    pub modules: Vec<String>,
    pub max_output: usize,
}

#[derive(Default)]
pub struct Stream {
    pub id: String,
    pub verified: bool,
    pub truncated: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub terminal: Option<Value>,
    pub invalid: bool,
}

#[derive(Deserialize)]
struct Event {
    schema: String,
    schema_version: u32,
    run_id: String,
    seq: u64,
    kind: String,
    payload: serde_json::Map<String, Value>,
}

pub fn read(input: impl Read, binding: &Binding, mut publish: impl FnMut(Value)) -> Stream {
    let mut input = BufReader::new(input);
    let mut stream = Stream::default();
    let mut line = Vec::new();
    let mut total = 0;
    let mut seq = 0;
    let mut entry_frame_seen = false;
    loop {
        line.clear();
        let size = match input
            .by_ref()
            .take(MAX_LINE as u64 + 1)
            .read_until(b'\n', &mut line)
        {
            Ok(0) => break,
            Ok(size) => size,
            Err(_) => {
                stream.invalid = true;
                break;
            }
        };
        total += size;
        if total > MAX_TRANSPORT || size > MAX_LINE {
            stream.invalid = true;
            break;
        }
        if line.last() != Some(&b'\n') {
            stream.invalid = true;
            break;
        }
        let Ok(event) = serde_json::from_slice::<Event>(&line) else {
            stream.invalid = true;
            break;
        };
        match consume(&mut stream, binding, event, seq, &mut entry_frame_seen) {
            Ok(Some(projection)) => publish(projection),
            Ok(None) => {}
            Err(()) => {
                stream.invalid = true;
                break;
            }
        }
        seq += 1;
    }
    if stream.invalid {
        // Drain without keeping bytes so a malformed child cannot block on its pipe.
        let _ = std::io::copy(&mut input, &mut std::io::sink());
        stream.terminal = None;
    }
    stream
}

fn consume(
    stream: &mut Stream,
    binding: &Binding,
    event: Event,
    seq: u64,
    entry_frame_seen: &mut bool,
) -> Result<Option<Value>, ()> {
    if event.schema != SCHEMA
        || event.schema_version != 1
        || event.seq != seq
        || event.run_id.is_empty()
        || event.run_id.len() > 128
        || stream.terminal.is_some()
    {
        return Err(());
    }
    let payload = &event.payload;
    let kind = event.kind.as_str();
    if seq == 0 {
        if kind != "stream.opened"
            || payload.get("protocol") != Some(&json!({"name": SCHEMA, "schema_version": 1}))
            || payload
                .get("producer")
                .and_then(|value| value["name"].as_str())
                != Some("marqdo")
            || payload
                .get("producer")
                .and_then(|value| value["version"].as_str())
                != Some(&binding.producer)
            || !payload.get("limits").is_some_and(Value::is_object)
            || !payload.get("observation").is_some_and(Value::is_object)
        {
            return Err(());
        }
        stream.id = event.run_id;
        return Ok(Some(json!({"seq": seq, "kind": kind})));
    }
    if stream.id != event.run_id {
        return Err(());
    }
    let mut projection = json!({"seq": seq, "kind": kind});
    if seq == 1 {
        match kind {
            "run.rejected" => {
                if text(payload, "status") != Some("rejected") || text(payload, "phase").is_none() {
                    return Err(());
                }
                stream.terminal = Some(failure(payload)?);
                return Ok(None);
            }
            "run.started" => {
                let source = payload.get("source").ok_or(())?;
                if source["analysis_digest"].as_str() != Some(&binding.digest)
                    || source["root_id"].as_str() != Some(&binding.root)
                    || text(payload, "entry") != Some(&binding.entry)
                    || text(payload, "mode") != Some(&binding.mode)
                {
                    return Err(());
                }
                stream.verified = true;
                return Ok(Some(projection));
            }
            _ => return Err(()),
        }
    }
    match kind {
        "stream.opened" | "run.started" | "run.rejected" => return Err(()),
        "stdout.chunk" | "stderr.chunk" => {
            let data = text(payload, "data_base64").ok_or(())?;
            if text(payload, "encoding") != Some("base64") || data.len() > MAX_CHUNK.div_ceil(3) * 4
            {
                return Err(());
            }
            let bytes = STANDARD.decode(data).map_err(|_| ())?;
            if bytes.len() > MAX_CHUNK
                || payload.get("byte_count").and_then(Value::as_u64) != Some(bytes.len() as u64)
            {
                return Err(());
            }
            if stream.stdout.len() + stream.stderr.len() + bytes.len() > binding.max_output {
                return Err(());
            }
            if kind == "stdout.chunk" {
                stream.stdout.extend_from_slice(&bytes);
            } else {
                stream.stderr.extend_from_slice(&bytes);
            }
            projection["data_base64"] = json!(data);
        }
        "run.completed" => {
            let value = payload
                .get("value")
                .filter(|value| super::value_wire_v1::valid(value))
                .ok_or(())?;
            if text(payload, "status") != Some("ok") || (!*entry_frame_seen && !stream.truncated) {
                return Err(());
            }
            stream.terminal = Some(json!({"success": true, "value": value}));
            return Ok(None);
        }
        "run.failed" => {
            if text(payload, "status") != Some("error") {
                return Err(());
            }
            stream.terminal = Some(failure(payload)?);
            return Ok(None);
        }
        "observation.truncated" => {
            if stream.truncated
                || !matches!(
                    text(payload, "reason"),
                    Some("event_budget" | "byte_budget")
                )
                || payload.get("limit").and_then(Value::as_u64).is_none()
                || payload.get("written").and_then(Value::as_u64).is_none()
                || !payload
                    .get("dropped")
                    .is_some_and(|value| value.is_null() || value.is_u64())
            {
                return Err(());
            }
            stream.truncated = true;
        }
        "call.enter" | "call.exit" | "return" | "branch.eval" | "loop.iteration"
        | "control.break" | "control.continue" | "effect" => {
            let module = text(payload, "module_id").ok_or(())?;
            let span = payload.get("span").ok_or(())?;
            let span_ok = span["start"]
                .as_u64()
                .zip(span["end"].as_u64())
                .is_some_and(|(start, end)| start < end);
            if !binding.modules.iter().any(|known| known == module)
                || !(span_ok
                    || span.is_null() && !matches!(kind, "control.break" | "control.continue"))
                || kind != "effect" && text(payload, "frame_id").is_none()
            {
                return Err(());
            }
            match kind {
                "call.enter" => {
                    let function = text(payload, "function_id").ok_or(())?;
                    match payload.get("parent_frame_id") {
                        Some(Value::Null) if !*entry_frame_seen && function == binding.function => {
                            *entry_frame_seen = true
                        }
                        Some(Value::String(_)) if *entry_frame_seen => {}
                        _ => return Err(()),
                    }
                }
                "call.exit" if text(payload, "status").is_none() => return Err(()),
                "return" if text(payload, "value_type").is_none() => return Err(()),
                "branch.eval" => {
                    projection["decision"] =
                        json!(payload.get("result").and_then(Value::as_bool).ok_or(())?)
                }
                "loop.iteration" => {
                    let decision = text(payload, "decision")
                        .filter(|decision| matches!(*decision, "continue" | "exit"))
                        .ok_or(())?;
                    projection["decision"] = json!(decision);
                }
                "effect"
                    if text(payload, "operation").is_none()
                        || text(payload, "result_code").is_none() =>
                {
                    return Err(())
                }
                _ => {}
            }
        }
        // Values and additive kinds are never emitted as browser observations.
        _ => return Ok(None),
    }
    Ok(Some(projection))
}

fn text<'a>(payload: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a str> {
    payload
        .get(key)?
        .as_str()
        .filter(|text| !text.is_empty() && text.len() <= MAX_LINE)
}

fn failure(payload: &serde_json::Map<String, Value>) -> Result<Value, ()> {
    let error = payload.get("error").ok_or(())?;
    let code = error["code"].as_str().ok_or(())?;
    let message = error["message"].as_str().ok_or(())?;
    Ok(json!({"success": false, "error": {"code": code, "message": message}}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (Binding, Vec<Value>) {
        let binding = Binding {
            digest: "sha256:expected".into(),
            root: "root:p".into(),
            entry: "root:p::index.mq.md::main".into(),
            function: "root:p::index.mq.md::main".into(),
            mode: "pure".into(),
            producer: "3.0.0-rc.2".into(),
            modules: vec!["root:p::index.mq.md".into()],
            max_output: 100,
        };
        let event = |seq, kind, payload| {
            json!({"schema": SCHEMA, "schema_version": 1, "run_id": "r", "seq": seq,
                "kind": kind, "payload": payload})
        };
        let events = vec![
            event(
                0,
                "stream.opened",
                json!({"protocol": {"name": SCHEMA, "schema_version": 1},
                "producer": {"name": "marqdo", "version": binding.producer}, "limits": {}, "observation": {}}),
            ),
            event(
                1,
                "run.started",
                json!({"entry": binding.entry, "mode": binding.mode,
                "source": {"analysis_digest": binding.digest, "root_id": binding.root}}),
            ),
            event(
                2,
                "stdout.chunk",
                json!({"data_base64": "NAo=", "encoding": "base64", "byte_count": 2}),
            ),
            event(
                3,
                "run.failed",
                json!({"status": "error", "error": {"code": "runtime.test", "message": "test"}}),
            ),
        ];
        (binding, events)
    }

    fn decode(binding: &Binding, events: &[Value]) -> Stream {
        let ndjson = events
            .iter()
            .map(|value| format!("{value}\n"))
            .collect::<String>();
        read(ndjson.as_bytes(), binding, |_| {})
    }

    #[test]
    fn validates_stream_identity_bytes_and_terminal_boundary() {
        let (binding, mut events) = sample();
        let stream = decode(&binding, &events);
        assert!(stream.verified && !stream.invalid);
        assert_eq!(stream.stdout, b"4\n");
        assert!(stream.terminal.is_some());
        assert!(decode(&binding, &events[..3]).terminal.is_none());
        events[2]["payload"]["byte_count"] = json!(1);
        assert!(decode(&binding, &events).invalid);
        events[1]["payload"]["source"]["analysis_digest"] = json!("wrong");
        assert!(!decode(&binding, &events).verified);
    }
}
