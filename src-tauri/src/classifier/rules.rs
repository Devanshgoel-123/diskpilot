//! The classification rule table.
//!
//! Rules are plain data so they can be reviewed, tested and extended without
//! touching the matching logic. Order matters: the first matching rule wins,
//! so more specific rules must come before broader ones.
//!
//! A classification describes *what* something is. It never decides whether
//! something may be deleted; that is the recommendation engine's job, and every
//! action still requires explicit user approval.

use crate::model::{Category, Confidence};

/// How a matched directory affects classification of its descendants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleKind {
    /// A broad area (e.g. `~/Library/Application Support`). More specific rules
    /// can still match inside it.
    Container,
    /// A self-contained item (e.g. a `node_modules` folder). Everything inside
    /// belongs to it; no further rules are evaluated below.
    Leaf,
}

#[derive(Debug)]
pub enum Matcher {
    /// Exact path relative to the user's home directory.
    HomeRelative(&'static [&'static str]),
    /// Any of several exact paths relative to the home directory.
    HomeRelativeAny(&'static [&'static [&'static str]]),
    /// Exact absolute path.
    Absolute(&'static str),
    /// Folder name anywhere.
    Name(&'static [&'static str]),
    /// Folder name, but only when the parent folder contains one of `siblings`
    /// (e.g. `target` next to `Cargo.toml`).
    NameWithSibling {
        names: &'static [&'static str],
        siblings: &'static [&'static str],
    },
    /// A folder that directly contains one of these entries
    /// (e.g. a folder containing `.git`). Never matches the home folder itself.
    Contains(&'static [&'static str]),
}

#[derive(Debug)]
pub struct DirRule {
    pub id: &'static str,
    pub matcher: Matcher,
    pub kind: RuleKind,
    pub category: Category,
    pub label: &'static str,
    pub explanation: &'static str,
    pub confidence: Confidence,
    /// For containers: whether file-extension rules may refine files inside
    /// (e.g. a `.dmg` inside Documents is reported as binary data, but
    /// everything inside Downloads stays "Downloads").
    pub file_rules_apply: bool,
}

#[derive(Debug)]
pub struct FileRule {
    pub id: &'static str,
    pub extensions: &'static [&'static str],
    pub category: Category,
    pub label: &'static str,
    pub explanation: &'static str,
    pub confidence: Confidence,
}

use Category::*;
use Confidence::*;
use Matcher::*;
use RuleKind::*;

macro_rules! leaf {
    ($id:expr, $m:expr, $cat:expr, $conf:expr, $label:expr, $expl:expr) => {
        DirRule {
            id: $id,
            matcher: $m,
            kind: Leaf,
            category: $cat,
            label: $label,
            explanation: $expl,
            confidence: $conf,
            file_rules_apply: false,
        }
    };
}

macro_rules! container {
    ($id:expr, $m:expr, $cat:expr, $conf:expr, $files:expr, $label:expr, $expl:expr) => {
        DirRule {
            id: $id,
            matcher: $m,
            kind: Container,
            category: $cat,
            label: $label,
            explanation: $expl,
            confidence: $conf,
            file_rules_apply: $files,
        }
    };
}

