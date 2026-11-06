//! Go: module cache, build cache, downloaded toolchains and installed programs.
//!
//! Locations come from `go env`, so a custom GOPATH, GOMODCACHE or GOCACHE
//! set with `go env -w` is found too. Without a `go` binary, Go's macOS
//! defaults are checked instead.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;

use super::versions::parse_version;
use super::{
    exists, modified_ms, name_of, outermost_total, subdirs, Ctx, ItemState, RecSpec, ToolItem,
    ToolReport, ToolSection,
};
use crate::model::Recommendation;
use crate::recommend::policies::policy_for;

/// The official installer, then Homebrew on Apple silicon and on Intel.
const SYSTEM_GO: &[&str] = &[
    "/usr/local/go/bin/go",
    "/opt/homebrew/bin/go",
    "/usr/local/bin/go",
];

/// Where mise and asdf install each Go version, below the home folder, and
/// the binary inside one version's folder.
const MANAGED_GO: &[(&str, &str)] = &[
    (".local/share/mise/installs/go", "bin/go"),
    (".asdf/installs/golang", "go/bin/go"),
];

fn go_binaries(home: &Path) -> Vec<PathBuf> {
    SYSTEM_GO
        .iter()
        .map(PathBuf::from)
        .chain(managed_go(home))
        .collect()
}

/// Go versions installed by mise or asdf, newest first. Their shims are not
/// used: an app does not get the shell setup the shims rely on.
fn managed_go(home: &Path) -> Vec<PathBuf> {
    let mut found: Vec<(Vec<u64>, PathBuf)> = MANAGED_GO
        .iter()
        .flat_map(|(dir, bin)| {
            subdirs(&home.join(dir))
                .into_iter()
                .filter_map(move |v| Some((parse_version(&name_of(&v))?, v.join(bin))))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, go)| go).collect()
}

#[derive(Debug, PartialEq)]
struct GoEnv {
    version: Option<String>,
    /// The first GOPATH entry, where `go install` puts programs.
    gopath: PathBuf,
    modcache: PathBuf,
    /// `None` when Go's build cache is turned off.
    cache: Option<PathBuf>,
}

#[derive(Deserialize)]
#[serde(rename_all = "UPPERCASE")]
struct RawGoEnv {
    goversion: Option<String>,
    gopath: String,
    gomodcache: String,
    gocache: String,
}

impl GoEnv {
    fn defaults(home: &Path) -> Self {
        Self {
            version: None,
            gopath: home.join("go"),
            modcache: home.join("go/pkg/mod"),
            cache: Some(home.join("Library/Caches/go-build")),
        }
    }

    /// Parse `go env -json` output. Paths that are not absolute are refused.
    fn parse(json: &str) -> Option<Self> {
        let raw: RawGoEnv = serde_json::from_str(json).ok()?;
        let absolute = |p: &str| Some(PathBuf::from(p)).filter(|p| p.is_absolute());
        Some(Self {
            version: raw.goversion.filter(|v| !v.is_empty()),
            gopath: absolute(raw.gopath.split(':').next()?)?,
            modcache: absolute(&raw.gomodcache)?,
            cache: absolute(&raw.gocache),
        })
    }

    /// Ask the first installed `go`. `GOTOOLCHAIN=local` stops a go.mod in
    /// the working folder from making `go` download another toolchain first.
    fn query(binaries: &[PathBuf]) -> Option<Self> {
        let go = binaries.iter().find(|b| b.is_file())?;
        let mut command = Command::new(go);
        command
            .args([
                "env",
                "-json",
                "GOVERSION",
                "GOPATH",
                "GOMODCACHE",
                "GOCACHE",
            ])
            .env("GOTOOLCHAIN", "local")
            .current_dir("/");
        Self::parse(&super::run_command(command, Duration::from_secs(10))?)
    }
}

pub fn analyze(ctx: &Ctx) -> (ToolReport, Vec<Recommendation>) {
    let env = GoEnv::query(&go_binaries(ctx.home)).unwrap_or_else(|| GoEnv::defaults(ctx.home));
    report(ctx, &env)
}

