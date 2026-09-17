//! Pagination + formatting of a sorted result set (design §5, §3.1).
//!
//! Most endpoints hand us their complete, already-sorted `Vec<T>`; we compute
//! `total`, slice the requested page, wrap the taginfo envelope (unless
//! `--no-envelope`), and print in the chosen format. An endpoint whose full
//! result is too expensive to build can instead hand over a [`Page`] it
//! selected itself.

use crate::cli::{Cli, Format};
use crate::model::Envelope;
use anyhow::Result;
use serde::Serialize;
use serde_json::Value;

/// One page of a sorted result: the total row count before paging, and the
/// requested page's rows.
pub struct Page<T> {
    pub total: usize,
    pub rows: Vec<T>,
}

impl<T> Page<T> {
    /// Page a complete, sorted result set.
    pub fn from_rows(rows: Vec<T>, page: usize, rp: usize) -> Self {
        Page {
            total: rows.len(),
            rows: paginate(rows, page, rp),
        }
    }
}

/// The `[start, end)` row range of `page` at `rp` rows a page, clamped to
/// `total`. `rp == 0` means "all rows".
pub(crate) fn page_bounds(total: usize, page: usize, rp: usize) -> (usize, usize) {
    if rp == 0 {
        return (0, total);
    }
    let start = page.saturating_sub(1).saturating_mul(rp).min(total);
    (start, start.saturating_add(rp).min(total))
}

/// Slice `rows` to the requested page. `rp == 0` means "all rows".
pub(crate) fn paginate<T>(rows: Vec<T>, page: usize, rp: usize) -> Vec<T> {
    if rp == 0 {
        return rows;
    }
    let start = page.saturating_sub(1) * rp;
    rows.into_iter().skip(start).take(rp).collect()
}

/// Paginate and wrap (or not) in the taginfo envelope. Pure -- no I/O, no
/// printing -- so both the CLI's [`emit`] and the `serve`-mode HTTP handlers
/// can share one implementation of the envelope shaping.
pub fn envelope<T: Serialize>(
    url: String,
    data_until: &str,
    page: usize,
    rp: usize,
    no_envelope: bool,
    rows: Vec<T>,
) -> serde_json::Result<Value> {
    envelope_page(
        url,
        data_until,
        page,
        rp,
        no_envelope,
        Page::from_rows(rows, page, rp),
    )
}

/// [`envelope`] for a page the endpoint already selected.
pub fn envelope_page<T: Serialize>(
    url: String,
    data_until: &str,
    page: usize,
    rp: usize,
    no_envelope: bool,
    Page { total, rows: data }: Page<T>,
) -> serde_json::Result<Value> {
    if no_envelope {
        serde_json::to_value(&data)
    } else {
        serde_json::to_value(Envelope {
            url,
            data_until: data_until.to_string(),
            page,
            rp,
            total,
            data,
        })
    }
}

/// Build the envelope, paginate, and print. `url` is the canonical invocation
/// string for this request; `data_until` comes from the opened archive.
pub fn emit<T: Serialize>(cli: &Cli, data_until: &str, url: String, rows: Vec<T>) -> Result<()> {
    print(
        cli,
        &envelope(url, data_until, cli.page, cli.rp, cli.no_envelope, rows)?,
    )
}

/// [`emit`] for a page the endpoint already selected.
pub fn emit_page<T: Serialize>(
    cli: &Cli,
    data_until: &str,
    url: String,
    page: Page<T>,
) -> Result<()> {
    print(
        cli,
        &envelope_page(url, data_until, cli.page, cli.rp, cli.no_envelope, page)?,
    )
}

fn print(cli: &Cli, json: &Value) -> Result<()> {
    match cli.format {
        Format::Json => println!("{}", serde_json::to_string(json)?),
        Format::Pretty => println!("{}", serde_json::to_string_pretty(json)?),
        Format::Table => print_table(json),
    }
    Ok(())
}

/// Best-effort aligned table over a flat array of JSON objects. Not a stable
/// interface — `json`/`pretty` are the contract.
fn print_table(json: &Value) {
    let rows = match json {
        Value::Array(rows) => rows.as_slice(),
        // Enveloped: the rows live under `data`.
        Value::Object(map) => match map.get("data") {
            Some(Value::Array(rows)) => rows.as_slice(),
            _ => return println!("{json}"),
        },
        _ => return println!("{json}"),
    };
    let Some(Value::Object(first)) = rows.first() else {
        return;
    };
    let cols: Vec<&String> = first.keys().collect();

    let mut widths: Vec<usize> = cols.iter().map(|c| c.len()).collect();
    for row in rows {
        if let Value::Object(map) = row {
            for (i, c) in cols.iter().enumerate() {
                let w = cell(map.get(*c)).len();
                if w > widths[i] {
                    widths[i] = w;
                }
            }
        }
    }

    let header: Vec<String> = cols
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{:<width$}", c, width = widths[i]))
        .collect();
    println!("{}", header.join("  "));
    for row in rows {
        if let Value::Object(map) = row {
            let line: Vec<String> = cols
                .iter()
                .enumerate()
                .map(|(i, c)| format!("{:<width$}", cell(map.get(*c)), width = widths[i]))
                .collect();
            println!("{}", line.join("  "));
        }
    }
}

fn cell(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}