pub static DIR_RULES: &[DirRule] = &[
    // ---- Docker -----------------------------------------------------------
    leaf!(
        "docker.desktop-vm",
        HomeRelative(&["Library", "Containers", "com.docker.docker"]),
        DockerStorage,
        High,
        "Docker Desktop data",
        "Docker Desktop keeps its Linux VM disk here. It holds all images, containers, volumes and build cache. Never delete these files directly: volumes can contain databases and other persistent data. Manage it through Docker instead."
    ),
    leaf!(
        "docker.group-container",
        HomeRelative(&["Library", "Group Containers", "group.com.docker"]),
        DockerStorage,
        High,
        "Docker Desktop shared data",
        "Settings and shared state used by Docker Desktop."
    ),
    leaf!(
        "docker.orbstack",
        HomeRelative(&[".orbstack"]),
        DockerStorage,
        High,
        "OrbStack data",
        "OrbStack's container and VM storage. Manage it through OrbStack or the Docker CLI, not by deleting files."
    ),
    leaf!(
        "docker.colima",
        HomeRelative(&[".colima"]),
        DockerStorage,
        High,
        "Colima VM data",
        "Colima's Linux VM disks, which hold container images and volumes."
    ),
    leaf!(
        "docker.cli-config",
        HomeRelative(&[".docker"]),
        DockerStorage,
        High,
        "Docker CLI configuration",
        "Docker CLI contexts, plugins and credentials configuration. Usually small; required for the Docker CLI to work."
    ),
    leaf!(
        "podman.storage",
        HomeRelative(&[".local", "share", "containers"]),
        DockerStorage,
        High,
        "Podman data",
        "Podman's machine VM disks, images and volumes. Never delete these files directly: volumes can hold databases. Manage it with `podman system prune` or `podman machine rm`."
    ),
    leaf!(
        "minikube.cache",
        HomeRelative(&[".minikube", "cache"]),
        Cache,
        High,
        "Minikube download cache",
        "VM images, container images and Kubernetes binaries downloaded by Minikube. Downloaded again the next time a cluster starts."
    ),
    container!(
        "minikube.home",
        HomeRelative(&[".minikube"]),
        DockerStorage,
        High,
        false,
        "Minikube data",
        "Minikube's clusters, certificates and download cache. Remove clusters you no longer need with `minikube delete`."
    ),
    // ---- Xcode & Apple developer tools --------------------------------------
    leaf!(
        "xcode.derived-data",
        HomeRelative(&["Library", "Developer", "Xcode", "DerivedData"]),
        BuildArtifacts,
        High,
        "Xcode DerivedData",
        "Intermediate build products and indexes created by Xcode. Xcode regenerates them on the next build, which will take longer than usual."
    ),
    leaf!(
        "xcode.archives",
        HomeRelative(&["Library", "Developer", "Xcode", "Archives"]),
        DeveloperTools,
        High,
        "Xcode archives",
        "App archives created for distribution. They contain dSYM debug symbols needed to symbolicate crash reports for shipped builds, so they are not simply regenerable."
    ),
    leaf!(
        "devices.iphone-backups",
        HomeRelative(&["Library", "Application Support", "MobileSync", "Backup"]),
        UserData,
        High,
        "iPhone & iPad backups",
        "Local backups of iPhones and iPads made by Finder (or iTunes), one folder per backup. A backup may be the only copy of a device's photos, messages and app data."
    ),
    leaf!(
        "xcode.device-support",
        HomeRelative(&["Library", "Developer", "Xcode", "iOS DeviceSupport"]),
        DeveloperTools,
        High,
        "iOS device support files",
        "Debug symbols copied from connected iOS devices, one folder per OS version. Xcode copies them again when a device with that version is connected."
    ),
    leaf!(
        "xcode.simulators",
        HomeRelative(&["Library", "Developer", "CoreSimulator"]),
        DeveloperTools,
        High,
        "iOS Simulator devices & runtimes",
        "Simulator devices, their installed apps and data, and simulator runtimes. Simulator data can include test state you may want to keep."
    ),
    container!(
        "apple.developer",
        HomeRelative(&["Library", "Developer"]),
        DeveloperTools,
        Medium,
        false,
        "Apple developer data",
        "Data created by Xcode and Apple developer tools."
    ),
    // ---- Package managers & toolchains ----------------------------------------
    leaf!(
        "npm.cache",
        HomeRelative(&[".npm"]),
        Cache,
        High,
        "npm cache",
        "npm's download cache. npm re-downloads packages when they are needed again; offline installs stop working until it is refilled."
    ),
    leaf!(
        "pnpm.store-library",
        HomeRelative(&["Library", "pnpm"]),
        Cache,
        High,
        "pnpm store",
        "pnpm's content-addressable package store. Projects hard-link packages from here; `pnpm store prune` removes packages no project references."
    ),
    leaf!(
        "pnpm.store-home",
        HomeRelative(&[".pnpm-store"]),
        Cache,
        High,
        "pnpm store",
        "pnpm's content-addressable package store. Projects hard-link packages from here; `pnpm store prune` removes packages no project references."
    ),
    leaf!(
        "pnpm.store-xdg",
        HomeRelative(&[".local", "share", "pnpm"]),
        Cache,
        Medium,
        "pnpm store",
        "pnpm's package store and global installs (XDG location)."
    ),
    leaf!(
        "yarn.cache",
        HomeRelative(&["Library", "Caches", "Yarn"]),
        Cache,
        High,
        "Yarn cache",
        "Yarn's global package cache. Yarn re-downloads packages on the next install."
    ),
    leaf!(
        "yarn.berry-cache",
        HomeRelative(&[".yarn", "berry", "cache"]),
        Cache,
        High,
        "Yarn Berry cache",
        "Yarn 2+ global package cache."
    ),
    leaf!(
        "bun.cache",
        HomeRelative(&[".bun", "install", "cache"]),
        Cache,
        High,
        "Bun install cache",
        "Bun's global package cache."
    ),
    leaf!(
        "nvm.versions",
        HomeRelative(&[".nvm", "versions"]),
        DeveloperTools,
        High,
        "Node.js versions (nvm)",
        "Node.js runtimes installed by nvm. Old versions may still be used by projects pinned through .nvmrc."
    ),
    container!(
        "nvm.home",
        HomeRelative(&[".nvm"]),
        DeveloperTools,
        High,
        false,
        "nvm (Node version manager)",
        "The nvm installation and the Node.js versions it manages."
    ),
    leaf!(
        "cargo.registry",
        HomeRelative(&[".cargo", "registry"]),
        Cache,
        High,
        "Cargo registry cache",
        "Downloaded crate sources and index. Cargo re-downloads them when needed."
    ),
    leaf!(
        "rustup.toolchains",
        HomeRelative(&[".rustup", "toolchains"]),
        DeveloperTools,
        High,
        "Rust toolchains",
        "Rust compiler toolchains installed by rustup. Remove unused ones with `rustup toolchain uninstall`."
    ),
    leaf!(
        "gradle.caches",
        HomeRelative(&[".gradle", "caches"]),
        Cache,
        High,
        "Gradle caches",
        "Downloaded dependencies and build caches used by Gradle."
    ),
    leaf!(
        "maven.repository",
        HomeRelative(&[".m2", "repository"]),
        Cache,
        High,
        "Maven repository",
        "Downloaded Java dependencies. Maven re-downloads them on the next build."
    ),
    container!(
        "cargo.home",
        HomeRelative(&[".cargo"]),
        DeveloperTools,
        High,
        false,
        "Cargo home",
        "Cargo's installed binaries, registry cache and git checkouts."
    ),
    container!(
        "rustup.home",
        HomeRelative(&[".rustup"]),
        DeveloperTools,
        High,
        false,
        "rustup",
        "Rust toolchains and components managed by rustup."
    ),
    container!(
        "gradle.home",
        HomeRelative(&[".gradle"]),
        DeveloperTools,
        High,
        false,
        "Gradle home",
        "Gradle's caches, wrapper distributions and daemon data."
    ),
    container!(
        "android.home",
        HomeRelative(&[".android"]),
        DeveloperTools,
        High,
        false,
        "Android tooling data",
        "Android emulator (AVD) images, debug keystore and adb configuration. The debug keystore signs development builds; keep it if you publish apps signed with it."
    ),
    leaf!(
        "flutter.pub-cache-hosted",
        HomeRelative(&[".pub-cache", "hosted"]),
        Cache,
        High,
        "Dart & Flutter package cache",
        "Packages downloaded by `flutter pub get` / `dart pub get`. Downloaded again the next time a project fetches its packages."
    ),
    container!(
        "flutter.pub-cache",
        HomeRelative(&[".pub-cache"]),
        DeveloperTools,
        High,
        false,
        "Dart & Flutter pub cache",
        "Downloaded packages, git dependencies and globally activated Dart tools (`dart pub global activate`)."
    ),
    leaf!(
        "unreal.shared-ddc",
        HomeRelative(&["Library", "Application Support", "Epic", "UnrealEngine", "Common", "DerivedDataCache"]),
        Cache,
        High,
        "Unreal Engine shared cache",
        "Unreal Engine's shared Derived Data Cache: compiled shaders and cooked assets shared by all projects. Rebuilt when needed, which can take a long time."
    ),
    // ---- AI coding agents ---------------------------------------------------
    leaf!(
        "ai.claude-code-versions",
        HomeRelative(&[".local", "share", "claude"]),
        DeveloperTools,
        Medium,
        "Claude Code installations",
        "Installed versions of the Claude Code command-line tool."
    ),
    container!(
        "ai.claude-code",
        HomeRelative(&[".claude"]),
        DeveloperTools,
        High,
        false,
        "Claude Code data",
        "Claude Code settings, session transcripts and per-project memory. Conversation history lives only here."
    ),
    container!(
        "ai.codex",
        HomeRelative(&[".codex"]),
        DeveloperTools,
        High,
        false,
        "Codex CLI data",
        "OpenAI Codex CLI configuration, sessions and logs."
    ),
    container!(
        "ai.gemini",
        HomeRelative(&[".gemini"]),
        DeveloperTools,
        High,
        false,
        "Gemini CLI data",
        "Gemini CLI configuration and session data."
    ),
    leaf!(
        "xdg.cache",
        HomeRelative(&[".cache"]),
        Cache,
        Medium,
        "CLI tool caches",
        "Cache folder shared by many command-line tools (pip, Hugging Face, pre-commit and others). Some tools keep large downloads here, such as ML models, that are slow to fetch again."
    ),
    // ---- Editors & AI coding tools -----------------------------------------
    leaf!(
        "cursor.caches",
        HomeRelativeAny(&[
            &["Library", "Application Support", "Cursor", "Cache"],
            &["Library", "Application Support", "Cursor", "CachedData"],
            &["Library", "Application Support", "Cursor", "Code Cache"],
            &["Library", "Application Support", "Cursor", "GPUCache"],
            &["Library", "Application Support", "Cursor", "CachedExtensionVSIXs"],
            &["Library", "Application Support", "Cursor", "CachedProfilesData"],
            &["Library", "Application Support", "Cursor", "Service Worker", "CacheStorage"],
        ]),
        Cache,
        High,
        "Cursor caches",
        "Chromium and extension caches used by Cursor's editor shell. Rebuilt automatically; quit Cursor before removing them."
    ),
    leaf!(
        "cursor.logs",
        HomeRelative(&["Library", "Application Support", "Cursor", "logs"]),
        Logs,
        High,
        "Cursor logs",
        "Diagnostic logs written by Cursor, one folder per session."
    ),
    leaf!(
        "cursor.workspace-storage",
        HomeRelative(&["Library", "Application Support", "Cursor", "User", "workspaceStorage"]),
        ApplicationData,
        High,
        "Cursor workspace storage",
        "Per-project state, including agent and chat history for each workspace. Removing it loses that history."
    ),
    leaf!(
        "cursor.global-storage",
        HomeRelative(&["Library", "Application Support", "Cursor", "User", "globalStorage"]),
        ApplicationData,
        High,
        "Cursor global state",
        "Cursor's global state database (state.vscdb) with settings, extension state and chat history. Removing it resets Cursor."
    ),
    leaf!(
        "vscode.caches",
        HomeRelativeAny(&[
            &["Library", "Application Support", "Code", "Cache"],
            &["Library", "Application Support", "Code", "CachedData"],
            &["Library", "Application Support", "Code", "Code Cache"],
            &["Library", "Application Support", "Code", "GPUCache"],
            &["Library", "Application Support", "Code", "CachedExtensionVSIXs"],
            &["Library", "Application Support", "Code", "CachedProfilesData"],
            &["Library", "Application Support", "Code", "Service Worker", "CacheStorage"],
        ]),
        Cache,
        High,
        "VS Code caches",
        "Chromium and extension caches used by VS Code. Rebuilt automatically; quit VS Code before removing them."
    ),
    leaf!(
        "vscode.logs",
        HomeRelative(&["Library", "Application Support", "Code", "logs"]),
        Logs,
        High,
        "VS Code logs",
        "Diagnostic logs written by VS Code, one folder per session."
    ),
    leaf!(
        "vscode.workspace-storage",
        HomeRelative(&["Library", "Application Support", "Code", "User", "workspaceStorage"]),
        ApplicationData,
        High,
        "VS Code workspace storage",
        "Per-project state such as open editors, extension data and chat history."
    ),
    leaf!(
        "windsurf.caches",
        HomeRelativeAny(&[
            &["Library", "Application Support", "Windsurf", "Cache"],
            &["Library", "Application Support", "Windsurf", "CachedData"],
            &["Library", "Application Support", "Windsurf", "Code Cache"],
            &["Library", "Application Support", "Windsurf", "GPUCache"],
            &["Library", "Application Support", "Windsurf", "CachedExtensionVSIXs"],
            &["Library", "Application Support", "Windsurf", "CachedProfilesData"],
            &["Library", "Application Support", "Windsurf", "Service Worker", "CacheStorage"],
        ]),
        Cache,
        High,
        "Windsurf caches",
        "Chromium and extension caches used by Windsurf's editor shell. Rebuilt automatically; quit Windsurf before removing them."
    ),
    leaf!(
        "windsurf.logs",
        HomeRelative(&["Library", "Application Support", "Windsurf", "logs"]),
        Logs,
        High,
        "Windsurf logs",
        "Diagnostic logs written by Windsurf, one folder per session."
    ),
    leaf!(
        "windsurf.workspace-storage",
        HomeRelative(&["Library", "Application Support", "Windsurf", "User", "workspaceStorage"]),
        ApplicationData,
        High,
        "Windsurf workspace storage",
        "Per-project state such as open editors, extension data and chat history."
    ),
    container!(
        "windsurf.app-data",
        HomeRelative(&["Library", "Application Support", "Windsurf"]),
        ApplicationData,
        High,
        false,
        "Windsurf app data",
        "Windsurf's settings, workspace storage, logs and caches."
    ),
    leaf!(
        "windsurf.home",
        HomeRelative(&[".windsurf"]),
        DeveloperTools,
        High,
        "Windsurf extensions",
        "Windsurf's installed extensions and configuration."
    ),
    container!(
        "windsurf.codeium",
        HomeRelative(&[".codeium"]),
        DeveloperTools,
        Medium,
        false,
        "Codeium / Windsurf AI data",
        "Data used by Windsurf's and Codeium's AI features, such as the language server and local indexes."
    ),
    container!(
        "cursor.app-data",
        HomeRelative(&["Library", "Application Support", "Cursor"]),
        ApplicationData,
        High,
        false,
        "Cursor app data",
        "Cursor's settings, workspace storage, chat history and caches. Workspace storage holds per-project state such as agent conversations."
    ),
    leaf!(
        "cursor.home",
        HomeRelative(&[".cursor"]),
        DeveloperTools,
        High,
        "Cursor extensions & agent data",
        "Cursor's installed extensions and agent configuration."
    ),
    container!(
        "vscode.app-data",
        HomeRelative(&["Library", "Application Support", "Code"]),
        ApplicationData,
        High,
        false,
        "VS Code app data",
        "VS Code settings, workspace storage, logs and caches."
    ),
    leaf!(
        "vscode.extensions",
        HomeRelative(&[".vscode", "extensions"]),
        DeveloperTools,
        High,
        "VS Code extensions",
        "Installed VS Code extensions."
    ),
    // ---- Browsers, chat and other apps -----------------------------------------
    leaf!(
        "browser.chromium-caches",
        HomeRelativeAny(&[
            &["Library", "Application Support", "Google", "Chrome", "*", "Service Worker", "CacheStorage"],
            &["Library", "Application Support", "Google", "Chrome", "*", "Code Cache"],
            &["Library", "Application Support", "Google", "Chrome", "*", "GPUCache"],
            &["Library", "Application Support", "BraveSoftware", "Brave-Browser", "*", "Service Worker", "CacheStorage"],
            &["Library", "Application Support", "BraveSoftware", "Brave-Browser", "*", "Code Cache"],
            &["Library", "Application Support", "BraveSoftware", "Brave-Browser", "*", "GPUCache"],
            &["Library", "Application Support", "Microsoft Edge", "*", "Service Worker", "CacheStorage"],
            &["Library", "Application Support", "Microsoft Edge", "*", "Code Cache"],
            &["Library", "Application Support", "Microsoft Edge", "*", "GPUCache"],
            &["Library", "Application Support", "Arc", "User Data", "*", "Service Worker", "CacheStorage"],
            &["Library", "Application Support", "Arc", "User Data", "*", "Code Cache"],
            &["Library", "Application Support", "Arc", "User Data", "*", "GPUCache"],
            &["Library", "Application Support", "Chromium", "*", "Service Worker", "CacheStorage"],
            &["Library", "Application Support", "Chromium", "*", "Code Cache"],
            &["Library", "Application Support", "Chromium", "*", "GPUCache"],
            &["Library", "Application Support", "Vivaldi", "*", "Service Worker", "CacheStorage"],
            &["Library", "Application Support", "Vivaldi", "*", "Code Cache"],
            &["Library", "Application Support", "Vivaldi", "*", "GPUCache"],
            &["Library", "Application Support", "Google", "GoogleUpdater", "crx_cache"],
        ]),
        Cache,
        High,
        "Browser caches",
        "Caches that Chrome, Brave, Edge, Arc, Chromium and Vivaldi keep per profile: offline copies of websites (Service Worker), compiled scripts and graphics. Browsers rebuild them as you browse. Logins, history, bookmarks and site settings are stored elsewhere."
    ),
    leaf!(
        "apps.chromium-caches",
        HomeRelativeAny(&[
            &["Library", "Application Support", "*", "Cache", "Cache_Data"],
            &["Library", "Application Support", "*", "Code Cache"],
            &["Library", "Application Support", "*", "GPUCache"],
            &["Library", "Application Support", "*", "DawnGraphiteCache"],
            &["Library", "Application Support", "*", "DawnWebGPUCache"],
            &["Library", "Application Support", "*", "Service Worker", "CacheStorage"],
        ]),
        Cache,
        Medium,
        "App web caches",
        "Caches that apps built on web technology (Slack, Discord, Notion, Figma and many others) keep for downloaded pages, compiled scripts and graphics. The app rebuilds them when needed."
    ),
    leaf!(
        "chat.whatsapp-media",
        HomeRelative(&["Library", "Group Containers", "group.net.whatsapp.WhatsApp.shared", "Message", "Media"]),
        UserData,
        High,
        "WhatsApp media",
        "Photos, videos, voice notes and documents received or sent in WhatsApp on this Mac, one folder per chat. Some may exist nowhere else, and WhatsApp may not be able to download older media again."
    ),
    leaf!(
        "chat.telegram-media",
        HomeRelativeAny(&[
            &["Library", "Group Containers", "6N38VWS5BX.ru.keepcoder.Telegram", "appstore", "*", "postbox", "media"],
            &["Library", "Group Containers", "6N38VWS5BX.ru.keepcoder.Telegram", "stable", "*", "postbox", "media"],
        ]),
        Cache,
        High,
        "Telegram media cache",
        "Photos, videos and files Telegram downloaded for chats on this Mac. Telegram keeps them in its cloud and downloads them again when you open them."
    ),
    leaf!(
        "playwright.browsers",
        HomeRelative(&["Library", "Caches", "ms-playwright"]),
        DeveloperTools,
        High,
        "Playwright browsers",
        "Browser builds downloaded by Playwright for automated testing, one folder per browser version. `npx playwright install` downloads them again."
    ),
    leaf!(
        "go.module-cache",
        HomeRelative(&["go", "pkg", "mod"]),
        Cache,
        High,
        "Go module cache",
        "Source code of Go modules downloaded for builds. Go downloads modules again when a project needs them."
    ),
    leaf!(
        "asdf.installs",
        HomeRelativeAny(&[&[".asdf", "installs"], &[".local", "share", "mise", "installs"]]),
        DeveloperTools,
        High,
        "Tool versions (asdf, mise)",
        "Language and tool versions installed by asdf or mise, one folder per tool. Projects that pin a version need it installed."
    ),
    leaf!(
        "chromium.http-cache",
        Name(&["Cache_Data"]),
        Cache,
        Medium,
        "Web cache",
        "The download cache of a browser or of an app built on Chromium. It is rebuilt as pages are loaded again."
    ),
    // ---- User library ------------------------------------------------------
    leaf!(
        "trash",
        HomeRelative(&[".Trash"]),
        UserData,
        High,
        "Trash",
        "Items you moved to the Trash. They keep using disk space until the Trash is emptied."
    ),
    container!(
        "library.caches",
        HomeRelative(&["Library", "Caches"]),
        Cache,
        High,
        false,
        "Application caches",
        "Apps store regenerable data here. It is usually rebuilt on demand, but an app may lose offline content or need to download it again."
    ),
    container!(
        "library.logs",
        HomeRelative(&["Library", "Logs"]),
        Logs,
        High,
        false,
        "Application logs",
        "Diagnostic logs written by apps and macOS. Useful when troubleshooting; otherwise rarely needed."
    ),
    leaf!(
        "library.mail",
        HomeRelative(&["Library", "Mail"]),
        UserData,
        High,
        "Mail data",
        "Apple Mail messages and attachments stored on this Mac."
    ),
    leaf!(
        "library.messages",
        HomeRelative(&["Library", "Messages"]),
        UserData,
        High,
        "Messages history",
        "iMessage and SMS history with attachments."
    ),
    leaf!(
        "library.icloud-drive",
        HomeRelative(&["Library", "Mobile Documents"]),
        UserData,
        High,
        "iCloud Drive",
        "Local copies of iCloud Drive files. Deleting files here deletes them from iCloud too."
    ),
    leaf!(
        "library.cloud-storage",
        HomeRelative(&["Library", "CloudStorage"]),
        UserData,
        High,
        "Cloud storage providers",
        "Local copies of files from Dropbox, Google Drive, OneDrive and similar providers. Deleting files here deletes them in the cloud too."
    ),
    container!(
        "library.application-support",
        HomeRelative(&["Library", "Application Support"]),
        ApplicationData,
        Medium,
        false,
        "Application support data",
        "Data apps need to run: databases, settings, downloaded content. Often essential to the app that created it."
    ),
    container!(
        "library.containers",
        HomeRelative(&["Library", "Containers"]),
        ApplicationData,
        Medium,
        false,
        "Sandboxed app data",
        "Private data folders of sandboxed (mostly App Store) apps. Contains each app's documents, settings and caches."
    ),
    container!(
        "library.group-containers",
        HomeRelative(&["Library", "Group Containers"]),
        ApplicationData,
        Medium,
        false,
        "Shared app data",
        "Data shared between related apps and extensions from the same developer."
    ),
    container!(
        "library",
        HomeRelative(&["Library"]),
        ApplicationData,
        Low,
        false,
        "User library",
        "Per-user settings and app data managed by macOS and apps."
    ),
    // ---- User folders --------------------------------------------------------
    container!(
        "home.downloads",
        HomeRelative(&["Downloads"]),
        Downloads,
        High,
        false,
        "Downloads",
        "Files you downloaded. Installers and archives are often no longer needed, but this can be the only copy of a file."
    ),
    container!(
        "home.documents",
        HomeRelative(&["Documents"]),
        UserData,
        High,
        true,
        "Documents",
        "Your documents folder."
    ),
    container!(
        "home.desktop",
        HomeRelative(&["Desktop"]),
        UserData,
        High,
        true,
        "Desktop",
        "Files on your desktop."
    ),
    container!(
        "home.pictures",
        HomeRelative(&["Pictures"]),
        UserData,
        High,
        true,
        "Pictures",
        "Your pictures, including the Photos library."
    ),
    container!(
        "home.movies",
        HomeRelative(&["Movies"]),
        UserData,
        High,
        true,
        "Movies",
        "Your videos and video projects."
    ),
    container!(
        "home.music",
        HomeRelative(&["Music"]),
        UserData,
        High,
        true,
        "Music",
        "Your music library and audio projects."
    ),
    // ---- System locations (only seen when scanning outside home) ------------
    leaf!(
        "system.private-var-folders",
        Absolute("/private/var/folders"),
        Cache,
        Medium,
        "Temporary & cache files",
        "Per-user temporary and cache folders managed by macOS. macOS cleans these itself; do not delete them manually while apps are running."
    ),
    leaf!(
        "system.system",
        Absolute("/System"),
        SystemFiles,
        High,
        "macOS system",
        "The read-only macOS system volume. Never modified by DiskPilot."
    ),
    leaf!(
        "system.usr",
        Absolute("/usr"),
        SystemFiles,
        High,
        "Unix system files",
        "Core Unix programs and libraries."
    ),
    container!(
        "system.private",
        Absolute("/private"),
        SystemFiles,
        High,
        false,
        "System data",
        "System configuration, databases and virtual memory files."
    ),
    leaf!(
        "system.bin",
        Absolute("/bin"),
        SystemFiles,
        High,
        "System binaries",
        "Core system programs."
    ),
    leaf!(
        "system.sbin",
        Absolute("/sbin"),
        SystemFiles,
        High,
        "System binaries",
        "Core system administration programs."
    ),
    leaf!(
        "homebrew.prefix",
        Absolute("/opt/homebrew"),
        DeveloperTools,
        High,
        "Homebrew",
        "Packages installed with Homebrew. Use `brew cleanup` to remove old versions and downloads."
    ),
    container!(
        "system.applications",
        Absolute("/Applications"),
        ApplicationData,
        High,
        false,
        "Applications",
        "Installed applications."
    ),
    container!(
        "system.library",
        Absolute("/Library"),
        SystemFiles,
        Medium,
        false,
        "System-wide library",
        "Settings, fonts, drivers and app data shared by all users."
    ),
    // ---- Project-level detections (anywhere) ---------------------------------
    leaf!(
        "node.node-modules",
        Name(&["node_modules"]),
        DeveloperTools,
        High,
        "Node.js dependencies",
        "Packages installed by npm, pnpm or Yarn for a project. They can be reinstalled with the project's install command as long as its lockfile is intact; the project will not run until they are."
    ),
    leaf!(
        "node.next-build",
        NameWithSibling {
            names: &[".next"],
            siblings: &["package.json"]
        },
        BuildArtifacts,
        High,
        "Next.js build output",
        "Build output and cache generated by `next build` / `next dev`. Regenerated on the next build."
    ),
    leaf!(
        "node.framework-caches",
        NameWithSibling {
            names: &[".turbo", ".parcel-cache", ".svelte-kit", ".nuxt", ".angular", ".vite"],
            siblings: &["package.json"]
        },
        BuildArtifacts,
        High,
        "JavaScript build cache",
        "Build cache generated by a JavaScript framework or bundler. Regenerated on the next build."
    ),
    leaf!(
        "flutter.dart-tool",
        NameWithSibling {
            names: &[".dart_tool"],
            siblings: &["pubspec.yaml"]
        },
        BuildArtifacts,
        High,
        "Dart tool cache",
        "Package configuration and build cache for a Dart or Flutter project. Recreated by `flutter pub get`."
    ),
    leaf!(
        "flutter.build",
        NameWithSibling {
            names: &["build"],
            siblings: &["pubspec.yaml"]
        },
        BuildArtifacts,
        High,
        "Flutter build output",
        "Compiled app builds for a Flutter project. `flutter clean` removes them; they are rebuilt on the next build."
    ),
    leaf!(
        "unity.library",
        NameWithSibling {
            names: &["Library"],
            siblings: &["ProjectSettings"]
        },
        BuildArtifacts,
        High,
        "Unity project cache",
        "Unity's Library folder: imported assets and caches for this project. Unity rebuilds it by reimporting the project, which can take a long time for big projects."
    ),
    leaf!(
        "unreal.project-ddc",
        NameWithSibling {
            names: &["DerivedDataCache"],
            siblings: &["Config"]
        },
        BuildArtifacts,
        Medium,
        "Unreal project cache",
        "An Unreal Engine project's Derived Data Cache (compiled shaders and cooked assets). Rebuilt when the project next opens, which can take a long time."
    ),
    leaf!(
        "node.dist",
        NameWithSibling {
            names: &["dist", "build", "out"],
            siblings: &["package.json"]
        },
        BuildArtifacts,
        Medium,
        "Project build output",
        "Inferred from the folder name next to a package.json. Usually generated by the build, but some projects commit or hand-edit files here, so check before removing."
    ),
    leaf!(
        "rust.target",
        NameWithSibling {
            names: &["target"],
            siblings: &["Cargo.toml"]
        },
        BuildArtifacts,
        High,
        "Rust build output",
        "Compiled artifacts produced by Cargo. `cargo clean` removes them; they are rebuilt on the next build."
    ),
    leaf!(
        "python.venv",
        NameWithSibling {
            names: &[".venv", "venv"],
            siblings: &["pyproject.toml", "requirements.txt", "setup.py", "Pipfile"]
        },
        DeveloperTools,
        High,
        "Python virtual environment",
        "Installed Python packages for a project. Can be recreated from the project's dependency files."
    ),
    leaf!(
        "python.pycache",
        Name(&["__pycache__"]),
        BuildArtifacts,
        High,
        "Python bytecode cache",
        "Compiled Python bytecode. Python regenerates it automatically."
    ),
    leaf!(
        "cocoapods.pods",
        NameWithSibling {
            names: &["Pods"],
            siblings: &["Podfile"]
        },
        DeveloperTools,
        High,
        "CocoaPods dependencies",
        "Dependencies installed by `pod install`."
    ),
    leaf!(
        "gradle.project-cache",
        NameWithSibling {
            names: &[".gradle"],
            siblings: &["build.gradle", "build.gradle.kts", "settings.gradle", "settings.gradle.kts"]
        },
        BuildArtifacts,
        High,
        "Gradle project cache",
        "Per-project Gradle cache, regenerated on the next build."
    ),
    leaf!(
        "xcode.local-derived-data",
        Name(&["DerivedData"]),
        BuildArtifacts,
        Medium,
        "Xcode DerivedData",
        "A DerivedData folder, usually created by Xcode or xcodebuild with a custom build location."
    ),
    leaf!(
        "git.repository-data",
        Name(&[".git"]),
        DeveloperTools,
        High,
        "Git repository data",
        "The full version history of a repository. Unpushed branches, stashes and commits exist only here."
    ),
    container!(
        "project.code",
        Contains(&[".git"]),
        UserData,
        Medium,
        true,
        "Code project",
        "A folder under version control (contains .git). Project sources are user data."
    ),
];

