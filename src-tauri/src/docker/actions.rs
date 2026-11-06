//! Predefined Docker operations.
//!
//! Every action has a preview that lists the exact command and its effects.
//! The engine keeps what was shown; execution rebuilds the preview from fresh
//! Docker state and refuses to run unless it is exactly what the person
//! reviewed, still allowed, and, for volume deletes, confirmed by typing the
//! name. Nothing is ever
//! force-removed or pruned broadly. Volumes are removed only one at a time, at
//! the user's explicit request, when no other container (running or stopped)
//! uses them: either on their own, or ticked one by one when removing the
//! container that uses them. DiskPilot never suggests or automates removing a
//! volume.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::cli::{DockerCli, DockerError};
use super::{short_id, DockerOverview, DockerState};
use crate::model::{ActionStatus, CleanupItemResult, CleanupRecord, ItemStatus};
use crate::scanner::now_ms;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DockerAction {
    StopContainer {
        id: String,
    },
    /// Remove a container. `stop_first` allows a running container, which is
    /// stopped normally before it is removed. `volumes` are volumes the user
    /// ticked to delete along with it; each must be used by this container
    /// and no other.
    RemoveContainer {
        id: String,
        #[serde(default, rename = "stopFirst")]
        stop_first: bool,
        #[serde(default)]
        volumes: Vec<String>,
    },
    RemoveImage {
        id: String,
    },
    RemoveVolume {
        name: String,
    },
    PruneBuildCache,
}

