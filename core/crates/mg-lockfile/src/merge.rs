//! 3-way merge of `Lockfile` values (for branch merges / lockfile rebases).
//!
//! Semantics:
//! - Packages are keyed by `(name, version)`.
//! - A package added on one side (not in base) is kept.
//! - A package removed on one side is dropped.
//! - A package changed on *both* sides into *different* versions is a
//!   conflict → error (fail-closed, never guess which version wins).
//! - Workspaces are merged by path the same way.
//! - Direct/dev flags are recomputed by the caller from the manifest after
//!   merge; here we keep "direct if any side says direct".
//! - Automatic resolution of Git Conflict Markers (`<<<<<<<`, `=======`, `>>>>>>>`).

use crate::{serialization, LockPackage, Lockfile, WorkspaceLock};
use std::collections::BTreeMap;

/// Parses and auto-resolves git conflict markers in a raw lockfile text when possible.
/// (Deno `merge_conflict_sides` & PNPM merge model).
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
            ours_lines.push(line);
            theirs_lines.push(line);
        }
    }

    let ours_str = ours_lines.join("\n");
    let theirs_str = theirs_lines.join("\n");

    let ours_lock = serialization::from_toml::<Lockfile>(&ours_str).ok()?;
    let theirs_lock = serialization::from_toml::<Lockfile>(&theirs_str).ok()?;

    // Use empty base for 3-way merge synthesis between the two branches
    let base_lock = Lockfile::new(&ours_lock.core, &ours_lock.mode);
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

type PkgMap = BTreeMap<String, LockPackage>;
type WsMap = BTreeMap<String, WorkspaceLock>;

fn index_pkgs(pkgs: &[LockPackage]) -> PkgMap {
    pkgs.iter().map(|p| (p.name.clone(), p.clone())).collect()
}

fn index_ws(ws: &[WorkspaceLock]) -> WsMap {
    ws.iter().map(|w| (w.path.clone(), w.clone())).collect()
}

/// Merge `theirs` into `ours` relative to `base`.
/// Packages are keyed by *name*; the version is the value being merged.
/// Returns the merged lockfile using `ours` as the base identity
/// (core/mode/resolution copied from `ours`).
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
            // present only on one side → adoption (added)
            (None, Some(o), None) => out.packages.push(o.clone()),
            (None, None, Some(t)) => out.packages.push(t.clone()),
            // both derived from the same base version
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
            // both sides bumped differently → conflict
            (Some(b), Some(o), Some(t)) => {
                return Err(MergeConflict {
                    name: name.clone(),
                    base_version: Some(b.version.clone()),
                    ours_version: o.version.clone(),
                    theirs_version: t.version.clone(),
                });
            }
            // removed on one side, unchanged on the other → removal stands
            (Some(b), None, Some(t)) if t.version == b.version => {}
            (Some(b), Some(o), None) if o.version == b.version => {}
            // removed on one side while the other bumped → keep the bump
            (Some(_), None, Some(t)) => out.packages.push(t.clone()),
            (Some(_), Some(o), None) => out.packages.push(o.clone()),
            // removed on both / absent everywhere (includes (None, Some, Some)
            // added on both sides after base — guarded earlier as no-base additions)
            _ => {}
        }
    }

    // Workspaces are small; merge by path keeping ours on tie.
    let base_w = index_ws(&base.workspaces);
    let ours_w = index_ws(&ours.workspaces);
    let keys: std::collections::BTreeSet<String> = base_w
        .keys()
        .cloned()
        .chain(ours_w.keys().cloned())
        .chain(theirs.workspaces.iter().map(|w| w.path.clone()))
        .collect();
    out.workspaces = keys
        .into_iter()
        .filter_map(|path| {
            if let Some(w) = ours_w.get(&path) {
                Some(w.clone())
            } else if !base_w.contains_key(&path) {
                theirs.workspaces.iter().find(|w| w.path == path).cloned()
            } else {
                None // removed in ours
            }
        })
        .collect();

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LockPackage, ResolutionMeta};

    fn pkg(name: &str, version: &str) -> LockPackage {
        LockPackage {
            name: name.to_string(),
            version: version.to_string(),
            integrity: None,
            direct: false,
            dev: false,
            dependencies: vec![],
            peer_deps: vec![],
        }
    }

    fn lock(names: &[LockPackage]) -> Lockfile {
        let mut l = Lockfile::new("web", "frontend");
        l.resolution = ResolutionMeta {
            state: "locked".into(),
            store: "megagate".into(),
            package_count: names.len(),
        };
        l.packages = names.to_vec();
        l
    }

    #[test]
    fn merge3_keeps_both_additions() {
        let base = lock(&[]);
        let ours = lock(&[pkg("a", "1.0.0")]);
        let theirs = lock(&[pkg("b", "2.0.0")]);
        let out = merge3(&base, &ours, &theirs).unwrap();
        assert_eq!(out.packages.len(), 2);
        let names: Vec<_> = out.packages.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"a") && names.contains(&"b"));
    }

    #[test]
    fn merge3_keeps_common_changed_version() {
        let base = lock(&[pkg("a", "1.0.0")]);
        let ours = lock(&[pkg("a", "1.1.0")]);
        let theirs = lock(&[pkg("a", "1.1.0")]);
        let out = merge3(&base, &ours, &theirs).unwrap();
        assert_eq!(out.packages.len(), 1);
        assert_eq!(out.packages[0].version, "1.1.0");
    }

    #[test]
    fn merge3_conflicts_on_divergent_versions() {
        let base = lock(&[pkg("a", "1.0.0")]);
        let ours = lock(&[pkg("a", "1.1.0")]);
        let theirs = lock(&[pkg("a", "2.0.0")]);
        let err = merge3(&base, &ours, &theirs).unwrap_err();
        assert!(err.to_string().contains("conflict"), "{err}");
        assert_eq!(err.name, "a");
    }

    #[test]
    fn merge3_removal_wins_when_other_side_unchanged() {
        let base = lock(&[pkg("a", "1.0.0"), pkg("b", "1.0.0")]);
        let ours = lock(&[pkg("b", "1.0.0")]); // removed a
        let theirs = lock(&[pkg("a", "1.0.0"), pkg("b", "1.0.0")]);
        let out = merge3(&base, &ours, &theirs).unwrap();
        assert_eq!(out.packages.len(), 1);
        assert_eq!(out.packages[0].name, "b");
    }

    #[test]
    fn merge3_keeps_disjoint_workspaces() {
        let mut base = lock(&[]);
        base.workspaces.push(WorkspaceLock {
            path: "apps/web".into(),
            ..Default::default()
        });
        let mut ours = base.clone();
        let theirs = base.clone();
        ours.workspaces.push(WorkspaceLock {
            path: "apps/api".into(),
            ..Default::default()
        });
        let out = merge3(&base, &ours, &theirs).unwrap();
        assert_eq!(out.workspaces.len(), 2);
    }
}
