//! Local AI model stores: Ollama and LM Studio.
//!
//! Downloaded models are slow to fetch again and are the user's choice, so
//! they are reported but never recommended. The only recommendations are
//! Ollama blobs no model refers to and interrupted Ollama downloads: nothing
//! can load them, and Ollama would download them again if needed.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{
    exists, modified_ms, name_of, outermost_total, subdirs, Ctx, ItemState, RecSpec, ToolItem,
    ToolReport, ToolSection,
};
use crate::model::{CleanupMethod, Recommendation, Tier};

const OLLAMA_REGISTRY: &str = "registry.ollama.ai";

#[derive(Deserialize)]
struct Layer {
    digest: String,
    #[serde(default)]
    size: u64,
}

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    config: Option<Layer>,
    #[serde(default)]
    layers: Vec<Layer>,
}

/// A model listed by an Ollama manifest.
struct OllamaModel {
    name: String,
    manifest: PathBuf,
    digests: Vec<String>,
    bytes: u64,
}

/// Blob file name for a digest: `sha256:abc` is stored as `sha256-abc`.
fn blob_name(digest: &str) -> String {
    digest.replacen(':', "-", 1)
}

/// Every file under `dir`, without following symbolic links.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(e.path()),
                Ok(t) if t.is_file() => out.push(e.path()),
                _ => {}
            }
        }
    }
    out
}

/// The name `ollama list` shows: `llama3:8b` for the default registry,
/// `host/namespace/model:tag` otherwise.
fn ollama_name(manifests: &Path, manifest: &Path) -> Option<String> {
    let rel = manifest.strip_prefix(manifests).ok()?;
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let [host, ns, model, tag] = parts.as_slice() else {
        return None;
    };
    Some(if host == OLLAMA_REGISTRY && ns == "library" {
        format!("{model}:{tag}")
    } else if host == OLLAMA_REGISTRY {
        format!("{ns}/{model}:{tag}")
    } else {
        format!("{host}/{ns}/{model}:{tag}")
    })
}

fn read_ollama_models(models_dir: &Path) -> Vec<OllamaModel> {
    let manifests = models_dir.join("manifests");
    let mut out: Vec<OllamaModel> = files_under(&manifests)
        .into_iter()
        .filter_map(|path| {
            let name = ollama_name(&manifests, &path)?;
            let text = std::fs::read_to_string(&path).ok()?;
            let m: Manifest = serde_json::from_str(&text).ok()?;
            let layers: Vec<Layer> = m.config.into_iter().chain(m.layers).collect();
            Some(OllamaModel {
                name,
                manifest: path,
                bytes: layers.iter().map(|l| l.size).sum(),
                digests: layers.into_iter().map(|l| l.digest).collect(),
            })
        })
        .collect();
    out.sort_by_key(|m| std::cmp::Reverse(m.bytes));
    out
}

const SAFE_ORPHAN: &str =
    "No installed model refers to this file, so Ollama never loads it. Ollama downloads layers again if a model needs them.";
const RISK_ORPHAN: &str =
    "If a download is running right now, its files can look unused until it finishes. Wait for `ollama pull` to finish first.";

/// Ollama's model folder: `OLLAMA_MODELS` on Windows when set (the
/// installer's documented way to move models to another drive), otherwise
/// `~/.ollama/models`.
fn ollama_models(ctx: &Ctx) -> PathBuf {
    cfg!(windows)
        .then(|| ctx.env_path("OLLAMA_MODELS"))
        .flatten()
        .unwrap_or_else(|| ctx.home_path(".ollama/models"))
}

