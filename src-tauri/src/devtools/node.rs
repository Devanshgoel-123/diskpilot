//! Node.js: dependencies per project (with activity), package manager caches,
//! and Node versions installed with nvm.

use std::path::{Path, PathBuf};

use super::{
    exists, modified_ms, name_of, outermost_total, subdirs, Ctx, ItemState, ToolItem, ToolReport,
    ToolSection, STALE_DAYS,
};
use crate::model::Recommendation;

const CACHES_MAC: &[(&str, &str, &str)] = &[
    ("npm cache", ".npm", "npm cache clean --force"),
    ("pnpm store", "Library/pnpm/store", "pnpm store prune"),
    ("pnpm store", ".pnpm-store", "pnpm store prune"),
    ("pnpm store", ".local/share/pnpm/store", "pnpm store prune"),
    ("Yarn cache", "Library/Caches/Yarn", "yarn cache clean"),
    (
        "Yarn Berry cache",
        ".yarn/berry/cache",
        "yarn cache clean --mirror",
    ),
    ("Bun cache", ".bun/install/cache", "bun pm cache rm"),
];

const CACHES_WINDOWS: &[(&str, &str, &str)] = &[
    (
        "npm cache",
        "AppData/Local/npm-cache",
        "npm cache clean --force",
    ),
    ("pnpm store", "AppData/Local/pnpm/store", "pnpm store prune"),
    ("Yarn cache", "AppData/Local/Yarn/Cache", "yarn cache clean"),
    (
        "Yarn Berry cache",
        ".yarn/berry/cache",
        "yarn cache clean --mirror",
    ),
    ("Bun cache", ".bun/install/cache", "bun pm cache rm"),
];

const CACHES: &[(&str, &str, &str)] = if cfg!(windows) {
    CACHES_WINDOWS
} else {
    CACHES_MAC
};

const MANIFESTS: &[&str] = &[
    "package.json",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "bun.lockb",
];

/// Latest change to a project's dependency manifests.
pub fn project_activity_ms(project: &Path) -> Option<i64> {
    MANIFESTS
        .iter()
        .filter_map(|f| modified_ms(&project.join(f)))
        .max()
}

/// The version nvm uses by default, from `~/.nvm/alias/default`.
fn nvm_default(home: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(home.join(".nvm/alias/default")).ok()?;
    let v = raw.trim().trim_start_matches('v').to_string();
    (!v.is_empty()).then_some(v)
}

/// Does installed version folder `v22.11.0` satisfy alias `22` / `22.11` / `22.11.0`?
fn matches_alias(version_dir: &str, alias: &str) -> bool {
    let v = version_dir.trim_start_matches('v');
    v == alias || v.starts_with(&format!("{alias}."))
}

