// SPDX-License-Identifier: GPL-3.0-only
//! The terminal prompt of `nvmod install --interactive` and `nvmod mods reconfigure --interactive`: it consumes the
//! FOMOD question stream of [`nvmod_api::fomod`] and reads the choices from a line source (stdin).

use nvmod_api::fomod::{self, FomodEvent, GroupEvent, NextRequest};
use nvmod_api::{CancelToken, NoProgress, Outcome};
use std::collections::BTreeMap;
use std::io::{BufRead, Write};

/// Question id to the chosen option names.
pub(crate) type Answers = BTreeMap<String, Vec<String>>;

/// Ask every open group of every step until the plan comes back; the answers of the plan.
///
/// `request` names the archive (or the Mod to reconfigure); its `answers` start empty. A line is the numbers of the
/// options (separated by commas or spaces), or an empty line to take the ticked ones. A line that breaks the group's
/// rule is asked again; the end of input stops with an error.
pub(crate) fn ask(
    request: &NextRequest,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<Answers, String> {
    let mut request = request.clone();
    loop {
        let event = match fomod::next(&request, &mut NoProgress, &CancelToken::new()) {
            Ok(Outcome::Done(event)) => event,
            Ok(Outcome::Cancelled) => return Err("cancelled".to_owned()),
            Err(e) => return Err(e.to_string()),
        };
        match event {
            FomodEvent::Plan(plan) => return Ok(plan.answers),
            FomodEvent::Step(step) => {
                let _ = writeln!(out, "Step {}: {}", step.index + 1, step.step);
                let mut asked = false;
                for group in step
                    .groups
                    .iter()
                    .filter(|g| g.decided.is_none() && !g.answered)
                {
                    let chosen = ask_group(group, input, out)?;
                    request.answers.insert(group.id.clone(), chosen);
                    asked = true;
                }
                if !asked {
                    return Err(format!("step `{}` has nothing to answer", step.step));
                }
            }
        }
    }
}

fn ask_group(
    group: &GroupEvent,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<Vec<String>, String> {
    loop {
        let _ = writeln!(
            out,
            "Group {} ({}, choose {} to {})",
            group.name,
            group.kind.as_str(),
            group.min,
            group.max
        );
        for (i, o) in group.options.iter().enumerate() {
            let tick = if o.selected { "x" } else { " " };
            let kind = o.kind.map_or_else(String::new, |k| format!(" [{k:?}]"));
            let _ = writeln!(out, "  {}. [{tick}] {}{kind}", i + 1, o.name);
            if !o.description.is_empty() {
                let _ = writeln!(out, "       {}", o.description);
            }
        }
        let _ = write!(out, "choose> ");
        let _ = out.flush();
        let mut line = String::new();
        if input.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            return Err(format!(
                "input ended before the group `{}` was answered",
                group.id
            ));
        }
        let chosen = match parse(line.trim(), group) {
            Ok(c) => c,
            Err(why) => {
                let _ = writeln!(out, "  {why}");
                continue;
            }
        };
        if chosen.len() < group.min || chosen.len() > group.max {
            let _ = writeln!(out, "  choose {} to {} options", group.min, group.max);
            continue;
        }
        return Ok(chosen);
    }
}

fn parse(line: &str, group: &GroupEvent) -> Result<Vec<String>, String> {
    if line.is_empty() {
        return Ok(group
            .options
            .iter()
            .filter(|o| o.selected)
            .map(|o| o.name.clone())
            .collect());
    }
    let mut chosen = Vec::new();
    for token in line.split([',', ' ']).filter(|t| !t.is_empty()) {
        let n: usize = token
            .parse()
            .map_err(|_| format!("`{token}` is not an option number"))?;
        let option = n
            .checked_sub(1)
            .and_then(|i| group.options.get(i))
            .ok_or_else(|| format!("there is no option {n}"))?;
        if !option.usable {
            return Err(format!("option {n} cannot be chosen"));
        }
        if !chosen.contains(&option.name) {
            chosen.push(option.name.clone());
        }
    }
    Ok(chosen)
}
