// SPDX-License-Identifier: AGPL-3.0-or-later

//! `kitab agent-skill`: print the agent skill, or install it where Claude
//! Code, Codex or other agents look for skills.

use crate::cx::{self, Cx, shown};
use crate::out::{CliError, Result};
use serde_json::json;
use std::path::PathBuf;

/// The skill, built into the binary so it always matches the commands.
pub const SKILL: &str = include_str!("../../../skills/kitab/SKILL.md");

fn home() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

pub fn agent_skill(cx: &mut Cx) -> Result<()> {
    cx.no_more_args(0)?;
    let Some(target) = cx.args.value("install").map(str::to_string) else {
        if cx.out.json {
            cx.done(json!({"skill": SKILL}), "");
        } else {
            cx.out.content(SKILL);
        }
        return Ok(());
    };
    let base = match target.as_str() {
        "claude" | "codex" | "agents" => {
            let home = home().ok_or_else(|| {
                CliError::failed("can't find your home folder (HOME or USERPROFILE is not set)")
            })?;
            home.join(format!(".{target}")).join("skills")
        }
        dir => PathBuf::from(dir),
    };
    let file = base.join("kitab").join("SKILL.md");
    let existing = std::fs::read_to_string(&file).ok();
    if existing.as_deref() == Some(SKILL) {
        cx.done(
            json!({"installed": shown(&file), "changed": false}),
            &format!("The kitab skill is already up to date in {}", shown(&file)),
        );
        return Ok(());
    }
    if existing.is_some() && !cx.force() {
        return Err(CliError::exists(format!(
            "{} exists and differs (add --force to replace it)",
            shown(&file)
        )));
    }
    cx::make_parent(&file)?;
    cx::write_atomic(&file, SKILL.as_bytes())?;
    cx.done(
        json!({"installed": shown(&file), "changed": true}),
        &format!("Installed the kitab skill to {}", shown(&file)),
    );
    Ok(())
}