pub fn analyze(ctx: &Ctx) -> (ToolReport, Vec<Recommendation>) {
    let mut sections = Vec::new();
    let mut roots: Vec<(PathBuf, u64)> = Vec::new();

    // ---- node_modules per project (from the scan) ---------------------------
    let mut nm_items: Vec<ToolItem> = ctx
        .detected("node.node-modules")
        .into_iter()
        .map(|(path, bytes)| {
            let project = path.parent().map(Path::to_path_buf).unwrap_or_default();
            let activity = project_activity_ms(&project);
            let days = activity.map(|ms| ctx.days_since(ms));
            let stale = days.is_some_and(|d| d >= STALE_DAYS);
            roots.push((path.clone(), bytes));
            ToolItem {
                label: name_of(&project),
                path: Some(path.display().to_string()),
                bytes,
                detail: match days {
                    Some(d) if stale => format!("Package files unchanged for {d} days"),
                    Some(d) => format!("Package files changed {d} days ago"),
                    None => "No package manifest found next to it".into(),
                },
                state: if stale {
                    ItemState::Stale
                } else {
                    ItemState::Current
                },
                last_used_ms: activity,
                recommendation_id: Some(path.display().to_string()),
            }
        })
        .collect();
    nm_items.sort_by_key(|i| (i.state != ItemState::Stale, std::cmp::Reverse(i.bytes)));
    let stale: Vec<&ToolItem> = nm_items
        .iter()
        .filter(|i| i.state == ItemState::Stale)
        .collect();
    let stale_count = stale.len();
    let stale_bytes: u64 = stale.iter().map(|i| i.bytes).sum();
    if !nm_items.is_empty() {
        sections.push(ToolSection {
            title: "Project dependencies".into(),
            description: format!(
                "node_modules folders found by the last scan. Projects whose package files have not changed in {STALE_DAYS} days are marked inactive; each is under Recommendations."
            ),
            items: nm_items,
        });
    }

    // ---- Package manager caches ---------------------------------------------
    let cache_items: Vec<ToolItem> = CACHES
        .iter()
        .filter_map(|(label, rel, cmd)| {
            let path = ctx.home_path(rel);
            exists(&path).then(|| {
                let bytes = ctx.size(&path);
                roots.push((path.clone(), bytes));
                ToolItem {
                    label: (*label).into(),
                    path: Some(path.display().to_string()),
                    bytes,
                    detail: format!("Clean with `{cmd}`"),
                    state: ItemState::Info,
                    last_used_ms: modified_ms(&path),
                    recommendation_id: None,
                }
            })
        })
        .collect();
    if !cache_items.is_empty() {
        sections.push(ToolSection {
            title: "Package manager caches".into(),
            description: "Downloaded packages shared by all projects. Regenerable; caches that DiskPilot can clear appear under Recommendations.".into(),
            items: cache_items,
        });
    }

    // ---- nvm versions ---------------------------------------------------------
    let (nvm_dir, default) = if cfg!(windows) {
        // nvm-windows: its root folder, and the version its link points to.
        let (dir, in_use) = super::versions::nvm_windows(ctx);
        (dir, in_use.into_iter().next())
    } else {
        (ctx.home_path(".nvm/versions/node"), nvm_default(ctx.home))
    };
    let nvm_items: Vec<ToolItem> = subdirs(&nvm_dir)
        .into_iter()
        // nvm-windows keeps other folders next to the versions.
        .filter(|dir| !cfg!(windows) || super::versions::parse_version(&name_of(dir)).is_some())
        .map(|dir| {
            let v = name_of(&dir);
            let is_default = default.as_deref().is_some_and(|d| matches_alias(&v, d));
            let bytes = ctx.size(&dir);
            ToolItem {
                detail: if is_default && cfg!(windows) {
                    "Version in use (nvm use)".into()
                } else if is_default {
                    "Default version (nvm alias default)".into()
                } else {
                    format!(
                        "Remove with `nvm uninstall {}` if no project needs it",
                        v.trim_start_matches('v')
                    )
                },
                label: v,
                path: Some(dir.display().to_string()),
                bytes,
                state: if is_default {
                    ItemState::Current
                } else {
                    ItemState::Info
                },
                last_used_ms: modified_ms(&dir),
                recommendation_id: None,
            }
        })
        .collect();
    if !nvm_items.is_empty() {
        roots.push((nvm_dir.clone(), nvm_items.iter().map(|i| i.bytes).sum()));
        sections.push(ToolSection {
            title: "Node.js versions (nvm)".into(),
            description:
                "Projects may pin versions in .nvmrc; DiskPilot does not remove runtimes itself."
                    .into(),
            items: nvm_items,
        });
    }

    let next: Vec<(PathBuf, u64)> = ctx.detected("node.next-build");
    roots.extend(next.iter().cloned());

    let detected = !sections.is_empty();
    let mut summary = String::new();
    if let Some(s) = sections.iter().find(|s| s.title == "Project dependencies") {
        summary.push_str(&format!("{} projects with node_modules", s.items.len()));
        if stale_count > 0 {
            summary.push_str(&format!(
                ", {stale_count} inactive for {STALE_DAYS}+ days ({})",
                crate::docker::actions::human(stale_bytes)
            ));
        }
        summary.push('.');
    } else if detected {
        summary.push_str(
            "Package manager caches and runtimes found. Scan to see project dependencies.",
        );
    } else {
        summary.push_str("No Node.js tooling found.");
    }

    (
        ToolReport {
            id: "node".into(),
            name: "Node.js".into(),
            detected,
            version: default.map(|d| format!("nvm default {d}")),
            total_bytes: outermost_total(&roots),
            summary,
            sections,
        },
        // node_modules recommendations come from the scan's policies.
        Vec::new(),
    )
}

#[cfg(test)]
mod tests {
    use super::super::tests::{ctx, write};
    use super::*;

    #[test]
    fn nvm_alias_matching() {
        assert!(matches_alias("v22.11.0", "22"));
        assert!(matches_alias("v22.11.0", "22.11.0"));
        assert!(!matches_alias("v2.0.0", "22"));
        assert!(!matches_alias("v220.0.0", "22"));
    }

    #[test]
    fn reports_caches_and_default_runtime() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        write(&home.join(".npm/_cacache/a"), 1000);
        write(&home.join(".nvm/versions/node/v20.1.0/bin/node"), 100);
        write(&home.join(".nvm/versions/node/v22.11.0/bin/node"), 100);
        std::fs::create_dir_all(home.join(".nvm/alias")).unwrap();
        std::fs::write(home.join(".nvm/alias/default"), "22\n").unwrap();

        let (report, _) = analyze(&ctx(&home));
        assert!(report.detected);
        let nvm = report
            .sections
            .iter()
            .find(|s| s.title.contains("nvm"))
            .unwrap();
        assert_eq!(
            nvm.items
                .iter()
                .filter(|i| i.state == ItemState::Current)
                .count(),
            1
        );
        assert!(report
            .sections
            .iter()
            .any(|s| s.title == "Package manager caches"));
    }
}
