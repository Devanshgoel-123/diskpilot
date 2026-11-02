//! Docker integration via the Docker CLI.
//!
//! Read operations build a [`DockerOverview`]. Every change goes through
//! `actions`: a fixed set of predefined, parameterized operations that are
//! previewed first and re-validated right before they run. DiskPilot never
//! removes volumes and never runs a broad prune without listing what it
//! affects.

pub mod actions;
pub mod cli;
pub mod parse;

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use cli::{DockerCli, DockerError};
use parse::{label, parse_count, parse_size, parse_timestamp, truthy, RawDf, RawDfSummaryLine};

const COMPOSE_PROJECT: &str = "com.docker.compose.project";
const COMPOSE_IMAGE: &str = "com.docker.compose.image";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DockerState {
    NotInstalled,
    NotRunning,
    Running,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerStatus {
    pub state: DockerState,
    pub cli_path: Option<String>,
    pub client_version: Option<String>,
    pub server_version: Option<String>,
    /// e.g. "Docker Desktop 4.43.2".
    pub platform: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRow {
    pub total_count: u64,
    pub active: u64,
    pub size_bytes: u64,
    pub reclaimable_bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerUsage {
    pub images: UsageRow,
    pub containers: UsageRow,
    pub volumes: UsageRow,
    pub build_cache: UsageRow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerImage {
    pub id: String,
    pub short_id: String,
    pub repository: String,
    pub tag: String,
    pub created_at_ms: Option<i64>,
    pub size_bytes: u64,
    /// Layers shared with other images (not freed by removing this one).
    pub shared_bytes: u64,
    /// Roughly what removing this image frees.
    pub unique_bytes: u64,
    /// Names of containers (any state) created from this image.
    pub used_by: Vec<String>,
    /// Untagged image no longer referenced by a tag.
    pub dangling: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerContainer {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub image: String,
    pub image_id: Option<String>,
    pub state: String,
    pub status: String,
    pub running: bool,
    pub created_at_ms: Option<i64>,
    /// Size of the container's writable layer.
    pub size_bytes: u64,
    pub mounts: Vec<String>,
    pub compose_project: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerVolume {
    pub name: String,
    pub driver: String,
    pub size_bytes: Option<u64>,
    /// Number of containers referencing the volume, if Docker reports it.
    pub links: Option<u64>,
    pub used_by: Vec<String>,
    pub compose_project: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildCacheEntry {
    pub id: String,
    pub cache_type: String,
    pub description: String,
    pub size_bytes: u64,
    pub created: String,
    pub last_used: String,
    pub in_use: bool,
    pub shared: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerOverview {
    pub status: DockerStatus,
    pub usage: DockerUsage,
    pub images: Vec<DockerImage>,
    pub containers: Vec<DockerContainer>,
    pub volumes: Vec<DockerVolume>,
    pub build_cache: Vec<BuildCacheEntry>,
}

pub fn short_id(id: &str) -> String {
    id.trim_start_matches("sha256:").chars().take(12).collect()
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct RawVersion {
    client: Option<RawComponentVersion>,
    server: Option<RawServerVersion>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct RawComponentVersion {
    version: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct RawServerVersion {
    version: String,
    platform: Option<RawPlatform>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct RawPlatform {
    name: String,
}

/// Detect whether Docker is installed and its engine reachable.
pub fn status(cli: Option<&DockerCli>) -> DockerStatus {
    let Some(cli) = cli else {
        return DockerStatus {
            state: DockerState::NotInstalled,
            cli_path: None,
            client_version: None,
            server_version: None,
            platform: None,
            message: Some(DockerError::NotInstalled.to_string()),
        };
    };
    let cli_path = Some(cli.path().display().to_string());
    match cli.run(
        &["version", "--format", "{{json .}}"],
        Duration::from_secs(10),
    ) {
        Ok(out) => {
            let v: RawVersion = serde_json::from_str(out.trim()).unwrap_or_default();
            let server = v.server.filter(|s| !s.version.is_empty());
            DockerStatus {
                state: if server.is_some() {
                    DockerState::Running
                } else {
                    DockerState::NotRunning
                },
                cli_path,
                client_version: v.client.map(|c| c.version),
                server_version: server.as_ref().map(|s| s.version.clone()),
                platform: server
                    .and_then(|s| s.platform)
                    .map(|p| p.name)
                    .filter(|n| !n.is_empty()),
                message: None,
            }
        }
        Err(e) => DockerStatus {
            state: DockerState::NotRunning,
            cli_path,
            client_version: None,
            server_version: None,
            platform: None,
            message: Some(e.to_string()),
        },
    }
}

/// Collect everything the Docker Manager shows. Read-only.
pub fn overview(cli: &DockerCli, status: DockerStatus) -> Result<DockerOverview, DockerError> {
    let df = cli.run(
        &["system", "df", "-v", "--format", "{{json .}}"],
        Duration::from_secs(90),
    )?;
    let summary = cli.run(
        &["system", "df", "--format", "{{json .}}"],
        Duration::from_secs(60),
    )?;
    build_overview(status, &df, &summary)
}

/// Pure assembly from raw CLI output (tested without Docker).
pub fn build_overview(
    status: DockerStatus,
    df_verbose: &str,
    df_summary: &str,
) -> Result<DockerOverview, DockerError> {
    let raw: RawDf =
        serde_json::from_str(df_verbose.trim()).map_err(|e| DockerError::Parse(e.to_string()))?;

    let containers: Vec<DockerContainer> = raw
        .containers
        .unwrap_or_default()
        .into_iter()
        .map(|c| DockerContainer {
            short_id: short_id(&c.id),
            name: c.names.split(',').next().unwrap_or("").to_string(),
            image_id: label(&c.labels, COMPOSE_IMAGE).map(str::to_string),
            compose_project: label(&c.labels, COMPOSE_PROJECT).map(str::to_string),
            running: c.state == "running",
            created_at_ms: parse_timestamp(&c.created_at),
            size_bytes: parse_size(&c.size).unwrap_or(0),
            mounts: c
                .mounts
                .split(',')
                .map(str::trim)
                .filter(|m| !m.is_empty())
                .map(str::to_string)
                .collect(),
            id: c.id,
            image: c.image,
            state: c.state,
            status: c.status,
        })
        .collect();

    let images: Vec<DockerImage> = raw
        .images
        .unwrap_or_default()
        .into_iter()
        .map(|i| {
            let used_by = containers
                .iter()
                .filter(|c| container_uses_image(c, &i.id, &i.repository, &i.tag))
                .map(|c| c.name.clone())
                .collect();
            DockerImage {
                short_id: short_id(&i.id),
                created_at_ms: parse_timestamp(&i.created_at),
                size_bytes: parse_size(&i.size).unwrap_or(0),
                shared_bytes: parse_size(&i.shared_size).unwrap_or(0),
                unique_bytes: parse_size(&i.unique_size).unwrap_or(0),
                dangling: i.repository == "<none>" && i.tag == "<none>",
                used_by,
                id: i.id,
                repository: i.repository,
                tag: i.tag,
            }
        })
        .collect();

    let volumes: Vec<DockerVolume> = raw
        .volumes
        .unwrap_or_default()
        .into_iter()
        .map(|v| DockerVolume {
            used_by: containers
                .iter()
                .filter(|c| c.mounts.iter().any(|m| m == &v.name))
                .map(|c| c.name.clone())
                .collect(),
            size_bytes: parse_size(&v.size),
            links: parse_count(&v.links),
            compose_project: label(&v.labels, COMPOSE_PROJECT).map(str::to_string),
            name: v.name,
            driver: v.driver,
        })
        .collect();

    let build_cache: Vec<BuildCacheEntry> = raw
        .build_cache
        .unwrap_or_default()
        .into_iter()
        .map(|b| BuildCacheEntry {
            size_bytes: parse_size(&b.size).unwrap_or(0),
            in_use: truthy(&b.in_use),
            shared: truthy(&b.shared),
            id: b.id,
            cache_type: b.cache_type,
            description: b.description,
            created: b.created_since,
            last_used: b.last_used_since,
        })
        .collect();

    let mut usage = DockerUsage::default();
    for line in df_summary.lines().filter(|l| !l.trim().is_empty()) {
        let row: RawDfSummaryLine =
            serde_json::from_str(line).map_err(|e| DockerError::Parse(e.to_string()))?;
        let parsed = UsageRow {
            total_count: parse_count(&row.total_count).unwrap_or(0),
            active: parse_count(&row.active).unwrap_or(0),
            size_bytes: parse_size(&row.size).unwrap_or(0),
            reclaimable_bytes: parse_size(&row.reclaimable).unwrap_or(0),
        };
        match row.kind.as_str() {
            "Images" => usage.images = parsed,
            "Containers" => usage.containers = parsed,
            "Local Volumes" => usage.volumes = parsed,
            "Build Cache" => usage.build_cache = parsed,
            _ => {}
        }
    }

    let mut images = images;
    images.sort_by(|a, b| b.unique_bytes.cmp(&a.unique_bytes));
    let mut containers = containers;
    containers.sort_by(|a, b| b.running.cmp(&a.running).then(a.name.cmp(&b.name)));

    Ok(DockerOverview {
        status,
        usage,
        images,
        containers,
        volumes,
        build_cache,
    })
}

/// Whether container `c` was created from the image (id / repo / tag).
pub fn container_uses_image(c: &DockerContainer, id: &str, repo: &str, tag: &str) -> bool {
    if let Some(image_id) = &c.image_id {
        return image_id == id;
    }
    let image = c.image.as_str();
    let by_id = image == id
        || (image.len() >= 12
            && id
                .trim_start_matches("sha256:")
                .starts_with(image.trim_start_matches("sha256:")));
    let by_name = repo != "<none>"
        && (image == format!("{repo}:{tag}") || (tag == "latest" && image == repo));
    by_id || by_name
}

/// Index of names → containers, used by actions to explain dependencies.
pub fn containers_by_id(o: &DockerOverview) -> HashMap<&str, &DockerContainer> {
    o.containers.iter().map(|c| (c.id.as_str(), c)).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const DF_VERBOSE: &str = r#"{"Images":[
      {"Containers":"1","CreatedAt":"2026-11-02 16:26:04 +0530 IST","ID":"sha256:5cd2022af62e63cfa5df9327bcce1a9606b35d1df32d90fc61d74e079445581f","Repository":"shop-web","SharedSize":"173.1MB","Size":"348MB","Tag":"latest","UniqueSize":"174.8MB"},
      {"Containers":"0","CreatedAt":"2026-11-02 11:48:10 +0530 IST","ID":"sha256:ef60141d283bb798a5e9b4820aeda6a8995f25244ec5359b0d76dd849de69db8","Repository":"<none>","SharedSize":"173.1MB","Size":"348MB","Tag":"<none>","UniqueSize":"174.8MB"},
      {"Containers":"1","CreatedAt":"2026-10-22 04:49:19 +0530 IST","ID":"sha256:0cefaabd882e42bcb2a2f3dc56d3ce4770560ba62fd7f197f1a2e008fd5731ab","Repository":"postgres","SharedSize":"9.31MB","Size":"411MB","Tag":"16-alpine","UniqueSize":"401.4MB"}],
     "Containers":[
      {"CreatedAt":"2026-11-02 16:26:08 +0530 IST","ID":"f608a28855b6cf35511b2f2f441f313d65bce8fb897cd60324557f78665d3d19","Image":"shop-web","Labels":"com.docker.compose.image=sha256:5cd2022af62e63cfa5df9327bcce1a9606b35d1df32d90fc61d74e079445581f,com.docker.compose.project=shop","LocalVolumes":"0","Mounts":"","Names":"shop-web-1","Size":"4.1kB","State":"running","Status":"Up 47 minutes"},
      {"CreatedAt":"2026-10-22 05:00:00 +0530 IST","ID":"a1b2c3d4e5f60718293a4b5c6d7e8f90112233445566778899aabbccddeeff00","Image":"postgres:16-alpine","Labels":"com.docker.compose.project=ledger","LocalVolumes":"1","Mounts":"ledger_pg","Names":"ledger-db-1","Size":"63B","State":"exited","Status":"Exited (0) 2 days ago"}],
     "Volumes":[
      {"Driver":"local","Labels":"com.docker.compose.project=ledger","Links":"1","Name":"ledger_pg","Size":"49.48MB"},
      {"Driver":"local","Labels":"","Links":"0","Name":"orphan_data","Size":"98.3MB"}],
     "BuildCache":[
      {"ID":"k9x","CacheType":"regular","Description":"[web 3/5] RUN npm ci","Size":"120MB","CreatedSince":"3 days ago","LastUsedSince":"3 days ago","UsageCount":2,"InUse":false,"Shared":false},
      {"ID":"z1y","CacheType":"source.local","Description":"local source","Size":"5MB","CreatedSince":"1 hour ago","LastUsedSince":"1 hour ago","UsageCount":1,"InUse":"true","Shared":"false"}]}"#;

    pub(crate) const DF_SUMMARY: &str = r#"{"Active":"2","Reclaimable":"282.8MB (23%)","Size":"1.205GB","TotalCount":"3","Type":"Images"}
{"Active":"1","Reclaimable":"63B (0%)","Size":"114.7kB","TotalCount":"2","Type":"Containers"}
{"Active":"1","Reclaimable":"98.3MB (4%)","Size":"147.8MB","TotalCount":"2","Type":"Local Volumes"}
{"Active":"1","Reclaimable":"120MB","Size":"125MB","TotalCount":"2","Type":"Build Cache"}"#;

    pub(crate) fn running_status() -> DockerStatus {
        DockerStatus {
            state: DockerState::Running,
            cli_path: Some("/usr/local/bin/docker".into()),
            client_version: Some("28.3.2".into()),
            server_version: Some("28.3.2".into()),
            platform: None,
            message: None,
        }
    }

    pub(crate) fn sample() -> DockerOverview {
        build_overview(running_status(), DF_VERBOSE, DF_SUMMARY).unwrap()
    }

    #[test]
    fn builds_overview_with_dependencies() {
        let o = sample();
        assert_eq!(o.usage.images.size_bytes, 1_205_000_000);
        assert_eq!(o.usage.volumes.reclaimable_bytes, 98_300_000);
        assert_eq!(o.containers[0].name, "shop-web-1");
        assert!(o.containers[0].running);
        assert_eq!(o.containers[1].compose_project.as_deref(), Some("ledger"));

        let web = o
            .images
            .iter()
            .find(|i| i.repository == "shop-web")
            .unwrap();
        assert_eq!(web.used_by, vec!["shop-web-1"]);
        let pg = o
            .images
            .iter()
            .find(|i| i.repository == "postgres")
            .unwrap();
        assert_eq!(pg.used_by, vec!["ledger-db-1"]);
        let dangling = o.images.iter().find(|i| i.dangling).unwrap();
        assert!(
            dangling.used_by.is_empty(),
            "compose image label pins the exact id"
        );

        let vol = o.volumes.iter().find(|v| v.name == "ledger_pg").unwrap();
        assert_eq!(vol.used_by, vec!["ledger-db-1"]);
        assert_eq!(o.build_cache.iter().filter(|b| b.in_use).count(), 1);
    }

    #[test]
    fn handles_empty_and_null_sections() {
        let o = build_overview(running_status(), r#"{"Images":null,"BuildCache":[]}"#, "").unwrap();
        assert!(o.images.is_empty() && o.containers.is_empty() && o.volumes.is_empty());
    }

    #[test]
    fn short_ids() {
        assert_eq!(short_id("sha256:5cd2022af62e63cf"), "5cd2022af62e");
    }

    #[test]
    fn missing_cli_means_not_installed() {
        assert_eq!(status(None).state, DockerState::NotInstalled);
    }
}