impl DockerAction {
    pub fn is_destructive(&self) -> bool {
        matches!(
            self,
            DockerAction::RemoveContainer { .. }
                | DockerAction::RemoveImage { .. }
                | DockerAction::RemoveVolume { .. }
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

/// One command of an action, run in order; a failure stops the rest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerStep {
    /// The exact argument vector that will run, starting with "docker".
    pub command: Vec<String>,
    /// What the step acts on, for history.
    pub target: String,
    /// Space this step frees, when known.
    pub bytes: Option<u64>,
}

/// A volume the container uses, offered for deletion alongside it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeChoice {
    pub name: String,
    pub bytes: Option<u64>,
    pub compose_project: Option<String>,
    pub selected: bool,
    /// Set when the volume cannot be deleted with this container.
    pub kept_because: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerActionPreview {
    /// Set by the command that stores the preview; execution names it.
    #[serde(default)]
    pub id: String,
    pub action: DockerAction,
    pub title: String,
    pub steps: Vec<DockerStep>,
    /// Volumes the user can choose to delete too (container removal only).
    pub volume_choices: Vec<VolumeChoice>,
    pub destructive: bool,
    pub affected: Vec<AffectedItem>,
    pub effects: Vec<String>,
    pub warnings: Vec<String>,
    /// Set when the action cannot run; the reason is shown instead.
    pub blocked: Option<String>,
    pub reclaim_bytes: Option<u64>,
    /// A name the person must type to confirm, when a volume is deleted.
    /// Checked by the engine, not only by the dialog.
    pub confirm_name: Option<String>,
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

/// Why an action that stops a `--rm` container must not run: Docker would
/// delete its anonymous volumes with it, which nobody reviewed.
fn auto_remove_blocker(c: &super::DockerContainer) -> Option<String> {
    let anonymous = c
        .mounts
        .iter()
        .filter(|m| super::is_anonymous_volume(m))
        .count();
    (anonymous > 0).then(|| {
        format!(
            "{} was started with --rm, so stopping it makes Docker delete it together with {} anonymous volume{} and the data in {}. DiskPilot will not do that. If you want it, stop the container yourself.",
            c.name,
            anonymous,
            if anonymous == 1 { "" } else { "s" },
            if anonymous == 1 { "it" } else { "them" },
        )
    })
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

fn step(args: &[&str], target: &str, bytes: Option<u64>) -> DockerStep {
    DockerStep {
        command: std::iter::once("docker")
            .chain(args.iter().copied())
            .map(str::to_string)
            .collect(),
        target: target.to_string(),
        bytes,
    }
}

/// The named volumes container `c` uses, and whether each can be deleted
/// with it: only when no other container, running or stopped, uses it.
/// Bind mounts and names Docker shortened are not volumes we can match, so
/// they are left out.
fn volume_choices(
    c: &super::DockerContainer,
    o: &DockerOverview,
    selected: &[String],
) -> Vec<VolumeChoice> {
    let mut seen = std::collections::BTreeSet::new();
    c.mounts
        .iter()
        .filter(|m| seen.insert(m.as_str()))
        .filter_map(|m| o.volumes.iter().find(|v| &v.name == m))
        .map(|v| {
            let others: std::collections::BTreeSet<&str> = o
                .containers
                .iter()
                .filter(|other| other.id != c.id && other.mounts.iter().any(|m| m == &v.name))
                .map(|other| other.name.as_str())
                .chain(
                    v.used_by
                        .iter()
                        .map(String::as_str)
                        .filter(|n| *n != c.name),
                )
                .collect();
            let kept_because = if !others.is_empty() {
                Some(format!(
                    "Also used by {}.",
                    others.into_iter().collect::<Vec<_>>().join(", ")
                ))
            } else if v.links.is_some_and(|n| n > 1) {
                Some("Another container also uses it.".into())
            } else {
                None
            };
            VolumeChoice {
                selected: kept_because.is_none() && selected.iter().any(|s| s == &v.name),
                name: v.name.clone(),
                bytes: v.size_bytes,
                compose_project: v.compose_project.clone(),
                kept_because,
            }
        })
        .collect()
}

/// Describe what `action` would do, given current Docker state.
pub fn preview(
    action: &DockerAction,
    o: &DockerOverview,
) -> Result<DockerActionPreview, DockerError> {
    let mut p = DockerActionPreview {
        id: String::new(),
        action: action.clone(),
        title: String::new(),
        steps: Vec::new(),
        volume_choices: Vec::new(),
        destructive: action.is_destructive(),
        affected: Vec::new(),
        effects: Vec::new(),
        warnings: Vec::new(),
        blocked: None,
        reclaim_bytes: None,
        confirm_name: None,
    };
    if o.status.state != DockerState::Running {
        p.blocked = Some(DockerError::NotRunning.to_string());
    }

    match action {
        DockerAction::StopContainer { id } => {
            validate_id(id)?;
            let c = o
                .containers
                .iter()
                .find(|c| &c.id == id)
                .ok_or_else(|| DockerError::Blocked("Container no longer exists.".into()))?;
            p.title = format!("Stop container {}", c.name);
            // A normal stop: Docker asks the processes to exit, then waits.
            p.steps.push(step(&["stop", id], &c.name, None));
            p.affected.push(AffectedItem {
                label: c.name.clone(),
                detail: format!("{} · {}", c.image, c.status),
                bytes: None,
            });
            if c.auto_remove {
                p.effects.push(
                    "It was started with --rm, so Docker deletes the container as soon as it stops. Its named volumes are kept."
                        .into(),
                );
                if let Some(reason) = auto_remove_blocker(c) {
                    p.blocked = Some(reason);
                }
            } else {
                p.effects.push(
                    "Stops the container's processes. Its files and volumes are kept, and you can start it again in Docker Desktop or with `docker start`."
                        .into(),
                );
            }
            if !c.running {
                p.blocked = Some("The container is already stopped.".into());
            }
            if let Some(project) = &c.compose_project {
                p.warnings.push(format!(
                    "Part of Compose project “{project}”. Other services in it may stop working while this one is stopped."
                ));
            }
        }
        DockerAction::RemoveContainer {
            id,
            stop_first,
            volumes,
        } => {
            validate_id(id)?;
            let c = o
                .containers
                .iter()
                .find(|c| &c.id == id)
                .ok_or_else(|| DockerError::Blocked("Container no longer exists.".into()))?;
            p.title = format!("Remove container {}", c.name);
            if c.auto_remove {
                if let Some(reason) = auto_remove_blocker(c) {
                    p.blocked = Some(reason);
                }
            }
            if c.running {
                if *stop_first {
                    p.steps.push(step(&["stop", id], &c.name, None));
                    p.effects.push(
                        "Stops the container normally first: Docker asks its processes to exit, then waits."
                            .into(),
                    );
                } else {
                    p.blocked = Some("The container is running. Stop it first.".into());
                }
            }
            // No -f (never force-remove) and no -v (volumes only when ticked).
            p.steps.push(step(&["rm", id], &c.name, Some(c.size_bytes)));
            p.affected.push(AffectedItem {
                label: c.name.clone(),
                detail: format!("{} · {}", c.image, c.status),
                bytes: Some(c.size_bytes),
            });
            p.effects.push(
                "Deletes the container and its writable layer (files changed inside it that are not in a volume)."
                    .into(),
            );

            p.volume_choices = volume_choices(c, o, volumes);
            for name in volumes {
                validate_volume_name(name)?;
                match p.volume_choices.iter().find(|v| &v.name == name) {
                    Some(v) if v.kept_because.is_none() => {}
                    Some(v) => {
                        p.blocked = Some(format!(
                            "Volume {name} cannot be deleted: {}",
                            v.kept_because.as_deref().unwrap_or_default()
                        ));
                    }
                    None => {
                        p.blocked = Some(format!("{} does not use a volume named {name}.", c.name));
                    }
                }
            }
            let mut deleted = Vec::new();
            for v in p.volume_choices.iter().filter(|v| v.selected) {
                p.steps
                    .push(step(&["volume", "rm", &v.name], &v.name, v.bytes));
                p.affected.push(AffectedItem {
                    label: v.name.clone(),
                    detail: match &v.compose_project {
                        Some(project) => format!("Volume · Compose project “{project}”"),
                        None => "Volume".into(),
                    },
                    bytes: v.bytes,
                });
                deleted.push(v.name.as_str());
            }
            let kept: Vec<&str> = p
                .volume_choices
                .iter()
                .filter(|v| !v.selected)
                .map(|v| v.name.as_str())
                .collect();
            if !deleted.is_empty() {
                p.confirm_name = Some(c.name.clone());
                p.effects.push(format!(
                    "Permanently deletes volume{} {}, with everything stored in {}. This does not go to the Trash and cannot be undone.",
                    if deleted.len() == 1 { "" } else { "s" },
                    deleted.join(", "),
                    if deleted.len() == 1 { "it" } else { "them" }
                ));
            }
            if !kept.is_empty() {
                p.effects
                    .push(format!("These volumes are kept: {}.", kept.join(", ")));
            }
            p.reclaim_bytes = Some(p.steps.iter().filter_map(|s| s.bytes).sum());
            if let Some(project) = &c.compose_project {
                p.warnings.push(format!(
                    "Part of Compose project “{project}”. `docker compose up` will create it again{}.",
                    if deleted.is_empty() {
                        ""
                    } else {
                        ", with empty volumes in place of the deleted ones"
                    }
                ));
            }
            p.warnings.push(
                "A stopped container is not necessarily unused: you may want to run it again."
                    .into(),
            );
            if !deleted.is_empty() {
                p.warnings.push(
                    "Back up any volume data you might need first, for example with `docker run --rm -v VOLUME:/data -v \"$PWD\":/backup alpine tar czf /backup/volume.tgz /data`."
                        .into(),
                );
            }
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
            p.steps
                .push(step(&["image", "rm", id], &name, Some(img.unique_bytes)));
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
        DockerAction::RemoveVolume { name } => {
            validate_volume_name(name)?;
            let v = o
                .volumes
                .iter()
                .find(|v| &v.name == name)
                .ok_or_else(|| DockerError::Blocked("Volume no longer exists.".into()))?;
            p.title = format!("Delete volume {}", v.name);
            p.confirm_name = Some(v.name.clone());
            // No -f: Docker refuses to remove a volume any container uses.
            p.steps
                .push(step(&["volume", "rm", name], name, v.size_bytes));
            p.affected.push(AffectedItem {
                label: v.name.clone(),
                detail: match &v.compose_project {
                    Some(project) => format!("{} · Compose project “{project}”", v.driver),
                    None => v.driver.clone(),
                },
                bytes: v.size_bytes,
            });
            p.reclaim_bytes = v.size_bytes;
            p.effects.push(
                "Permanently deletes everything stored in the volume, such as database files and uploads. It does not go to the Trash and cannot be undone."
                    .into(),
            );
            let users: Vec<&str> = o
                .containers
                .iter()
                .filter(|c| c.mounts.iter().any(|m| m == name))
                .map(|c| c.name.as_str())
                .chain(v.used_by.iter().map(String::as_str))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            if !users.is_empty() || v.links.is_some_and(|n| n > 0) {
                p.blocked = Some(if users.is_empty() {
                    "A container uses this volume. Remove that container first.".into()
                } else {
                    format!(
                        "Used by container{} {}. Remove {} first.",
                        if users.len() == 1 { "" } else { "s" },
                        users.join(", "),
                        if users.len() == 1 { "it" } else { "them" }
                    )
                });
            }
            if let Some(project) = &v.compose_project {
                p.warnings.push(format!(
                    "Belongs to Compose project “{project}”. `docker compose up` creates an empty volume in its place; the old data is not restored."
                ));
            }
            p.warnings.push(
                "If you might need this data, back it up first, for example with `docker run --rm -v VOLUME:/data -v \"$PWD\":/backup alpine tar czf /backup/volume.tgz /data`."
                    .into(),
            );
        }
        DockerAction::PruneBuildCache => {
            let unused: Vec<_> = o.build_cache.iter().filter(|b| !b.in_use).collect();
            p.title = "Remove unused build cache".into();
            p.steps.push(step(
                &["builder", "prune", "--all", "--force"],
                "Unused build cache",
                None,
            ));
            let total: u64 = unused.iter().map(|b| b.size_bytes).sum();
            // Records can share data, so their sizes overlap; prefer Docker's
            // own reclaimable figure when it reports one.
            let reported = o.usage.build_cache.reclaimable_bytes;
            p.reclaim_bytes = Some(if reported > 0 {
                reported.min(total)
            } else {
                total
            });
            if let Some(s) = p.steps.last_mut() {
                s.bytes = p.reclaim_bytes;
            }
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

/// What the person reviewed and confirmed for one action.
#[derive(Debug, Clone)]
pub struct Reviewed {
    pub action: DockerAction,
    /// The exact commands the preview showed.
    pub commands: Vec<Vec<String>>,
    pub confirm_name: Option<String>,
}

impl Reviewed {
    pub fn of(preview: &DockerActionPreview) -> Self {
        Reviewed {
            action: preview.action.clone(),
            commands: preview.steps.iter().map(|s| s.command.clone()).collect(),
            confirm_name: preview.confirm_name.clone(),
        }
    }
}

/// Whether `fresh` still runs exactly what was reviewed, and the person typed
/// the required name. Checked by the engine so neither a stale dialog nor a
/// script in the window can run something nobody confirmed.
pub fn check_confirmed(
    fresh: &DockerActionPreview,
    reviewed: &Reviewed,
    typed: &str,
) -> Result<(), DockerError> {
    if let Some(reason) = &fresh.blocked {
        return Err(DockerError::Blocked(reason.clone()));
    }
    let now: Vec<&Vec<String>> = fresh.steps.iter().map(|s| &s.command).collect();
    let then: Vec<&Vec<String>> = reviewed.commands.iter().collect();
    if now != then || fresh.confirm_name != reviewed.confirm_name {
        return Err(DockerError::Blocked(
            "Docker changed since you reviewed this. Review it again.".into(),
        ));
    }
    if let Some(name) = &reviewed.confirm_name {
        if typed.trim() != name {
            return Err(DockerError::Blocked(format!("Type {name} to confirm.")));
        }
    }
    Ok(())
}

/// Execute what the person reviewed, after re-validating against fresh state.
pub fn execute(
    cli: &DockerCli,
    reviewed: &Reviewed,
    typed: &str,
    fresh: &DockerOverview,
) -> Result<DockerActionResult, DockerError> {
    let action = &reviewed.action;
    let preview = preview(action, fresh)?;
    check_confirmed(&preview, reviewed, typed)?;
    // A --rm container is deleted by Docker itself once stopped.
    let auto_removed = match action {
        DockerAction::RemoveContainer { id, .. } => fresh
            .containers
            .iter()
            .any(|c| &c.id == id && c.auto_remove),
        _ => false,
    };
    let started = now_ms();
    let mut items = Vec::new();
    let mut outputs = Vec::new();
    let mut error = None;
    for step in &preview.steps {
        let line = step.command.join(" ");
        if error.is_some() {
            items.push(CleanupItemResult {
                target: step.target.clone(),
                operation: line,
                bytes: 0,
                status: ItemStatus::Skipped,
                error: Some("Not run because an earlier step failed.".into()),
            });
            continue;
        }
        let args: Vec<&str> = step.command.iter().skip(1).map(String::as_str).collect();
        match cli.run(&args, step_timeout(&step.command)) {
            Ok(out) => {
                let out = out.trim();
                if !out.is_empty() {
                    outputs.push(out.to_string());
                }
                items.push(CleanupItemResult {
                    target: step.target.clone(),
                    operation: line,
                    bytes: step.bytes.unwrap_or(0),
                    status: ItemStatus::Succeeded,
                    error: None,
                });
            }
            Err(e) if auto_removed && is_rm(&step.command) && already_removed(&e.to_string()) => {
                // Docker removed it when it stopped; wait until it is gone so
                // the volumes it used are free to delete.
                wait_until_gone(cli, &step.command[2]);
                items.push(CleanupItemResult {
                    target: step.target.clone(),
                    operation: line,
                    bytes: step.bytes.unwrap_or(0),
                    status: ItemStatus::Succeeded,
                    error: None,
                });
            }
            Err(e) => {
                let message = failure_message(&step.command, &step.target, &e);
                items.push(CleanupItemResult {
                    target: step.target.clone(),
                    operation: line,
                    bytes: 0,
                    status: ItemStatus::Failed,
                    error: Some(message.clone()),
                });
                error = Some(message);
            }
        }
    }
    let succeeded = error.is_none();
    let output = outputs.join("\n");

    let record = action.is_destructive().then(|| {
        let bytes = items
            .iter()
            .filter(|i| i.status == ItemStatus::Succeeded)
            .map(|i| i.bytes)
            .sum();
        CleanupRecord {
            id: format!("docker-{started}"),
            kind: "docker".into(),
            title: preview.title.clone(),
            started_at_ms: started,
            finished_at_ms: now_ms(),
            status: removal_status(&items),
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

/// How long one step may take. `docker stop` waits for each container's own
/// shutdown period, which some set to minutes, so it gets far longer.
fn step_timeout(command: &[String]) -> Duration {
    if command.get(1).is_some_and(|c| c == "stop") {
        Duration::from_secs(15 * 60)
    } else {
        Duration::from_secs(180)
    }
}

/// A step's failure in plain words. A stop that runs out of time has not
/// failed: Docker keeps stopping the container on its own.
fn failure_message(command: &[String], target: &str, e: &DockerError) -> String {
    match e {
        DockerError::Timeout(_) if command.get(1).is_some_and(|c| c == "stop") => format!(
            "Docker is still stopping {target}; it is taking longer than usual to shut down. Nothing else was run. Check Docker in a few minutes, then try again."
        ),
        _ => e.to_string(),
    }
}

fn is_rm(command: &[String]) -> bool {
    command.get(1).is_some_and(|c| c == "rm")
}

/// Docker's answer to `rm` for a container it is removing or has removed.
fn already_removed(message: &str) -> bool {
    message.contains("No such container") || message.contains("is already in progress")
}

fn wait_until_gone(cli: &DockerCli, id: &str) {
    for _ in 0..20 {
        let args = ["inspect", "--type", "container", "--format", "{{.Id}}", id];
        if cli.run(&args, Duration::from_secs(10)).is_err() {
            return;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// The action's status from the steps that remove something: a stop that
/// worked before a failed removal is not a partial cleanup.
fn removal_status(items: &[CleanupItemResult]) -> ActionStatus {
    let removing: Vec<CleanupItemResult> = items
        .iter()
        .filter(|i| !i.operation.starts_with("docker stop "))
        .cloned()
        .collect();
    if removing.is_empty() {
        ActionStatus::from_items(items)
    } else {
        ActionStatus::from_items(&removing)
    }
}

pub fn inspect(cli: &DockerCli, kind: &str, id: &str) -> Result<String, DockerError> {
    let id = match kind {
        "container" | "image" => validate_id(id)?,
        "volume" => validate_volume_name(id)?,
        _ => return Err(DockerError::InvalidId(kind.chars().take(20).collect())),
    };
    let out = cli.run(&["inspect", "--type", kind, id], Duration::from_secs(20))?;
    // Re-indent for display. If it cannot be parsed, show nothing rather than
    // raw text that could carry unmasked environment values.
    let mut value: serde_json::Value =
        serde_json::from_str(&out).map_err(|e| DockerError::Parse(e.to_string()))?;
    mask_environment(&mut value);
    serde_json::to_string_pretty(&value).map_err(|e| DockerError::Parse(e.to_string()))
}

/// Hide the values of `Config.Env` entries. Containers often get database
/// passwords and API keys this way, and the details panel can be shared in a
/// screenshot. Variable names stay visible.
fn mask_environment(value: &mut serde_json::Value) {
    let objects: Vec<&mut serde_json::Value> = match value {
        serde_json::Value::Array(items) => items.iter_mut().collect(),
        other => vec![other],
    };
    for obj in objects {
        let Some(env) = obj
            .pointer_mut("/Config/Env")
            .and_then(serde_json::Value::as_array_mut)
        else {
            continue;
        };
        for entry in env {
            if let Some((name, val)) = entry.as_str().and_then(|e| e.split_once('=')) {
                if !val.is_empty() {
                    *entry = serde_json::Value::String(format!("{name}=(hidden)"));
                }
            }
        }
    }
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
            "Volumes often hold databases. DiskPilot never removes volumes on its own. If you are sure one is unneeded, back it up, then delete it from the Volumes list, where you confirm by typing its name.",
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

    #[test]
    fn inspect_output_hides_environment_values() {
        let mut v = serde_json::json!([{
            "Id": "abc",
            "Config": {"Env": ["POSTGRES_PASSWORD=hunter2", "PATH=/usr/bin", "EMPTY="]}
        }]);
        mask_environment(&mut v);
        assert_eq!(
            v[0]["Config"]["Env"],
            serde_json::json!(["POSTGRES_PASSWORD=(hidden)", "PATH=(hidden)", "EMPTY="])
        );
    }

    use super::*;
    use crate::docker::tests::sample;

    const WEB: &str = "f608a28855b6cf35511b2f2f441f313d65bce8fb897cd60324557f78665d3d19";
    const DB: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90112233445566778899aabbccddeeff00";
    const PG_IMAGE: &str =
        "sha256:0cefaabd882e42bcb2a2f3dc56d3ce4770560ba62fd7f197f1a2e008fd5731ab";
    const DANGLING: &str =
        "sha256:ef60141d283bb798a5e9b4820aeda6a8995f25244ec5359b0d76dd849de69db8";

    fn remove(id: &str, stop_first: bool, volumes: &[&str]) -> DockerAction {
        DockerAction::RemoveContainer {
            id: id.into(),
            stop_first,
            volumes: volumes.iter().map(|v| v.to_string()).collect(),
        }
    }

    fn commands(p: &DockerActionPreview) -> Vec<Vec<&str>> {
        p.steps
            .iter()
            .map(|s| s.command.iter().map(String::as_str).collect())
            .collect()
    }

    #[test]
    fn a_running_container_can_be_stopped_then_removed() {
        let p = preview(&remove(WEB, true, &[]), &sample()).unwrap();
        assert!(p.blocked.is_none());
        assert_eq!(
            commands(&p),
            vec![vec!["docker", "stop", WEB], vec!["docker", "rm", WEB]]
        );
    }

    #[test]
    fn stopping_a_rm_container_with_anonymous_volumes_is_refused() {
        let mut o = sample();
        let web = o.containers.iter_mut().find(|c| c.id == WEB).unwrap();
        web.auto_remove = true;
        web.mounts.push("9".repeat(64));
        let stop = preview(&DockerAction::StopContainer { id: WEB.into() }, &o).unwrap();
        assert!(stop.blocked.as_deref().unwrap().contains("--rm"));
        assert!(stop
            .effects
            .iter()
            .all(|e| !e.contains("volumes are kept, and")));
        let rm = preview(&remove(WEB, true, &[]), &o).unwrap();
        assert!(rm.blocked.as_deref().unwrap().contains("anonymous volume"));
    }

    #[test]
    fn stopping_a_rm_container_says_docker_deletes_it() {
        let mut o = sample();
        o.containers
            .iter_mut()
            .find(|c| c.id == WEB)
            .unwrap()
            .auto_remove = true;
        let stop = preview(&DockerAction::StopContainer { id: WEB.into() }, &o).unwrap();
        assert!(stop.blocked.is_none());
        assert!(stop.effects[0].contains("deletes the container as soon as it stops"));
    }

    #[test]
    fn restarting_and_paused_containers_are_stopped_before_removal() {
        for state in ["restarting", "paused"] {
            let df = crate::docker::tests::DF_VERBOSE
                .replace("\"State\":\"running\"", &format!("\"State\":\"{state}\""));
            let o = crate::docker::build_overview(
                crate::docker::tests::running_status(),
                &df,
                crate::docker::tests::DF_SUMMARY,
            )
            .unwrap();
            let web = o.containers.iter().find(|c| c.id == WEB).unwrap();
            assert!(web.running, "{state}");
            let p = preview(&remove(WEB, true, &[]), &o).unwrap();
            assert_eq!(commands(&p)[0], vec!["docker", "stop", WEB], "{state}");
            let stop = preview(&DockerAction::StopContainer { id: WEB.into() }, &o).unwrap();
            assert!(stop.blocked.is_none(), "{state}");
        }
    }

    #[test]
    fn volume_deletes_need_the_typed_name_and_the_reviewed_commands() {
        let o = sample();
        let shown = preview(&remove(DB, false, &["ledger_pg"]), &o).unwrap();
        assert_eq!(
            shown.confirm_name.as_deref(),
            Some(shown.affected[0].label.as_str())
        );
        let reviewed = Reviewed::of(&shown);
        let name = shown.confirm_name.clone().unwrap();

        let fresh = preview(&reviewed.action, &o).unwrap();
        assert!(check_confirmed(&fresh, &reviewed, &name).is_ok());
        assert!(check_confirmed(&fresh, &reviewed, &format!("  {name} ")).is_ok());
        assert!(check_confirmed(&fresh, &reviewed, "").is_err());
        assert!(check_confirmed(&fresh, &reviewed, "something else").is_err());

        // Docker changed after the review: the same action now runs more.
        let mut changed = fresh.clone();
        changed.steps.insert(0, step(&["stop", DB], "x", None));
        let err = check_confirmed(&changed, &reviewed, &name).unwrap_err();
        assert!(err.to_string().contains("changed since you reviewed"));

        // A plain removal without volumes needs no typed name.
        let plain = preview(&remove(DB, false, &[]), &o).unwrap();
        assert_eq!(plain.confirm_name, None);
        assert!(check_confirmed(&plain, &Reviewed::of(&plain), "").is_ok());

        let vol = preview(
            &DockerAction::RemoveVolume {
                name: "orphan_data".into(),
            },
            &o,
        )
        .unwrap();
        assert!(vol.blocked.is_none());
        assert_eq!(vol.confirm_name.as_deref(), Some("orphan_data"));
    }

    #[test]
    fn a_slow_stop_gets_time_and_a_plain_message() {
        let cmd = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let stop = cmd(&["docker", "stop", WEB]);
        let rm = cmd(&["docker", "rm", WEB]);
        assert_eq!(step_timeout(&stop), Duration::from_secs(900));
        assert_eq!(step_timeout(&rm), Duration::from_secs(180));
        let slow = failure_message(&stop, "web-1", &DockerError::Timeout(900));
        assert!(slow.starts_with("Docker is still stopping web-1"));
        let other = failure_message(&rm, "web-1", &DockerError::Timeout(180));
        assert!(!other.contains("still stopping"));
    }

    #[test]
    fn a_stop_alone_does_not_make_a_failed_removal_partial() {
        let item = |op: &str, ok: bool| CleanupItemResult {
            target: "t".into(),
            operation: op.into(),
            bytes: 0,
            status: if ok {
                ItemStatus::Succeeded
            } else {
                ItemStatus::Failed
            },
            error: None,
        };
        let failed = [item("docker stop x", true), item("docker rm x", false)];
        assert_eq!(removal_status(&failed), ActionStatus::Failed);
        let partial = [
            item("docker stop x", true),
            item("docker rm x", true),
            item("docker volume rm v", false),
        ];
        assert_eq!(removal_status(&partial), ActionStatus::Partial);
        assert_eq!(
            removal_status(&[item("docker rm x", true)]),
            ActionStatus::Succeeded
        );
    }

    #[test]
    fn ticked_volumes_are_deleted_after_the_container() {
        let p = preview(&remove(DB, false, &["ledger_pg"]), &sample()).unwrap();
        assert!(p.blocked.is_none());
        assert_eq!(
            commands(&p),
            vec![
                vec!["docker", "rm", DB],
                vec!["docker", "volume", "rm", "ledger_pg"]
            ]
        );
        assert!(p.volume_choices[0].selected);
        assert_eq!(p.reclaim_bytes, Some(63 + 49_480_000));
        assert!(p.effects.iter().any(|e| e.contains("cannot be undone")));
    }

    #[test]
    fn volumes_the_container_does_not_own_alone_cannot_be_ticked() {
        let mut o = sample();
        // Another container mounts ledger_pg too.
        let mut other = o.containers[1].clone();
        other.id = "b".repeat(64);
        other.name = "ledger-backup-1".into();
        o.containers.push(other);
        let p = preview(&remove(DB, false, &["ledger_pg"]), &o).unwrap();
        assert!(p.volume_choices[0]
            .kept_because
            .as_deref()
            .unwrap()
            .contains("ledger-backup-1"));
        assert!(p.blocked.as_deref().unwrap().contains("ledger_pg"));

        // A volume this container does not use, or a bad name, never runs.
        let q = preview(&remove(DB, false, &["orphan_data"]), &sample()).unwrap();
        assert!(q.blocked.is_some());
        assert!(preview(&remove(DB, false, &["-f"]), &sample()).is_err());
    }

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
        let p = preview(&remove(WEB, false, &[]), &sample()).unwrap();
        assert!(p.blocked.is_some());
        assert_eq!(commands(&p), vec![vec!["docker", "rm", WEB]]);
    }

    #[test]
    fn removing_a_stopped_container_keeps_volumes_and_never_forces() {
        let p = preview(&remove(DB, false, &[]), &sample()).unwrap();
        assert!(p.blocked.is_none());
        assert_eq!(commands(&p), vec![vec!["docker", "rm", DB]]);
        assert!(p.effects.iter().any(|e| e.contains("kept: ledger_pg")));
        assert_eq!(p.volume_choices.len(), 1);
        assert!(!p.volume_choices[0].selected);
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
            commands(&p),
            vec![vec!["docker", "builder", "prune", "--all", "--force"]]
        );
    }

    #[test]
    fn stopping_keeps_data_and_only_applies_to_running_containers() {
        let p = preview(&DockerAction::StopContainer { id: WEB.into() }, &sample()).unwrap();
        assert!(p.blocked.is_none());
        assert!(!p.destructive);
        assert_eq!(commands(&p), vec![vec!["docker", "stop", WEB]]);
        let stopped = preview(&DockerAction::StopContainer { id: DB.into() }, &sample()).unwrap();
        assert!(stopped.blocked.is_some());
    }

    #[test]
    fn volumes_used_by_any_container_cannot_be_deleted() {
        // ledger_pg belongs to a stopped container: still blocked.
        let p = preview(
            &DockerAction::RemoveVolume {
                name: "ledger_pg".into(),
            },
            &sample(),
        )
        .unwrap();
        assert!(p.blocked.as_deref().unwrap().contains("ledger-db-1"));
        assert!(p.warnings.iter().any(|w| w.contains("Compose")));
    }

    #[test]
    fn deleting_an_unused_volume_is_one_plain_command() {
        let p = preview(
            &DockerAction::RemoveVolume {
                name: "orphan_data".into(),
            },
            &sample(),
        )
        .unwrap();
        assert!(p.blocked.is_none());
        assert!(p.destructive);
        assert_eq!(
            commands(&p),
            vec![vec!["docker", "volume", "rm", "orphan_data"]]
        );
        assert_eq!(p.reclaim_bytes, Some(98_300_000));
        assert!(p.effects.iter().any(|e| e.contains("cannot be undone")));
        assert!(preview(&DockerAction::RemoveVolume { name: "-f".into() }, &sample()).is_err());
        assert!(preview(
            &DockerAction::RemoveVolume {
                name: "gone".into()
            },
            &sample()
        )
        .is_err());
    }

    #[test]
    fn nothing_runs_when_docker_is_down() {
        let mut o = sample();
        o.status.state = DockerState::NotRunning;
        let p = preview(&remove(DB, false, &[]), &o).unwrap();
        assert!(p.blocked.is_some());
    }

    #[test]
    fn diagnostics_flag_orphan_volumes_without_offering_removal() {
        let f = diagnostics(&sample(), Some((6_000_000_000, 64_000_000_000)));
        let vol = f.iter().find(|d| d.title.contains("volume")).unwrap();
        assert_eq!(vol.level, FindingLevel::Warning);
        assert!(vol.detail.contains("never removes volumes on its own"));
        assert!(f.iter().any(|d| d.title.contains("untagged")));
        assert!(f.iter().any(|d| d.title.contains("build cache")));
    }
}
