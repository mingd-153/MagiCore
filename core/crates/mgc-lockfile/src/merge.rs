//! 3-way merge of `Lockfile` values (for branch merges / lockfile rebases).
//! Trộn lockfile 3 chiều — dùng cho merge nhánh / rebase.
//!
//! Semantics:
//! - Packages are keyed by *name*; the version is the value being merged.
//! - A package added on one side (not in base) is kept.
//! - A package removed on one side (unchanged on the other) is dropped.
//! - A package changed on both sides into different versions is a conflict
//!   → error (fail-closed, never guess which version wins).
//! - Automatic resolution of Git Conflict Markers when both sides parse.
// (v2 schema: no workspaces / direct / dev fields — simpler than the v1 merger.)

use crate::{parser, Lockfile, Package};
use std::collections::BTreeMap;

/// Parses and auto-resolves git conflict markers in a raw lockfile text when possible.
/// Both sides must parse as canonical TOML v2; otherwise returns None (caller bails).
// (Tự giải quyết conflict marker khi cả 2 phía parse được; ngược lại trả None.)
pub fn resolve_git_conflict_markers(content: &str) -> Option<Lockfile> {
    if !content.contains("<<<<<<<") || !content.contains("=======") || !content.contains(">>>>>>>")
    {
        return None;
    }

    let mut ours_lines = Vec::new();
    let mut theirs_lines = Vec::new();
    let mut in_ours = false;
    let mut in_theirs = false;

    for line in content.lines() {
        if line.starts_with("<<<<<<<") {
            in_ours = true;
            in_theirs = false;
        } else if line.starts_with("=======") {
            in_ours = false;
            in_theirs = true;
        } else if line.starts_with(">>>>>>>") {
            in_ours = false;
            in_theirs = false;
        } else if in_ours {
            ours_lines.push(line);
        } else if in_theirs {
            theirs_lines.push(line);
        } else {
            // Dòng chung ngoài marker → thuộc cả 2 phía
            ours_lines.push(line);
            theirs_lines.push(line);
        }
    }

    let ours_lock = parser::parse_lockfile(&ours_lines.join("\n")).ok()?;
    let theirs_lock = parser::parse_lockfile(&theirs_lines.join("\n")).ok()?;

    // Empty base: 3-way synthesis between the two branches
    let base_lock = Lockfile::new();
    merge3(&base_lock, &ours_lock, &theirs_lock).ok()
}

#[derive(Debug)]
pub struct MergeConflict {
    pub name: String,
    pub base_version: Option<String>,
    pub ours_version: String,
    pub theirs_version: String,
}

impl std::fmt::Display for MergeConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "conflict: package '{}' changed on both sides (base {}, ours {}, theirs {})",
            self.name,
            self.base_version.as_deref().unwrap_or("<none>"),
            self.ours_version,
            self.theirs_version
        )
    }
}

type PkgMap = BTreeMap<String, Package>;

fn index_pkgs(pkgs: &[Package]) -> PkgMap {
    pkgs.iter().map(|p| (p.name.clone(), p.clone())).collect()
}

/// Merge `theirs` into `ours` relative to `base`.
/// Keyed by name; identity (metadata) comes from `ours`.
// (Trộn theirs vào ours trên nền base — xung đột version phân kỳ → lỗi fail-closed.)
pub fn merge3(
    base: &Lockfile,
    ours: &Lockfile,
    theirs: &Lockfile,
) -> Result<Lockfile, MergeConflict> {
    let base_p = index_pkgs(&base.packages);
    let ours_p = index_pkgs(&ours.packages);
    let theirs_p = index_pkgs(&theirs.packages);

    let mut out = ours.clone();
    out.packages = vec![];

    let names: std::collections::BTreeSet<String> = base_p
        .keys()
        .cloned()
        .chain(ours_p.keys().cloned())
        .chain(theirs_p.keys().cloned())
        .collect();

    for name in &names {
        let b = base_p.get(name);
        let o = ours_p.get(name);
        let t = theirs_p.get(name);
        match (b, o, t) {
            // added on exactly one side → adoption
            (None, Some(o), None) => out.packages.push(o.clone()),
            (None, None, Some(t)) => out.packages.push(t.clone()),
            // unchanged on both sides relative to base
            (Some(b), Some(o), Some(t)) if b.version == o.version && b.version == t.version => {
                out.packages.push(o.clone());
            }
            // both bumped to the same version (base may differ) → merged
            (Some(_), Some(o), Some(t)) if o.version == t.version => {
                out.packages.push(o.clone());
            }
            // one side bumped, the other unchanged → take the bump
            (Some(b), Some(o), Some(t)) if o.version == b.version => {
                out.packages.push(t.clone());
            }
            (Some(b), Some(o), Some(t)) if t.version == b.version => {
                out.packages.push(o.clone());
            }
            // divergent bumps → fail-closed conflict
            (Some(b), Some(o), Some(t)) => {
                return Err(MergeConflict {
                    name: name.clone(),
                    base_version: Some(b.version.clone()),
                    ours_version: o.version.clone(),
                    theirs_version: t.version.clone(),
                });
            }
            // removed on one side while the other kept base version → removal stands
            (Some(b), None, Some(t)) if t.version == b.version => {}
            (Some(b), Some(o), None) if o.version == b.version => {}
            // removed on one side while the other bumped → keep the bump
            (Some(_), None, Some(t)) => out.packages.push(t.clone()),
            (Some(_), Some(o), None) => out.packages.push(o.clone()),
            // added on BOTH sides: same version → keep; divergent → conflict (git add/add model)
            (None, Some(o), Some(t)) if o.version == t.version => out.packages.push(o.clone()),
            (None, Some(o), Some(t)) => {
                return Err(MergeConflict {
                    name: name.clone(),
                    base_version: None,
                    ours_version: o.version.clone(),
                    theirs_version: t.version.clone(),
                });
            }
            // removed on both sides
            _ => {}
        }
    }

    Ok(out)
}

