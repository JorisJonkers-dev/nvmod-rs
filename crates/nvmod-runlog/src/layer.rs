// SPDX-License-Identifier: GPL-3.0-only
//! The two `tracing` layers nvmod installs: human text on standard error, and the redacted NDJSON run log.

use std::collections::BTreeMap;
use std::fmt::{Debug, Write as _};
use std::fs::File;
use std::io::Write;
use std::sync::Mutex;

use serde_json::{Map, Value, json};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Record};
use tracing::{Event, Id, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

use crate::redact::Redactor;

/// The fields a span or an event can carry that have a column of their own in the run log.
const COLUMNS: [&str; 7] = [
    "run_id",
    "op",
    "step",
    "tool",
    "tool_version",
    "exit_code",
    "duration_ms",
];
/// Columns that hold program-made text and are written as they are.
const VERBATIM: [&str; 1] = ["run_id"];

/// The fields of one span or event.
#[derive(Default)]
struct Fields(BTreeMap<String, Value>);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.0
            .insert(field.name().to_owned(), Value::String(format!("{value:?}")));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0
            .insert(field.name().to_owned(), Value::String(value.to_owned()));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().to_owned(), json!(value));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_owned(), json!(value));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().to_owned(), json!(value));
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.0.insert(field.name().to_owned(), json!(value));
    }
}

fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// What the layers keep about an event: the span context merged with the event's own fields.
struct Collected {
    columns: BTreeMap<String, Value>,
    message: String,
}

fn collect<S>(ctx: &Context<'_, S>, event: &Event<'_>) -> Collected
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    let mut merged: BTreeMap<String, Value> = BTreeMap::new();
    if let Some(scope) = ctx.event_scope(event) {
        for span in scope.from_root() {
            if let Some(fields) = span.extensions().get::<Fields>() {
                merged.extend(fields.0.clone());
            }
        }
    }
    let mut own = Fields::default();
    event.record(&mut own);
    let mut message = own
        .0
        .remove("message")
        .map(|v| text(&v))
        .unwrap_or_default();
    let mut columns = BTreeMap::new();
    for (key, value) in merged.into_iter().chain(own.0) {
        if COLUMNS.contains(&key.as_str()) {
            columns.insert(key, value);
        } else {
            // Fields without a column go to the end of the message, as `key=value`.
            let sep = if message.is_empty() { "" } else { " " };
            let _ = write!(message, "{sep}{key}={}", text(&value));
        }
    }
    Collected { columns, message }
}

fn remember_span<S>(attrs: &Attributes<'_>, id: &Id, ctx: &Context<'_, S>)
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    // Both layers remember the span's fields; the first one to see the span stores them.
    if let Some(span) = ctx.span(id) {
        let mut extensions = span.extensions_mut();
        if extensions.get_mut::<Fields>().is_none() {
            let mut fields = Fields::default();
            attrs.record(&mut fields);
            extensions.insert(fields);
        }
    }
}

fn update_span<S>(id: &Id, values: &Record<'_>, ctx: &Context<'_, S>)
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    // Recorded twice when both layers are installed, which is harmless: the values are overwritten.
    if let Some(span) = ctx.span(id)
        && let Some(fields) = span.extensions_mut().get_mut::<Fields>()
    {
        values.record(fields);
    }
}

/// Human text on standard error for events at or above `level` (default `warn`).
pub struct StderrLayer {
    level: LevelFilter,
}

impl StderrLayer {
    /// A layer printing events up to `level`.
    #[must_use]
    pub fn new(level: LevelFilter) -> StderrLayer {
        StderrLayer { level }
    }
}

impl<S> Layer<S> for StderrLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        remember_span(attrs, id, &ctx);
    }
    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        update_span(id, values, &ctx);
    }
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        if *event.metadata().level() > self.level {
            return;
        }
        let c = collect(&ctx, event);
        let context = ["tool", "step"]
            .iter()
            .filter_map(|k| c.columns.get(*k))
            .map(text)
            .collect::<Vec<_>>()
            .join(" ");
        let level = event.metadata().level().as_str().to_ascii_lowercase();
        let context = if context.is_empty() {
            String::new()
        } else {
            format!(" [{context}]")
        };
        let _ = writeln!(
            std::io::stderr().lock(),
            "nvmod: {level}{context}: {}",
            c.message
        );
    }
}

/// The run log: one redacted JSON object per line, written as the events happen.
pub struct FileLayer {
    file: Mutex<File>,
    redactor: Redactor,
    level: LevelFilter,
}

impl FileLayer {
    /// A layer writing events up to `level` to `file`, through `redactor`.
    #[must_use]
    pub fn new(file: File, redactor: Redactor, level: LevelFilter) -> FileLayer {
        FileLayer {
            file: Mutex::new(file),
            redactor,
            level,
        }
    }

    /// One line of the log: the keys of the schema, every program-made text through the redactor.
    fn line(&self, level: &str, c: &Collected) -> String {
        let mut object = Map::new();
        object.insert("ts".into(), Value::String(crate::time::now()));
        object.insert("level".into(), Value::String(level.to_owned()));
        for key in COLUMNS {
            let value = match c.columns.get(key) {
                Some(Value::String(s)) if VERBATIM.contains(&key) => Value::String(s.clone()),
                Some(Value::String(s)) => Value::String(self.redactor.apply(s)),
                Some(other) => other.clone(),
                None => Value::Null,
            };
            object.insert(key.to_owned(), value);
        }
        object.insert("msg".into(), Value::String(self.redactor.apply(&c.message)));
        Value::Object(object).to_string()
    }

    /// Write `line` and a newline; a failing disk never fails the command.
    pub(crate) fn write_line(&self, line: &str) {
        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(file, "{line}");
        }
    }
}

impl<S> Layer<S> for FileLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        remember_span(attrs, id, &ctx);
    }
    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        update_span(id, values, &ctx);
    }
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        if *event.metadata().level() > self.level {
            return;
        }
        let c = collect(&ctx, event);
        let level = event.metadata().level().as_str().to_ascii_lowercase();
        self.write_line(&self.line(&level, &c));
    }
}