fn ollama(ctx: &Ctx, sections: &mut Vec<ToolSection>, recs: &mut Vec<Recommendation>) {
    let models_dir = ollama_models(ctx);
    let blobs = models_dir.join("blobs");
    if !exists(&blobs) {
        return;
    }
    let models = read_ollama_models(&models_dir);
    let referenced: HashSet<String> = models
        .iter()
        .flat_map(|m| m.digests.iter().map(|d| blob_name(d)))
        .collect();

    // How many models share each blob, so shared layers are explained.
    let mut users: HashMap<String, usize> = HashMap::new();
    for m in &models {
        for d in &m.digests {
            *users.entry(blob_name(d)).or_default() += 1;
        }
    }

    let items: Vec<ToolItem> = models
        .iter()
        .map(|m| {
            let shared = m
                .digests
                .iter()
                .filter(|d| users.get(&blob_name(d)).copied().unwrap_or(0) > 1)
                .count();
            ToolItem {
                label: m.name.clone(),
                path: Some(m.manifest.display().to_string()),
                bytes: m.bytes,
                detail: if shared > 0 {
                    format!(
                        "Shares {shared} layer(s) with other models; remove with `ollama rm {}`",
                        m.name
                    )
                } else {
                    format!("Remove with `ollama rm {}`", m.name)
                },
                state: ItemState::Info,
                last_used_ms: modified_ms(&m.manifest),
                recommendation_id: None,
            }
        })
        .collect();
    if !items.is_empty() {
        sections.push(ToolSection {
            title: "Ollama models".into(),
            description: "Downloaded models. Sizes include layers shared with other models, so they can add up to more than the folder.".into(),
            items,
        });
    }

    let mut leftovers = Vec::new();
    for path in files_under(&blobs) {
        let name = name_of(&path);
        let partial = name.contains("-partial");
        if !partial && referenced.contains(&name) {
            continue;
        }
        let bytes = path.symlink_metadata().map(|m| m.len()).unwrap_or(0);
        let last = modified_ms(&path);
        let rec = RecSpec {
            source: "ollama",
            title: if partial {
                "Interrupted Ollama download".into()
            } else {
                "Ollama layer no model uses".into()
            },
            path: &path,
            bytes,
            tier: Tier::LikelySafe,
            methods: &[CleanupMethod::Trash],
            detected_because: if partial {
                format!("{name} is part of a download that did not finish.")
            } else {
                format!(
                    "{name} is not listed in any of the {} installed model manifest(s).",
                    models.len()
                )
            },
            safe_because: SAFE_ORPHAN,
            risks: RISK_ORPHAN,
            proposed_action: "Move to Trash",
            alternative: None,
            last_activity_ms: last,
            notes: vec!["Quit Ollama first.".into()],
        }
        .build(ctx);
        leftovers.push(ToolItem {
            label: name,
            path: Some(path.display().to_string()),
            bytes,
            detail: if partial {
                "Unfinished download".into()
            } else {
                "Not used by any model".into()
            },
            state: ItemState::Orphaned,
            last_used_ms: last,
            recommendation_id: Some(rec.id.clone()),
        });
        recs.push(rec);
    }
    if !leftovers.is_empty() {
        leftovers.sort_by_key(|i| std::cmp::Reverse(i.bytes));
        sections.push(ToolSection {
            title: "Unused Ollama files".into(),
            description: "Layers no installed model refers to, and downloads that did not finish."
                .into(),
            items: leftovers,
        });
    }
}

/// LM Studio keeps models as `<publisher>/<model>/<files>`.
fn lm_studio(ctx: &Ctx, sections: &mut Vec<ToolSection>) {
    for rel in [".lmstudio/models", ".cache/lm-studio/models"] {
        let root = ctx.home_path(rel);
        let mut items: Vec<ToolItem> = subdirs(&root)
            .into_iter()
            .flat_map(|publisher| {
                subdirs(&publisher)
                    .into_iter()
                    .map(move |m| (name_of(&publisher), m))
            })
            .map(|(publisher, dir)| ToolItem {
                label: format!("{publisher}/{}", name_of(&dir)),
                bytes: ctx.size(&dir),
                path: Some(dir.display().to_string()),
                detail: "Remove from LM Studio's My Models tab".into(),
                state: ItemState::Info,
                last_used_ms: modified_ms(&dir),
                recommendation_id: None,
            })
            .collect();
        if items.is_empty() {
            continue;
        }
        items.sort_by_key(|i| std::cmp::Reverse(i.bytes));
        sections.push(ToolSection {
            title: "LM Studio models".into(),
            description: format!("Downloaded models in ~/{rel}."),
            items,
        });
    }
}