pub static FILE_RULES: &[FileRule] = &[
    FileRule {
        id: "file.log",
        extensions: &["log"],
        category: Logs,
        label: "Log file",
        explanation: "A log file, identified by its extension.",
        confidence: Medium,
    },
    FileRule {
        id: "file.disk-image",
        extensions: &["dmg", "iso", "pkg", "mpkg", "xip"],
        category: BinaryData,
        label: "Installer or disk image",
        explanation: "Installers and disk images are often kept after installing, but may be the only copy of what they contain.",
        confidence: High,
    },
    FileRule {
        id: "file.archive",
        extensions: &["zip", "tar", "gz", "tgz", "bz2", "xz", "zst", "7z", "rar"],
        category: BinaryData,
        label: "Archive",
        explanation: "A compressed archive. It may be a backup or the only copy of its contents.",
        confidence: High,
    },
    FileRule {
        id: "file.vm-disk",
        extensions: &["vmdk", "qcow2", "vdi", "vhd", "vhdx", "utm", "img"],
        category: BinaryData,
        label: "Virtual disk image",
        explanation: "A virtual machine disk. It contains a whole operating system and its data.",
        confidence: Medium,
    },
    FileRule {
        id: "file.ml-model",
        extensions: &["safetensors", "gguf", "ckpt", "pt", "pth", "onnx", "mlmodel", "h5", "tflite"],
        category: BinaryData,
        label: "ML model weights",
        explanation: "Machine-learning model weights. Often large and slow to download or retrain.",
        confidence: High,
    },
    FileRule {
        id: "file.dataset",
        extensions: &["parquet", "arrow", "feather", "npy", "npz", "hdf5", "csv", "tsv", "jsonl"],
        category: BinaryData,
        label: "Dataset",
        explanation: "A data file. DiskPilot cannot tell whether it can be regenerated.",
        confidence: Medium,
    },
    FileRule {
        id: "file.media",
        extensions: &[
            "mov", "mp4", "m4v", "mkv", "avi", "webm", "jpg", "jpeg", "png", "heic", "gif", "tiff",
            "raw", "cr2", "nef", "arw", "dng", "psd", "mp3", "m4a", "wav", "aiff", "flac",
        ],
        category: UserData,
        label: "Media file",
        explanation: "A photo, video or audio file. Likely personal content.",
        confidence: Medium,
    },
    FileRule {
        id: "file.document",
        extensions: &["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "key", "pages", "numbers"],
        category: UserData,
        label: "Document",
        explanation: "A document file. Likely personal content.",
        confidence: Medium,
    },
];
