//! Predefined Docker operations.
//!
//! Every action has a preview that lists the exact command and its effects,
//! and every execution rebuilds that preview from fresh Docker state and
//! refuses to run if a precondition no longer holds. There is deliberately no
//! action that removes volumes, force-removes anything, or prunes broadly.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::cli::{DockerCli, DockerError};
use super::{short_id, DockerOverview, DockerState};
use crate::model::{ActionStatus, CleanupItemResult, CleanupRecord, ItemStatus};
use crate::scanner::now_ms;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DockerAction {
    RemoveContainer { id: String },
    RemoveImage { id: String },
    PruneBuildCache,
}

impl DockerAction {
    pub fn is_destructive(&self) -> bool {
        matches!(
            self,
            DockerAction::RemoveContainer { .. }
                | DockerAction::RemoveImage { .. }
                | DockerAction::PruneBuildCache
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AffectedItem {
    pub label: String,
    pub detail: String,
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerActionPreview {
    pub action: DockerAction,
    pub title: String,
    /// The exact argument vector that will run, starting with "docker".
    pub command: Vec<String>,
    pub destructive: bool,
    pub affected: Vec<AffectedItem>,
    pub effects: Vec<String>,
    pub warnings: Vec<String>,
    /// Set when the action cannot run; the reason is shown instead.
    pub blocked: Option<String>,
    pub reclaim_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerActionResult {
    pub preview: DockerActionPreview,
    pub succeeded: bool,
    pub output: String,
    pub error: Option<String>,
    /// History entry for destructive actions.
    pub record: Option<CleanupRecord>,
}

/// Container/image ids as Docker prints them: 12–64 hex chars, optionally
/// prefixed with "sha256:". Nothing else can reach the command line.
pub fn validate_id(id: &str) -> Result<&str, DockerError> {
    let hex = id.strip_prefix("sha256:").unwrap_or(id);
    let ok = (12..=64).contains(&hex.len()) && hex.bytes().all(|b| b.is_ascii_hexdigit());
    if ok {
        Ok(id)
    } else {
        Err(DockerError::InvalidId(id.chars().take(80).collect()))
    }
}

/// Volume names: Docker's own rule `[a-zA-Z0-9][a-zA-Z0-9_.-]+`.
pub fn validate_volume_name(name: &str) -> Result<&str, DockerError> {
    let mut chars = name.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphanumeric());
    let rest_ok = chars.all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c));
    if first_ok && rest_ok && name.len() <= 255 {
        Ok(name)
    } else {
        Err(DockerError::InvalidId(name.chars().take(80).collect()))
    }
}

fn cmd(args: &[&str]) -> Vec<String> {
    std::iter::once("docker")
        .chain(args.iter().copied())
        .map(str::to_string)
        .collect()
}

/// Describe what `action` would do, given current Docker state.
pub fn preview(
    action: &DockerAction,
    o: &DockerOverview,
) -> Result<DockerActionPreview, DockerError> {
    let mut p = DockerActionPreview {
        action: action.clone(),
        title: String::new(),
        command: Vec::new(),
        destructive: action.is_destructive(),
        affected: Vec::new(),
        effects: Vec::new(),
        warnings: Vec::new(),
        blocked: None,
        reclaim_bytes: None,
    };
    if o.status.state != DockerState::Running {
        p.blocked = Some(DockerError::NotRunning.to_string());
    }

    match action {
        DockerAction::RemoveContainer { id } => {
            validate_id(id)?;
            let c = o
                .containers
                .iter()
                .find(|c| &c.id == id)
                .ok_or_else(|| DockerError::Blocked("Container no longer exists.".into()))?;
            p.title = format!("Remove container {}", c.name);
            // No -f (never remove running containers) and no -v (never volumes).
            p.command = cmd(&["rm", id]);
            p.affected.push(AffectedItem {
                label: c.name.clone(),
                detail: format!("{} · {}", c.image, c.status),
                bytes: Some(c.size_bytes),
            });
            p.reclaim_bytes = Some(c.size_bytes);
            p.effects.push(
                "Deletes the container and its writable layer (files changed inside it that are not in a volume)."
                    .into(),
            );
            if c.running {
                p.blocked = Some("The container is running. Stop it in Docker Desktop or with `docker stop` first.".into());
            }
            if !c.mounts.is_empty() {
                p.effects
                    .push(format!("Its volumes are kept: {}.", c.mounts.join(", ")));
            }
            if let Some(project) = &c.compose_project {
                p.warnings.push(format!(
                    "Part of Compose project “{project}”. `docker compose up` will create it again."
                ));
            }
            p.warnings.push(
                "A stopped container is not necessarily unused: you may want to run it again."
                    .into(),
            );
        }
        DockerAction::RemoveImage { id } => {
            validate_id(id)?;
            let img = o
                .images
                .iter()
                .find(|i| &i.id == id)
                .ok_or_else(|| DockerError::Blocked("Image no longer exists.".into()))?;
            let name = if img.dangling {
                format!("untagged image {}", img.short_id)
            } else {
                format!("{}:{}", img.repository, img.tag)
            };
            p.title = format!("Remove image {name}");
            // No -f: Docker refuses to remove images that containers use.
            p.command = cmd(&["image", "rm", id]);
            p.affected.push(AffectedItem {
                label: name,
                detail: format!("ID {}", short_id(&img.id)),
                bytes: Some(img.unique_bytes),
            });
            p.reclaim_bytes = Some(img.unique_bytes);
            p.effects.push(format!(
                "Frees about {} of layers used only by this image.",
                human(img.unique_bytes)
            ));
            if img.shared_bytes > 0 {
                p.effects.push(format!(
                    "{} of its layers are shared with other images and stay on disk.",
                    human(img.shared_bytes)
                ));
            }
            if !img.used_by.is_empty() {
                p.blocked = Some(format!(
                    "Used by container{} {}. Remove {} first; Docker will not remove an image a container depends on.",
                    if img.used_by.len() == 1 { "" } else { "s" },
                    img.used_by.join(", "),
                    if img.used_by.len() == 1 { "it" } else { "them" }
                ));
            }
            if !img.dangling {
                p.warnings
                    .push("You will need to pull or rebuild this image to use it again.".into());
            }
        }
        DockerAction::PruneBuildCache => {
            let unused: Vec<_> = o.build_cache.iter().filter(|b| !b.in_use).collect();
            p.title = "Remove unused build cache".into();
            p.command = cmd(&["builder", "prune", "--all", "--force"]);
            let total: u64 = unused.iter().map(|b| b.size_bytes).sum();
            // Records can share data, so their sizes overlap; prefer Docker's
            // own reclaimable figure when it reports one.
            let reported = o.usage.build_cache.reclaimable_bytes;
            p.reclaim_bytes = Some(if reported > 0 {
                reported.min(total)
            } else {
                total
            });
            if unused.iter().any(|b| b.shared) || (reported > 0 && reported < total) {
                p.effects.push(
                    "Some records share data, so their individual sizes add up to more than is freed."
                        .into(),
                );
            }
            p.affected = unused
                .iter()
                .map(|b| AffectedItem {
                    label: if b.description.is_empty() {
                        b.id.clone()
                    } else {
                        b.description.clone()
                    },
                    detail: format!("{} · last used {}", b.cache_type, b.last_used),
                    bytes: Some(b.size_bytes),
                })
                .collect();
            p.effects.push(format!(
                "Removes {} build cache record{} not used by a running build.",
                unused.len(),
                if unused.len() == 1 { "" } else { "s" }
            ));
            p.effects
                .push("Images, containers and volumes are not affected.".into());
            p.warnings
                .push("The next builds re-run steps that were cached and are slower.".into());
            if unused.is_empty() {
                p.blocked = Some("There is no unused build cache.".into());
            }
        }
    }
    Ok(p)
}

pub fn human(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1e9 {
        format!("{:.1} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.1} MB", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.1} KB", b / 1e3)
    } else {
        format!("{bytes} B")
    }
}

/// Execute after re-validating against fresh state.
pub fn execute(
    cli: &DockerCli,
    action: &DockerAction,
    fresh: &DockerOverview,
) -> Result<DockerActionResult, DockerError> {
    let preview = preview(action, fresh)?;
    if let Some(reason) = &preview.blocked {
        return Err(DockerError::Blocked(reason.clone()));
    }
    let args: Vec<&str> = preview.command.iter().skip(1).map(String::as_str).collect();
    let started = now_ms();
    let outcome = cli.run(&args, Duration::from_secs(180));
    let command_line = preview.command.join(" ");
    let (succeeded, output, error) = match outcome {
        Ok(out) => (true, out.trim().to_string(), None),
        Err(e) => (false, String::new(), Some(e.to_string())),
    };

    let record = action.is_destructive().then(|| {
        let bytes = if succeeded {
            preview.reclaim_bytes.unwrap_or(0)
        } else {
            0
        };
        let items = vec![CleanupItemResult {
            target: preview
                .affected
                .first()
                .map(|a| a.label.clone())
                .unwrap_or_else(|| preview.title.clone()),
            operation: command_line.clone(),
            bytes,
            status: if succeeded {
                ItemStatus::Succeeded
            } else {
                ItemStatus::Failed
            },
            error: error.clone(),
        }];
        CleanupRecord {
            id: format!("docker-{started}"),
            kind: "docker".into(),
            title: preview.title.clone(),
            started_at_ms: started,
            finished_at_ms: now_ms(),
            status: ActionStatus::from_items(&items),
            bytes_freed: bytes,
            bytes_trashed: 0,
            items,
        }
    });

    Ok(DockerActionResult {
        preview,
        succeeded,
        output,
        error,
        record,
    })
}

pub fn inspect(cli: &DockerCli, kind: &str, id: &str) -> Result<String, DockerError> {
    let id = match kind {
        "container" | "image" => validate_id(id)?,
        "volume" => validate_volume_name(id)?,
        _ => return Err(DockerError::InvalidId(kind.chars().take(20).collect())),
    };
    let out = cli.run(&["inspect", "--type", kind, id], Duration::from_secs(20))?;
    // Re-indent for display; fall back to the raw text.
    Ok(serde_json::from_str::<serde_json::Value>(&out)
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or(out))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FindingLevel {
    Info,
    Suggestion,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticFinding {
    pub level: FindingLevel,
    pub title: String,
    pub detail: String,
    pub bytes: Option<u64>,
}

/// Disk-usage diagnostics. `vm_disk` is (allocated, logical) bytes of Docker
/// Desktop's VM disk image, when found by the caller.
pub fn diagnostics(o: &DockerOverview, vm_disk: Option<(u64, u64)>) -> Vec<DiagnosticFinding> {
    use FindingLevel::*;
    let mut f = Vec::new();
    let finding = |level, title: String, detail: &str, bytes| DiagnosticFinding {
        level,
        title,
        detail: detail.to_string(),
        bytes,
    };

    let u = &o.usage;
    let total = u.images.size_bytes
        + u.containers.size_bytes
        + u.volumes.size_bytes
        + u.build_cache.size_bytes;
    f.push(finding(
        Info,
        format!("Docker reports {} in use", human(total)),
        "Images, containers, volumes and build cache inside Docker's VM.",
        Some(total),
    ));

    if let Some((allocated, logical)) = vm_disk {
        f.push(finding(
            Info,
            format!("VM disk image uses {} on your Mac", human(allocated)),
            &format!(
                "Docker Desktop stores everything in one sparse disk image with a {} limit. Space freed inside Docker returns to macOS as the image is trimmed, which can take a few minutes.",
                human(logical)
            ),
            Some(allocated),
        ));
    }

    let dangling: Vec<_> = o
        .images
        .iter()
        .filter(|i| i.dangling && i.used_by.is_empty())
        .collect();
    if !dangling.is_empty() {
        let bytes = dangling.iter().map(|i| i.unique_bytes).sum();
        f.push(finding(
            Suggestion,
            format!("{} untagged image{} not used by any container", dangling.len(), if dangling.len() == 1 { "" } else { "s" }),
            "Usually left behind by rebuilding an image under the same tag. Review and remove them individually in the Images tab.",
            Some(bytes),
        ));
    }

    let unused_tagged: Vec<_> = o
        .images
        .iter()
        .filter(|i| !i.dangling && i.used_by.is_empty())
        .collect();
    if !unused_tagged.is_empty() {
        f.push(finding(
            Info,
            format!("{} tagged image{} without containers", unused_tagged.len(), if unused_tagged.len() == 1 { "" } else { "s" }),
            "Not being used right now, but you may need them to start containers later. Removing them means pulling or rebuilding.",
            Some(unused_tagged.iter().map(|i| i.unique_bytes).sum()),
        ));
    }

    let stopped = o.containers.iter().filter(|c| !c.running).count();
    if stopped > 0 {
        f.push(finding(
            Info,
            format!("{stopped} stopped container{}", if stopped == 1 { "" } else { "s" }),
            "Stopped containers use little space themselves but keep their images from being removed. DiskPilot never assumes they are unneeded.",
            None,
        ));
    }

    let orphan_volumes: Vec<_> = o
        .volumes
        .iter()
        .filter(|v| v.links == Some(0) && v.used_by.is_empty())
        .collect();
    if !orphan_volumes.is_empty() {
        f.push(finding(
            Warning,
            format!("{} volume{} not attached to any container", orphan_volumes.len(), if orphan_volumes.len() == 1 { "" } else { "s" }),
            "Volumes often hold databases. DiskPilot never removes volumes; if you are sure one is unneeded, back it up and remove it yourself with `docker volume rm`.",
            Some(orphan_volumes.iter().filter_map(|v| v.size_bytes).sum()),
        ));
    }

    let cache: u64 = o
        .build_cache
        .iter()
        .filter(|b| !b.in_use)
        .map(|b| b.size_bytes)
        .sum();
    if cache > 0 {
        f.push(finding(
            Suggestion,
            format!("{} of unused build cache", human(cache)),
            "Safe to remove; builds re-run cached steps. Use “Remove unused build cache” in the Build cache tab to preview it.",
            Some(cache),
        ));
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::tests::sample;

    const WEB: &str = "f608a28855b6cf35511b2f2f441f313d65bce8fb897cd60324557f78665d3d19";
    const DB: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90112233445566778899aabbccddeeff00";
    const PG_IMAGE: &str =
        "sha256:0cefaabd882e42bcb2a2f3dc56d3ce4770560ba62fd7f197f1a2e008fd5731ab";
    const DANGLING: &str =
        "sha256:ef60141d283bb798a5e9b4820aeda6a8995f25244ec5359b0d76dd849de69db8";

    #[test]
    fn rejects_anything_that_is_not_an_id() {
        assert!(validate_id(WEB).is_ok());
        assert!(validate_id(PG_IMAGE).is_ok());
        assert!(validate_id("--force").is_err());
        assert!(validate_id("abc").is_err());
        assert!(validate_id("f608a28855b6; rm -rf /").is_err());
        assert!(validate_volume_name("ledger_pg").is_ok());
        assert!(validate_volume_name("-v").is_err());
        assert!(validate_volume_name("a b").is_err());
    }

    #[test]
    fn running_containers_cannot_be_removed() {
        let p = preview(&DockerAction::RemoveContainer { id: WEB.into() }, &sample()).unwrap();
        assert!(p.blocked.is_some());
        assert_eq!(p.command, vec!["docker", "rm", WEB]);
    }

    #[test]
    fn removing_a_stopped_container_keeps_volumes_and_never_forces() {
        let p = preview(&DockerAction::RemoveContainer { id: DB.into() }, &sample()).unwrap();
        assert!(p.blocked.is_none());
        assert!(!p
            .command
            .iter()
            .any(|a| a == "-f" || a == "-v" || a == "--volumes"));
        assert!(p.effects.iter().any(|e| e.contains("ledger_pg")));
        assert!(p.warnings.iter().any(|w| w.contains("Compose")));
    }

    #[test]
    fn images_in_use_are_blocked_with_an_explanation() {
        let p = preview(
            &DockerAction::RemoveImage {
                id: PG_IMAGE.into(),
            },
            &sample(),
        )
        .unwrap();
        assert!(p.blocked.as_deref().unwrap().contains("ledger-db-1"));
        let d = preview(
            &DockerAction::RemoveImage {
                id: DANGLING.into(),
            },
            &sample(),
        )
        .unwrap();
        assert!(d.blocked.is_none());
        assert_eq!(d.reclaim_bytes, Some(174_800_000));
        assert!(d.effects.iter().any(|e| e.contains("shared")));
    }

    #[test]
    fn build_cache_prune_lists_exactly_the_unused_records() {
        let p = preview(&DockerAction::PruneBuildCache, &sample()).unwrap();
        assert_eq!(p.affected.len(), 1);
        assert_eq!(p.reclaim_bytes, Some(120_000_000));
        assert_eq!(
            p.command,
            vec!["docker", "builder", "prune", "--all", "--force"]
        );
    }

    #[test]
    fn nothing_runs_when_docker_is_down() {
        let mut o = sample();
        o.status.state = DockerState::NotRunning;
        let p = preview(&DockerAction::RemoveContainer { id: DB.into() }, &o).unwrap();
        assert!(p.blocked.is_some());
    }

    #[test]
    fn diagnostics_flag_orphan_volumes_without_offering_removal() {
        let f = diagnostics(&sample(), Some((6_000_000_000, 64_000_000_000)));
        let vol = f.iter().find(|d| d.title.contains("volume")).unwrap();
        assert_eq!(vol.level, FindingLevel::Warning);
        assert!(vol.detail.contains("never removes volumes"));
        assert!(f.iter().any(|d| d.title.contains("untagged")));
        assert!(f.iter().any(|d| d.title.contains("build cache")));
    }
}