pub fn analyze(ctx: &Ctx) -> (ToolReport, Vec<Recommendation>) {
    let mut sections = Vec::new();
    let mut recs = Vec::new();
    ollama(ctx, &mut sections, &mut recs);
    lm_studio(ctx, &mut sections);

    let roots: Vec<(PathBuf, u64)> = [".lmstudio/models", ".cache/lm-studio/models"]
        .iter()
        .map(|r| ctx.home_path(r))
        .chain([ollama_models(ctx)])
        .filter(|p| exists(p))
        .map(|p| {
            let b = ctx.size(&p);
            (p, b)
        })
        .collect();
    let models: usize = sections
        .iter()
        .filter(|s| s.title.ends_with("models"))
        .map(|s| s.items.len())
        .sum();
    let detected = !roots.is_empty();
    (
        ToolReport {
            id: "local-models".into(),
            name: "Local AI models".into(),
            detected,
            version: None,
            total_bytes: outermost_total(&roots),
            summary: if detected {
                format!("{models} downloaded model(s).")
            } else {
                "No Ollama or LM Studio models found.".into()
            },
            sections,
        },
        recs,
    )
}

#[cfg(test)]
mod tests {
    use super::super::tests::{ctx, write};
    use super::*;

    fn manifest(home: &Path, rel: &str, digests: &[(&str, u64)]) {
        let layers: Vec<String> = digests
            .iter()
            .map(|(d, s)| format!(r#"{{"mediaType":"m","digest":"{d}","size":{s}}}"#))
            .collect();
        let path = home.join(".ollama/models/manifests").join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            format!(r#"{{"schemaVersion":2,"layers":[{}]}}"#, layers.join(",")),
        )
        .unwrap();
    }

    #[test]
    fn names_models_like_ollama_list() {
        let m = Path::new("/m");
        assert_eq!(
            ollama_name(m, Path::new("/m/registry.ollama.ai/library/llama3/8b")).unwrap(),
            "llama3:8b"
        );
        assert_eq!(
            ollama_name(m, Path::new("/m/hf.co/org/repo/latest")).unwrap(),
            "hf.co/org/repo:latest"
        );
        assert!(ollama_name(m, Path::new("/m/too/short")).is_none());
    }

    #[test]
    fn recommends_only_unreferenced_and_partial_blobs() {
        let dir = crate::test_support::sandbox();
        let home = crate::platform::canonicalize(dir.path()).unwrap();
        let blobs = home.join(".ollama/models/blobs");
        write(&blobs.join("sha256-aaa"), 300);
        write(&blobs.join("sha256-bbb"), 200);
        write(&blobs.join("sha256-ccc"), 100);
        write(&blobs.join("sha256-ddd-partial"), 50);
        manifest(
            &home,
            "registry.ollama.ai/library/a/latest",
            &[("sha256:aaa", 300), ("sha256:bbb", 200)],
        );
        manifest(
            &home,
            "registry.ollama.ai/library/b/1b",
            &[("sha256:bbb", 200)],
        );

        let (report, recs) = analyze(&ctx(&home));
        assert!(report.detected);
        let models = &report.sections[0];
        assert_eq!(models.items[0].label, "a:latest");
        assert_eq!(models.items[0].bytes, 500);
        assert!(models.items[0].detail.starts_with("Shares 1 layer"));

        let mut paths: Vec<String> = recs.iter().map(|r| name_of(Path::new(&r.path))).collect();
        paths.sort();
        assert_eq!(paths, vec!["sha256-ccc", "sha256-ddd-partial"]);
        assert!(recs.iter().all(|r| r.methods == vec![CleanupMethod::Trash]));
    }

    #[test]
    fn lists_lm_studio_models_without_recommending() {
        let dir = crate::test_support::sandbox();
        let home = crate::platform::canonicalize(dir.path()).unwrap();
        write(&home.join(".lmstudio/models/pub/model-gguf/m.gguf"), 1000);
        let (report, recs) = analyze(&ctx(&home));
        assert!(recs.is_empty());
        assert_eq!(report.sections[0].items[0].label, "pub/model-gguf");
    }

    #[test]
    fn absent_when_nothing_installed() {
        let dir = crate::test_support::sandbox();
        let home = crate::platform::canonicalize(dir.path()).unwrap();
        let (report, recs) = analyze(&ctx(&home));
        assert!(!report.detected && recs.is_empty());
    }
}