/// A cache Go manages, recommended under the same policy as when the scan
/// finds it in its default location.
fn cache_item(
    ctx: &Ctx,
    rule_id: &str,
    label: &str,
    var: &str,
    path: &Path,
) -> Option<(ToolItem, Option<Recommendation>)> {
    let policy = policy_for(rule_id)?;
    let bytes = ctx.size(path);
    let rec = (bytes >= policy.min_bytes).then(|| {
        RecSpec {
            source: "go",
            title: format!("Go {}", label.to_lowercase()),
            path,
            bytes,
            tier: policy.tier,
            methods: policy.methods,
            detected_because: format!("Go keeps its {} here ({var}).", label.to_lowercase()),
            safe_because: policy.safe_because,
            risks: policy.risks,
            proposed_action: policy.proposed_action,
            alternative: policy.alternative,
            last_activity_ms: modified_ms(path),
            notes: Vec::new(),
        }
        .build(ctx)
    });
    let item = ToolItem {
        label: label.into(),
        path: Some(path.display().to_string()),
        bytes,
        detail: policy
            .alternative
            .map(|cmd| format!("Clear with `{cmd}`"))
            .unwrap_or_default(),
        state: ItemState::Info,
        last_used_ms: modified_ms(path),
        recommendation_id: rec.as_ref().map(|r| r.id.clone()),
    };
    Some((item, rec))
}

/// Toolchains Go downloaded because a go.mod asked for another version.
/// Folder names look like `toolchain@v0.0.1-go1.26.0.darwin-arm64`.
fn toolchain_items(ctx: &Ctx, modcache: &Path) -> Vec<ToolItem> {
    subdirs(&modcache.join("golang.org"))
        .into_iter()
        .filter_map(|dir| {
            let name = name_of(&dir);
            let (version, _platform) = name.strip_prefix("toolchain@v0.0.1-")?.rsplit_once('.')?;
            Some(ToolItem {
                label: version.into(),
                bytes: ctx.size(&dir),
                path: Some(dir.display().to_string()),
                detail: "Downloaded for a project whose go.mod asks for it".into(),
                state: ItemState::Info,
                last_used_ms: modified_ms(&dir),
                recommendation_id: None,
            })
        })
        .collect()
}

fn cache_items(ctx: &Ctx, env: &GoEnv) -> (Vec<ToolItem>, Vec<Recommendation>) {
    let mut recs = Vec::new();
    let items = [
        (
            "go.mod-cache",
            "Module cache",
            "GOMODCACHE",
            Some(&env.modcache),
        ),
        (
            "go.build-cache",
            "Build cache",
            "GOCACHE",
            env.cache.as_ref(),
        ),
    ]
    .into_iter()
    .filter_map(|(rule, label, var, path)| {
        let path = path.filter(|p| exists(p))?;
        let (item, rec) = cache_item(ctx, rule, label, var, path)?;
        recs.extend(rec);
        Some(item)
    })
    .collect();
    (items, recs)
}

fn program_items(ctx: &Ctx, gopath: &Path) -> Vec<ToolItem> {
    let bin = gopath.join("bin");
    if !exists(&bin) {
        return Vec::new();
    }
    vec![ToolItem {
        label: "GOPATH/bin".into(),
        bytes: ctx.size(&bin),
        path: Some(bin.display().to_string()),
        detail: String::new(),
        state: ItemState::Info,
        last_used_ms: modified_ms(&bin),
        recommendation_id: None,
    }]
}

fn section(title: &str, description: &str, items: Vec<ToolItem>) -> Option<ToolSection> {
    (!items.is_empty()).then(|| ToolSection {
        title: title.into(),
        description: description.into(),
        items,
    })
}

fn report(ctx: &Ctx, env: &GoEnv) -> (ToolReport, Vec<Recommendation>) {
    let (caches, recs) = cache_items(ctx, env);
    let toolchains = toolchain_items(ctx, &env.modcache);
    let toolchain_count = toolchains.len();
    let sections: Vec<ToolSection> = [
        section(
            "Caches",
            "Go downloads or rebuilds everything here when it is needed again.",
            caches,
        ),
        section(
            "Downloaded toolchains",
            "Go versions fetched automatically into the module cache. They are read-only; `go clean -modcache` removes them with the cache.",
            toolchains,
        ),
        section(
            "Installed programs",
            "Programs installed with `go install`. DiskPilot does not remove them.",
            program_items(ctx, &env.gopath),
        ),
    ]
    .into_iter()
    .flatten()
    .collect();

    let detected = !sections.is_empty() || env.version.is_some();
    let roots: Vec<(PathBuf, u64)> = sections
        .iter()
        .flat_map(|s| s.items.iter())
        .filter_map(|i| i.path.as_ref().map(|p| (PathBuf::from(p), i.bytes)))
        .collect();
    let summary = match (detected, roots.len(), toolchain_count) {
        (false, ..) => "No Go tooling found.".into(),
        (true, 0, _) => "No caches or installed programs yet.".into(),
        (true, n, 0) => format!("{n} location(s) analyzed."),
        (true, n, t) => format!("{n} location(s) analyzed, including {t} downloaded toolchain(s)."),
    };
    (
        ToolReport {
            id: "go".into(),
            name: "Go".into(),
            detected,
            version: env.version.clone(),
            total_bytes: outermost_total(&roots),
            summary,
            sections,
        },
        recs,
    )
}

#[cfg(test)]
mod tests {
    use super::super::tests::{ctx, write};
    use super::*;
    use crate::model::{CleanupMethod, Tier};
    use crate::scanner::{DirIndex, DirStats};

    fn sized(index: &mut DirIndex, path: &Path, bytes: u64) {
        let stats = DirStats {
            bytes,
            files: 1,
            readable: true,
            ..Default::default()
        };
        index.insert(path.to_path_buf(), stats);
    }

    #[test]
    fn reads_where_go_keeps_its_data() {
        // Output of `go env -json GOVERSION GOPATH GOMODCACHE GOCACHE`, Go 1.27.
        let env = GoEnv::parse(
            r#"{
	"GOCACHE": "off",
	"GOMODCACHE": "/Users/a/work/pkg/mod",
	"GOPATH": "/Users/a/work:/Users/a/old",
	"GOVERSION": "go1.27.3"
}"#,
        )
        .unwrap();
        assert_eq!(
            env,
            GoEnv {
                version: Some("go1.27.3".into()),
                gopath: "/Users/a/work".into(),
                modcache: "/Users/a/work/pkg/mod".into(),
                cache: None,
            }
        );
        assert_eq!(
            GoEnv::parse(r#"{"GOPATH": "go", "GOMODCACHE": "/m", "GOCACHE": "/c"}"#),
            None
        );
        assert_eq!(GoEnv::parse("go: unknown flag -json"), None);
    }

    /// A `go` that answers `go env -json` only when it may not download a toolchain.
    fn fake_go(path: &Path, version: &str) {
        use std::os::unix::fs::PermissionsExt;
        let script = format!(
            r#"#!/bin/sh
[ "$GOTOOLCHAIN" = local ] && [ "$1 $2" = "env -json" ] || exit 1
echo '{{"GOVERSION": "{version}", "GOPATH": "/g", "GOMODCACHE": "/g/pkg/mod", "GOCACHE": "/c"}}'
"#
        );
        write(path, 0);
        std::fs::write(path, script).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn asks_the_installed_go_without_letting_it_download_a_toolchain() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let go = home.join("go-bin");
        fake_go(&go, "go1.27.3");
        let missing = home.join("missing/go");
        let env = GoEnv::query(&[missing.clone(), go]).unwrap();
        assert_eq!(env.version.as_deref(), Some("go1.27.3"));
        assert_eq!(env.cache, Some(PathBuf::from("/c")));
        assert_eq!(GoEnv::query(&[missing]), None);
    }

    #[test]
    fn finds_the_newest_go_installed_by_mise_or_asdf() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let mise = home.join(".local/share/mise/installs/go");
        let asdf = home.join(".asdf/installs/golang");
        fake_go(&mise.join("1.26.0/bin/go"), "go1.26.0");
        fake_go(&asdf.join("1.27.3/go/bin/go"), "go1.27.3");
        fake_go(&mise.join("1.9.7/bin/go"), "go1.9.7");
        std::fs::create_dir_all(mise.join("not-a-version/bin")).unwrap();
        std::os::unix::fs::symlink(mise.join("1.26.0"), mise.join("latest")).unwrap();

        assert_eq!(
            managed_go(&home),
            vec![
                asdf.join("1.27.3/go/bin/go"),
                mise.join("1.26.0/bin/go"),
                mise.join("1.9.7/bin/go"),
            ]
        );
        let env = GoEnv::query(&managed_go(&home)).unwrap();
        assert_eq!(env.version.as_deref(), Some("go1.27.3"));
    }

    #[test]
    fn reports_caches_and_downloaded_toolchains() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let env = GoEnv::defaults(&home);
        let toolchain = env
            .modcache
            .join("golang.org/toolchain@v0.0.1-go1.26.0.darwin-arm64");
        write(&toolchain.join("bin/go"), 10);
        write(&env.modcache.join("golang.org/x/text@v0.1.0/doc.go"), 10);
        write(&env.cache.clone().unwrap().join("00/a-d"), 10);
        write(&env.gopath.join("bin/gopls"), 10);
        let mut index = DirIndex::new();
        sized(&mut index, &env.modcache, 3_000_000_000);
        sized(&mut index, &toolchain, 231_000_000);
        sized(&mut index, env.cache.as_ref().unwrap(), 900_000_000);
        sized(&mut index, &env.gopath.join("bin"), 40_000_000);
        let mut c = ctx(&home);
        c.index = Some(&index);

        let (report, recs) = report(&c, &env);
        assert!(report.detected);
        assert_eq!(report.total_bytes, 3_940_000_000);
        let toolchains = report
            .sections
            .iter()
            .find(|s| s.title == "Downloaded toolchains")
            .unwrap();
        assert_eq!(toolchains.items[0].label, "go1.26.0");

        let modules = recs
            .iter()
            .find(|r| r.path == env.modcache.display().to_string())
            .unwrap();
        assert_eq!(modules.tier, Tier::ReviewFirst);
        assert!(modules.methods.is_empty());
        assert_eq!(modules.classification.rule_id, "go.mod-cache");
        let build = recs
            .iter()
            .find(|r| Some(Path::new(&r.path)) == env.cache.as_deref())
            .unwrap();
        assert_eq!(
            build.methods,
            vec![CleanupMethod::Trash, CleanupMethod::Delete]
        );
    }

    #[test]
    fn a_custom_module_cache_is_never_offered_for_the_trash() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let env = GoEnv {
            modcache: home.join(".cache/gomod"),
            ..GoEnv::defaults(&home)
        };
        write(&env.modcache.join("golang.org/x/text@v0.1.0/doc.go"), 10);
        let mut index = DirIndex::new();
        sized(&mut index, &env.modcache, 3_000_000_000);
        let mut c = ctx(&home);
        c.index = Some(&index);
        let (_, go_recs) = report(&c, &env);

        // The scan sees ~/.cache/gomod as one more CLI tool cache.
        let path = env.modcache.display().to_string();
        let scan_rec = crate::model::Recommendation {
            id: path.clone(),
            path: path.clone(),
            methods: vec![CleanupMethod::Trash],
            ..crate::test_support::recommendation()
        };
        let slot = crate::state::ScanSlot {
            recommendations: std::sync::Arc::new(vec![scan_rec]),
            devtools_recommendations: std::sync::Arc::new(go_recs),
            ..Default::default()
        };
        let merged = slot.all_recommendations();
        let modules: Vec<_> = merged.iter().filter(|r| r.path == path).collect();
        assert_eq!(modules.len(), 1);
        assert!(modules[0].methods.is_empty());
        assert_eq!(
            modules[0].alternative.as_deref(),
            Some("go clean -modcache")
        );
    }

    #[test]
    fn a_small_build_cache_is_listed_but_not_recommended() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let env = GoEnv::defaults(&home);
        write(&env.cache.clone().unwrap().join("00/a-d"), 10);
        let (report, recs) = report(&ctx(&home), &env);
        let cache = &report.sections[0].items[0];
        assert_eq!(cache.label, "Build cache");
        assert_eq!(cache.recommendation_id, None);
        assert_eq!(report.summary, "1 location(s) analyzed.");
        assert!(recs.is_empty());
    }

    #[test]
    fn nothing_is_detected_without_go_or_its_folders() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let (report, recs) = report(&ctx(&home), &GoEnv::defaults(&home));
        assert!(!report.detected);
        assert!(report.sections.is_empty());
        assert!(recs.is_empty());
    }

    #[test]
    fn go_without_caches_says_so() {
        let dir = crate::test_support::sandbox();
        let home = dir.path().canonicalize().unwrap();
        let env = GoEnv {
            version: Some("go1.27.3".into()),
            ..GoEnv::defaults(&home)
        };
        let (report, _) = report(&ctx(&home), &env);
        assert!(report.detected);
        assert_eq!(report.summary, "No caches or installed programs yet.");
    }
}
