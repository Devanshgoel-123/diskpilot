//! The Windows classification rule table.
//!
//! Same contract as [`super::rules`]: plain data, first match wins, and a
//! classification never decides that something may be deleted. Paths are
//! relative to well-known roots (`%LOCALAPPDATA%`, `%ProgramData%` and so on)
//! and are compared ignoring case, as NTFS does. Project-level rules
//! (`node_modules`, `target`, `.venv` and others) are shared with macOS and
//! evaluated after this table.
//!
//! Where a rule reuses a macOS rule id (for example `npm.cache`), the data
//! and the cleanup policy are the same on both systems.

use super::rules::Matcher::*;
use super::rules::RuleKind::*;
use super::rules::{container, leaf, DirRule, FileRule, NamedFileRule, Root};
use crate::model::{Category::*, Confidence::*};
use Root::*;

pub static WINDOWS_DIR_RULES: &[DirRule] = &[
    // ---- System drive: previous installations and upgrade leftovers ---------
    leaf!(
        "windows.old",
        Under(&[(SystemDrive, &["Windows.old"])]),
        SystemFiles,
        High,
        "Previous Windows installation",
        "The Windows installation from before the last upgrade, kept so you can go back to it. Windows removes it by itself after about 10 days. Files from the old user folders that were not carried over are also in here."
    ),
    leaf!(
        "windows.upgrade-files",
        Under(&[
            (SystemDrive, &["$WINDOWS.~BT"]),
            (SystemDrive, &["$WINDOWS.~WS"]),
            (SystemDrive, &["$GetCurrent"]),
            (SystemDrive, &["$WinREAgent"]),
            (SystemDrive, &["ESD"]),
        ]),
        SystemFiles,
        High,
        "Windows upgrade files",
        "Temporary files and logs left by a Windows feature update or the Media Creation Tool. They are only needed while an upgrade is in progress or being rolled back."
    ),
    leaf!(
        "windows.recycle-bin",
        Under(&[(AnyDrive, &["$Recycle.Bin"])]),
        UserData,
        High,
        "Recycle Bin",
        "Items you deleted on this drive. They keep using disk space until the Recycle Bin is emptied."
    ),
    leaf!(
        "windows.system-volume-information",
        Under(&[(AnyDrive, &["System Volume Information"])]),
        SystemFiles,
        High,
        "Restore points and shadow copies",
        "System Restore points, volume shadow copies and indexing data for this drive. Windows limits how much space they use; only an administrator can read the folder."
    ),
    leaf!(
        "windows.recovery",
        Under(&[(SystemDrive, &["Recovery"])]),
        SystemFiles,
        High,
        "Windows recovery files",
        "Files Windows uses to repair or reset this PC. Never modify them."
    ),
    leaf!(
        "gpu.driver-installers",
        Under(&[
            (SystemDrive, &["NVIDIA"]),
            (SystemDrive, &["AMD"]),
            (SystemDrive, &["Intel"]),
        ]),
        Cache,
        Medium,
        "Graphics driver installer files",
        "Files that NVIDIA, AMD and Intel driver installers unpack before installing, sometimes with installation logs. The installed drivers do not use them. Recognized by folder name at the root of the system drive."
    ),
    // ---- Inside C:\Windows -------------------------------------------------
    leaf!(
        "windows.temp",
        Under(&[(SystemRoot, &["Temp"])]),
        Cache,
        Medium,
        "Windows temporary files",
        "Temporary files written by Windows services and installers that run as the system. Files in use are locked; Windows' own cleanup removes the rest."
    ),
    leaf!(
        "windows.update-cache",
        Under(&[(SystemRoot, &["SoftwareDistribution", "Download"])]),
        Cache,
        High,
        "Windows Update downloads",
        "Update packages Windows Update downloaded. Windows removes them after the updates are installed; a large folder usually means an update is still pending or failed."
    ),
    leaf!(
        "windows.delivery-optimization",
        Under(&[
            (SystemRoot, &["ServiceProfiles", "NetworkService", "AppData", "Local", "Microsoft", "Windows", "DeliveryOptimization"]),
            (SystemRoot, &["SoftwareDistribution", "DeliveryOptimization"]),
        ]),
        Cache,
        High,
        "Delivery Optimization cache",
        "Pieces of Windows and Store app updates kept so other PCs can download them from this one. Windows manages the size; it can be cleared at any time."
    ),
    leaf!(
        "windows.memory-dumps",
        Under(&[
            (SystemRoot, &["Minidump"]),
            (SystemRoot, &["LiveKernelReports"]),
        ]),
        Logs,
        High,
        "System crash dumps",
        "Memory dumps Windows wrote when the system or a driver crashed. They help diagnose crashes; Windows does not need them to run."
    ),
    leaf!(
        "windows.component-store",
        Under(&[(SystemRoot, &["WinSxS"])]),
        SystemFiles,
        High,
        "Windows component store (WinSxS)",
        "Every Windows component and the versions kept for updates and repairs. Most files here are hard links to files in System32, so its apparent size overstates what it uses on disk. Only DISM can clean it safely; deleting files breaks Windows."
    ),
    leaf!(
        "windows.installer-cache",
        Under(&[(SystemRoot, &["Installer"])]),
        SystemFiles,
        High,
        "Windows Installer cache",
        "A copy of every installed program's .msi package and patches. Windows needs them to repair, update and uninstall those programs, so deleting them breaks those programs' updates and uninstalls."
    ),
    container!(
        "windows.system",
        Under(&[(SystemRoot, &[])]),
        SystemFiles,
        High,
        false,
        "Windows",
        "The Windows system folder. Never modified by DiskPilot."
    ),
    // ---- ProgramData and Program Files ------------------------------------
    leaf!(
        "windows.error-reports-system",
        Under(&[(ProgramData, &["Microsoft", "Windows", "WER"])]),
        Logs,
        High,
        "Windows error reports (all users)",
        "Crash and problem reports collected by Windows Error Reporting for programs and services, waiting to be sent or already sent to Microsoft."
    ),
    leaf!(
        "visualstudio.package-cache",
        Under(&[(ProgramData, &["Package Cache"])]),
        ApplicationData,
        High,
        "Installer package cache",
        "Installation packages kept by Visual Studio, the .NET and Windows SDKs, Visual C++ runtimes and other programs that use the same installer technology. They are needed to repair, modify and uninstall those programs."
    ),
    leaf!(
        "gpu.shader-cache",
        Under(&[
            (LocalAppData, &["D3DSCache"]),
            (LocalAppData, &["NVIDIA", "DXCache"]),
            (LocalAppData, &["NVIDIA", "GLCache"]),
            (LocalAppData, &["NVIDIA Corporation", "NV_Cache"]),
            (LocalAppData, &["AMD", "DxCache"]),
            (LocalAppData, &["AMD", "DxcCache"]),
            (LocalAppData, &["AMD", "VkCache"]),
            (LocalAppData, &["AMD", "GLCache"]),
            (LocalAppData, &["Intel", "ShaderCache"]),
        ]),
        Cache,
        High,
        "Graphics shader cache",
        "Compiled shaders that DirectX and the graphics driver keep so games and apps start faster. They are rebuilt when needed; games may stutter briefly the first time."
    ),
    leaf!(
        "conda.pkgs",
        Under(&[
            (Home, &["miniconda3", "pkgs"]),
            (Home, &["anaconda3", "pkgs"]),
            (Home, &["miniforge3", "pkgs"]),
            (Home, &[".conda", "pkgs"]),
            (ProgramData, &["miniconda3", "pkgs"]),
            (ProgramData, &["anaconda3", "pkgs"]),
        ]),
        Cache,
        High,
        "Conda package cache",
        "Packages conda downloaded and unpacked. Environments may link to files here, so let conda decide what is unused."
    ),
    container!(
        "windows.program-data",
        Under(&[(ProgramData, &[])]),
        ApplicationData,
        Medium,
        false,
        "ProgramData",
        "Data shared by all users: settings, databases and caches of installed programs and services."
    ),
    // ---- Games ---------------------------------------------------------------
    leaf!(
        "steam.shader-cache",
        Tail(&[&["steamapps", "shadercache"]]),
        Cache,
        High,
        "Steam shader cache",
        "Shaders Steam compiled or downloaded for each game. Rebuilt when the game next runs, which can take a while."
    ),
    leaf!(
        "steam.download-cache",
        Tail(&[&["Steam", "depotcache"], &["Steam", "appcache", "httpcache"]]),
        Cache,
        High,
        "Steam download cache",
        "Steam's cached download manifests and store data. Steam fetches them again when needed."
    ),
    leaf!(
        "steam.partial-downloads",
        Tail(&[&["steamapps", "downloading"], &["steamapps", "temp"]]),
        Cache,
        High,
        "Steam unfinished downloads",
        "Game updates and installs that Steam has not finished. Removing them restarts those downloads."
    ),
    leaf!(
        "steam.web-cache",
        Under(&[(LocalAppData, &["Steam", "htmlcache"])]),
        Cache,
        High,
        "Steam web cache",
        "Cache of the Steam store and community pages shown in the Steam client. Rebuilt as you browse."
    ),
    leaf!(
        "epic.webcache",
        Under(&[(LocalAppData, &["EpicGamesLauncher", "Saved", "webcache*"])]),
        Cache,
        High,
        "Epic Games Launcher web cache",
        "Cache of the store pages shown in the Epic Games Launcher. Rebuilt the next time the launcher starts."
    ),
    container!(
        "windows.program-files",
        Under(&[
            (SystemDrive, &["Program Files"]),
            (SystemDrive, &["Program Files (x86)"]),
        ]),
        ApplicationData,
        High,
        false,
        "Installed programs",
        "Programs installed for all users. Uninstall programs from Settings › Apps instead of deleting their folders."
    ),
    // ---- Per-user Windows data (in %LOCALAPPDATA%) ----------------------------
    leaf!(
        "windows.user-temp",
        Under(&[(Temp, &[]), (LocalAppData, &["Temp"])]),
        Cache,
        Medium,
        "Temporary files",
        "Temporary files programs and installers create while they run. Most are left behind when a program exits; files still in use are locked by the program using them."
    ),
    leaf!(
        "windows.error-reports",
        Under(&[(LocalAppData, &["Microsoft", "Windows", "WER"])]),
        Logs,
        High,
        "Windows error reports",
        "Problem reports Windows Error Reporting collected for your programs (ReportArchive, ReportQueue), waiting to be sent or already sent to Microsoft."
    ),
    leaf!(
        "windows.crash-dumps",
        Under(&[(LocalAppData, &["CrashDumps"])]),
        Logs,
        High,
        "App crash dumps",
        "Memory dumps written when one of your programs crashed. A developer or support team may ask for one; otherwise they are not needed."
    ),
    leaf!(
        "windows.explorer-cache",
        Under(&[(LocalAppData, &["Microsoft", "Windows", "Explorer"])]),
        Cache,
        High,
        "Thumbnail and icon cache",
        "Thumbnails and icons File Explorer keeps in thumbcache and iconcache files. File Explorer keeps them open and rebuilds them when they are cleared."
    ),
    leaf!(
        "windows.inet-cache",
        Under(&[(LocalAppData, &["Microsoft", "Windows", "INetCache"])]),
        Cache,
        High,
        "Temporary Internet Files",
        "Web cache used by Windows components and apps that show web content, such as Office and older Internet Explorer pages. Rebuilt when needed."
    ),
    // ---- Browsers --------------------------------------------------------------
    leaf!(
        "web.offline-site-data",
        Under(&[
            (LocalAppData, &["Google", "Chrome", "User Data", "*", "Service Worker", "CacheStorage"]),
            (LocalAppData, &["Microsoft", "Edge", "User Data", "*", "Service Worker", "CacheStorage"]),
            (LocalAppData, &["BraveSoftware", "Brave-Browser", "User Data", "*", "Service Worker", "CacheStorage"]),
            (LocalAppData, &["Vivaldi", "User Data", "*", "Service Worker", "CacheStorage"]),
            (LocalAppData, &["Chromium", "User Data", "*", "Service Worker", "CacheStorage"]),
            (RoamingAppData, &["Opera Software", "*", "Service Worker", "CacheStorage"]),
            (RoamingAppData, &["*", "Service Worker", "CacheStorage"]),
        ]),
        Cache,
        Medium,
        "Offline site data",
        "What websites and web apps stored for offline use (Service Worker caches) in a browser profile or an app built on web technology. Most is refilled when you go online, but a web app may keep things here that you saved to use offline."
    ),
    leaf!(
        "browser.chromium-caches",
        Under(&[
            (LocalAppData, &["Google", "Chrome", "User Data", "*", "Cache"]),
            (LocalAppData, &["Google", "Chrome", "User Data", "*", "Code Cache"]),
            (LocalAppData, &["Google", "Chrome", "User Data", "*", "GPUCache"]),
            (LocalAppData, &["Google", "Chrome", "User Data", "ShaderCache"]),
            (LocalAppData, &["Google", "Chrome", "User Data", "GrShaderCache"]),
            (LocalAppData, &["Google", "Chrome", "User Data", "GraphiteDawnCache"]),
            (LocalAppData, &["Microsoft", "Edge", "User Data", "*", "Cache"]),
            (LocalAppData, &["Microsoft", "Edge", "User Data", "*", "Code Cache"]),
            (LocalAppData, &["Microsoft", "Edge", "User Data", "*", "GPUCache"]),
            (LocalAppData, &["Microsoft", "Edge", "User Data", "ShaderCache"]),
            (LocalAppData, &["Microsoft", "Edge", "User Data", "GrShaderCache"]),
            (LocalAppData, &["BraveSoftware", "Brave-Browser", "User Data", "*", "Cache"]),
            (LocalAppData, &["BraveSoftware", "Brave-Browser", "User Data", "*", "Code Cache"]),
            (LocalAppData, &["BraveSoftware", "Brave-Browser", "User Data", "*", "GPUCache"]),
            (LocalAppData, &["BraveSoftware", "Brave-Browser", "User Data", "ShaderCache"]),
            (LocalAppData, &["Vivaldi", "User Data", "*", "Cache"]),
            (LocalAppData, &["Vivaldi", "User Data", "*", "Code Cache"]),
            (LocalAppData, &["Vivaldi", "User Data", "*", "GPUCache"]),
            (LocalAppData, &["Chromium", "User Data", "*", "Cache"]),
            (LocalAppData, &["Chromium", "User Data", "*", "Code Cache"]),
            (LocalAppData, &["Chromium", "User Data", "*", "GPUCache"]),
            (LocalAppData, &["Opera Software", "*", "Cache"]),
            (RoamingAppData, &["Opera Software", "*", "Code Cache"]),
            (RoamingAppData, &["Opera Software", "*", "GPUCache"]),
            (LocalAppData, &["Google", "GoogleUpdater", "crx_cache"]),
        ]),
        Cache,
        High,
        "Browser caches",
        "Caches that Chrome, Edge, Brave, Vivaldi, Opera and Chromium keep per profile: downloaded page files, compiled scripts and graphics. Browsers rebuild them as you browse. Logins, history, bookmarks, cookies and passwords are stored elsewhere in the profile."
    ),
    leaf!(
        "browser.firefox-cache",
        Under(&[
            (LocalAppData, &["Mozilla", "Firefox", "Profiles", "*", "cache2"]),
            (LocalAppData, &["Mozilla", "Firefox", "Profiles", "*", "startupCache"]),
            (LocalAppData, &["Mozilla", "Firefox", "Profiles", "*", "thumbnails"]),
            (LocalAppData, &["Mozilla", "Firefox", "Profiles", "*", "jumpListCache"]),
        ]),
        Cache,
        High,
        "Firefox cache",
        "Firefox's web cache and startup cache for a profile. Firefox rebuilds them as you browse. Logins, history, bookmarks and cookies are in the profile under AppData\\Roaming, which DiskPilot does not touch."
    ),
    // ---- Chat, meetings and music ------------------------------------------------
    leaf!(
        "teams.classic-cache",
        Under(&[
            (RoamingAppData, &["Microsoft", "Teams", "Cache"]),
            (RoamingAppData, &["Microsoft", "Teams", "Code Cache"]),
            (RoamingAppData, &["Microsoft", "Teams", "GPUCache"]),
            (RoamingAppData, &["Microsoft", "Teams", "tmp"]),
            (RoamingAppData, &["Microsoft", "Teams", "Service Worker", "CacheStorage"]),
        ]),
        Cache,
        High,
        "Microsoft Teams (classic) cache",
        "Web caches of classic Microsoft Teams. Teams downloads what it needs again; chats and files stay in Microsoft 365."
    ),
    leaf!(
        "teams.cache",
        Under(&[(LocalAppData, &["Packages", "MSTeams_8wekyb3d8bbwe", "LocalCache", "Microsoft", "MSTeams"])]),
        Cache,
        High,
        "Microsoft Teams cache",
        "The cache of the new Microsoft Teams. Microsoft's documented way to fix Teams problems is to clear this folder; Teams then rebuilds it and may ask you to sign in again. Chats and files stay in Microsoft 365."
    ),
    leaf!(
        "zoom.logs",
        Under(&[(RoamingAppData, &["Zoom", "logs"])]),
        Logs,
        High,
        "Zoom logs",
        "Diagnostic logs written by Zoom. Zoom support may ask for them; otherwise they are not needed. Recordings are kept in Documents\\Zoom, not here."
    ),
    leaf!(
        "spotify.cache",
        Under(&[
            (LocalAppData, &["Spotify", "Data"]),
            (LocalAppData, &["Spotify", "Storage"]),
            (LocalAppData, &["Packages", "SpotifyAB.SpotifyMusic_zpdnekdrzrea0", "LocalCache", "Spotify", "Data"]),
        ]),
        Cache,
        High,
        "Spotify cache",
        "Songs and podcasts Spotify cached for playback, and music you downloaded for offline listening. Spotify streams or downloads them again; downloads need Premium."
    ),
    leaf!(
        "chat.telegram-desktop-media",
        Under(&[
            (RoamingAppData, &["Telegram Desktop", "tdata", "user_data*"]),
            (LocalAppData, &["Packages", "TelegramMessengerLLP.TelegramDesktop_t4vj0pshhgkwm", "LocalCache", "Roaming", "Telegram Desktop UWP", "tdata", "user_data*"]),
        ]),
        Cache,
        High,
        "Telegram media cache",
        "Photos, videos and files Telegram Desktop downloaded for chats on this PC. Media from regular chats stays in Telegram's cloud and downloads again when you open it; media from secret chats exists only on your devices."
    ),
    container!(
        "chat.telegram-desktop",
        Under(&[
            (RoamingAppData, &["Telegram Desktop"]),
            (LocalAppData, &["Packages", "TelegramMessengerLLP.TelegramDesktop_t4vj0pshhgkwm"]),
        ]),
        UserData,
        High,
        false,
        "Telegram Desktop",
        "Telegram Desktop's program and account data, including the keys that keep you signed in. Removing it signs you out and loses secret chats."
    ),
    leaf!(
        "chat.whatsapp-desktop",
        Under(&[(LocalAppData, &["Packages", "5319275A.WhatsAppDesktop_cv1g1gvanyjgm"])]),
        UserData,
        High,
        "WhatsApp",
        "WhatsApp's messages database and the photos, videos, voice notes and documents of your chats on this PC. Some may exist nowhere else, and WhatsApp may not be able to download older media again."
    ),
    // ---- Package managers & toolchains ---------------------------------------
    leaf!(
        "npm.cache",
        Under(&[(LocalAppData, &["npm-cache"])]),
        Cache,
        High,
        "npm cache",
        "npm's download cache. npm re-downloads packages when they are needed again; offline installs stop working until it is refilled."
    ),
    leaf!(
        "pnpm.store-home",
        Under(&[
            (LocalAppData, &["pnpm", "store"]),
            (Home, &[".pnpm-store"]),
            (AnyDrive, &[".pnpm-store"]),
        ]),
        Cache,
        High,
        "pnpm store",
        "pnpm's content-addressable package store. Projects hard-link packages from here; `pnpm store prune` removes packages no project references."
    ),
    leaf!(
        "yarn.cache",
        Under(&[(LocalAppData, &["Yarn", "Cache"])]),
        Cache,
        High,
        "Yarn cache",
        "Yarn's global package cache. Yarn re-downloads packages on the next install."
    ),
    leaf!(
        "yarn.berry-cache",
        Under(&[(LocalAppData, &["Yarn", "Berry", "cache"]), (Home, &[".yarn", "berry", "cache"])]),
        Cache,
        High,
        "Yarn Berry cache",
        "Yarn 2+ global package cache."
    ),
    leaf!(
        "bun.cache",
        Under(&[(Home, &[".bun", "install", "cache"])]),
        Cache,
        High,
        "Bun install cache",
        "Bun's global package cache."
    ),
    leaf!(
        "nuget.packages",
        Under(&[(Home, &[".nuget", "packages"])]),
        Cache,
        High,
        "NuGet packages",
        "NuGet's global packages folder: every package version .NET projects on this PC restored. They are restored again on the next build, which needs network access."
    ),
    leaf!(
        "nuget.http-cache",
        Under(&[
            (LocalAppData, &["NuGet", "v3-cache"]),
            (LocalAppData, &["NuGet", "Cache"]),
            (LocalAppData, &["NuGet", "plugins-cache"]),
        ]),
        Cache,
        High,
        "NuGet download cache",
        "Package lists and downloads NuGet cached. Fetched again when needed."
    ),
    leaf!(
        "pip.cache",
        Under(&[(LocalAppData, &["pip", "Cache"])]),
        Cache,
        High,
        "pip cache",
        "Packages and wheels pip downloaded or built. Installed packages do not need them; pip downloads them again when needed."
    ),
    leaf!(
        "uv.cache",
        Under(&[(LocalAppData, &["uv", "cache"])]),
        Cache,
        High,
        "uv cache",
        "Packages uv downloaded and unpacked. Environments already created keep working; uv downloads packages again when needed."
    ),
    leaf!(
        "poetry.virtualenvs",
        Under(&[(LocalAppData, &["pypoetry", "Cache", "virtualenvs"])]),
        DeveloperTools,
        High,
        "Poetry virtual environments",
        "Virtual environments Poetry created for your projects, one folder per project. `poetry install` recreates one, but the project cannot run until then."
    ),
    leaf!(
        "poetry.cache",
        Under(&[
            (LocalAppData, &["pypoetry", "Cache", "cache"]),
            (LocalAppData, &["pypoetry", "Cache", "artifacts"]),
        ]),
        Cache,
        High,
        "Poetry cache",
        "Packages and package metadata Poetry downloaded. Downloaded again when needed."
    ),
    leaf!(
        "cargo.registry",
        Under(&[(Home, &[".cargo", "registry"])]),
        Cache,
        High,
        "Cargo registry cache",
        "Downloaded crate sources and index. Cargo re-downloads them when needed."
    ),
    leaf!(
        "rustup.toolchains",
        Under(&[(Home, &[".rustup", "toolchains"])]),
        DeveloperTools,
        High,
        "Rust toolchains",
        "Rust compiler toolchains installed by rustup. Remove unused ones with `rustup toolchain uninstall`."
    ),
    leaf!(
        "gradle.caches",
        Under(&[(Home, &[".gradle", "caches"])]),
        Cache,
        High,
        "Gradle caches",
        "Downloaded dependencies and build caches used by Gradle."
    ),
    leaf!(
        "maven.repository",
        Under(&[(Home, &[".m2", "repository"])]),
        Cache,
        High,
        "Maven repository",
        "Downloaded Java dependencies. Maven re-downloads them on the next build."
    ),
    leaf!(
        "go.mod-cache",
        Under(&[(Home, &["go", "pkg", "mod"])]),
        Cache,
        High,
        "Go module cache",
        "Source code of Go modules downloaded for builds, and Go toolchains downloaded when a project's go.mod asks for another Go version. Go makes these files read-only so builds cannot change them. `go clean -modcache` removes them; Go downloads modules again on the next build."
    ),
    leaf!(
        "go.build-cache",
        Under(&[(LocalAppData, &["go-build"])]),
        Cache,
        High,
        "Go build cache",
        "Compiled packages and cached test results kept by the go command (GOCACHE). Go rebuilds them on the next build; `go clean -cache` removes them."
    ),
    leaf!(
        "composer.cache",
        Under(&[(LocalAppData, &["Composer"])]),
        Cache,
        High,
        "Composer cache",
        "PHP packages and metadata Composer downloaded. Downloaded again on the next install."
    ),
    leaf!(
        "bundler.cache",
        Under(&[(Home, &[".bundle", "cache"])]),
        Cache,
        High,
        "Bundler cache",
        "Ruby gems Bundler downloaded. Installed gems do not need them; Bundler downloads them again when needed."
    ),
    leaf!(
        "playwright.browsers",
        Under(&[(LocalAppData, &["ms-playwright"])]),
        DeveloperTools,
        High,
        "Playwright browsers",
        "Browser builds downloaded by Playwright for automated testing, one folder per browser version. `npx playwright install` downloads them again."
    ),
    container!(
        "go.home",
        Under(&[(Home, &["go"])]),
        DeveloperTools,
        Medium,
        false,
        "Go workspace (GOPATH)",
        "Go's default GOPATH: programs installed with `go install` in bin, the module cache in pkg\\mod and, for older GOPATH-mode projects, source code in src."
    ),
    container!(
        "cargo.home",
        Under(&[(Home, &[".cargo"])]),
        DeveloperTools,
        High,
        false,
        "Cargo home",
        "Cargo's installed binaries, registry cache and git checkouts."
    ),
    container!(
        "rustup.home",
        Under(&[(Home, &[".rustup"])]),
        DeveloperTools,
        High,
        false,
        "rustup",
        "Rust toolchains and components managed by rustup."
    ),
    container!(
        "gradle.home",
        Under(&[(Home, &[".gradle"])]),
        DeveloperTools,
        High,
        false,
        "Gradle home",
        "Gradle's caches, wrapper distributions and daemon data."
    ),
    // ---- SDKs, IDEs and engines -------------------------------------------------
    leaf!(
        "android.system-images",
        Under(&[(LocalAppData, &["Android", "Sdk", "system-images"])]),
        DeveloperTools,
        High,
        "Android emulator system images",
        "Android system images used to create emulators, one folder per Android version. Android Studio's SDK Manager downloads them again; emulators that use a removed image stop starting."
    ),
    container!(
        "android.sdk",
        Under(&[(LocalAppData, &["Android", "Sdk"])]),
        DeveloperTools,
        High,
        false,
        "Android SDK",
        "The Android SDK: platforms, build tools, emulator and system images. Manage it with Android Studio's SDK Manager."
    ),
    leaf!(
        "android.avds",
        Under(&[(Home, &[".android", "avd"])]),
        DeveloperTools,
        High,
        "Android emulators",
        "Android Virtual Devices: each emulator's disk with the apps, data and snapshots on it. Delete emulators you no longer use in Android Studio's Device Manager."
    ),
    container!(
        "android.home",
        Under(&[(Home, &[".android"])]),
        DeveloperTools,
        High,
        false,
        "Android tooling data",
        "Android emulator (AVD) images, debug keystore and adb configuration. The debug keystore signs development builds; keep it if you publish apps signed with it."
    ),
    leaf!(
        "visualstudio.component-cache",
        Under(&[(LocalAppData, &["Microsoft", "VisualStudio", "*", "ComponentModelCache"])]),
        Cache,
        High,
        "Visual Studio component cache",
        "Visual Studio's cache of its extensions and components (the MEF cache). Visual Studio rebuilds it at the next start; clearing it is a common fix for extension errors."
    ),
    leaf!(
        "jetbrains.caches",
        Under(&[
            (LocalAppData, &["JetBrains", "*", "caches"]),
            (LocalAppData, &["JetBrains", "*", "index"]),
        ]),
        Cache,
        High,
        "JetBrains IDE caches",
        "Caches and indexes of a JetBrains IDE (IntelliJ IDEA, PyCharm, Rider and others), one folder per IDE version. The IDE rebuilds them, which takes a while for large projects. Folders of IDE versions you no longer use stay behind after upgrades."
    ),
    leaf!(
        "unity.cache",
        Under(&[(LocalAppData, &["Unity", "cache"])]),
        Cache,
        High,
        "Unity package cache",
        "Packages the Unity Editor downloaded for projects. Downloaded again when a project needs them."
    ),
    leaf!(
        "unreal.shared-ddc",
        Under(&[(LocalAppData, &["UnrealEngine", "Common", "DerivedDataCache"])]),
        Cache,
        High,
        "Unreal Engine shared cache",
        "Unreal Engine's shared Derived Data Cache: compiled shaders and cooked assets shared by all projects. Rebuilt when needed, which can take a long time."
    ),
    // ---- Docker and AI coding agents -----------------------------------------
    leaf!(
        "docker.desktop-vm",
        Under(&[(LocalAppData, &["Docker", "wsl"])]),
        DockerStorage,
        High,
        "Docker Desktop data",
        "Docker Desktop keeps its WSL 2 virtual disk here. It holds all images, containers, volumes and build cache. Never delete these files directly: volumes can contain databases and other persistent data. Manage it through Docker instead."
    ),
    leaf!(
        "docker.cli-config",
        Under(&[(Home, &[".docker"])]),
        DockerStorage,
        High,
        "Docker CLI configuration",
        "Docker CLI contexts, plugins and credentials configuration. Usually small; required for the Docker CLI to work."
    ),
    leaf!(
        "ai.claude-code-versions",
        Under(&[(Home, &[".local", "share", "claude"])]),
        DeveloperTools,
        High,
        "Claude Code installations",
        "Installed versions of the Claude Code command-line tool."
    ),
    container!(
        "ai.claude-code",
        Under(&[(Home, &[".claude"])]),
        DeveloperTools,
        High,
        false,
        "Claude Code data",
        "Claude Code settings, session transcripts and per-project memory. Conversation history lives only here."
    ),
    container!(
        "ai.codex",
        Under(&[(Home, &[".codex"])]),
        DeveloperTools,
        High,
        false,
        "Codex CLI data",
        "OpenAI Codex CLI configuration, sessions and logs."
    ),
    container!(
        "ai.gemini",
        Under(&[(Home, &[".gemini"])]),
        DeveloperTools,
        High,
        false,
        "Gemini CLI data",
        "Gemini CLI configuration and session data."
    ),
    leaf!(
        "xdg.cache",
        Under(&[(Home, &[".cache"])]),
        Cache,
        Medium,
        "CLI tool caches",
        "Cache folder used by some command-line tools (Hugging Face, pre-commit and others). Some tools keep large downloads here, such as ML models, that are slow to fetch again."
    ),
    // ---- Editors (same layout as on macOS, under %APPDATA%) ---------------------
    leaf!(
        "cursor.caches",
        Under(&[
            (RoamingAppData, &["Cursor", "Cache"]),
            (RoamingAppData, &["Cursor", "CachedData"]),
            (RoamingAppData, &["Cursor", "Code Cache"]),
            (RoamingAppData, &["Cursor", "GPUCache"]),
            (RoamingAppData, &["Cursor", "CachedExtensionVSIXs"]),
            (RoamingAppData, &["Cursor", "CachedProfilesData"]),
            (RoamingAppData, &["Cursor", "Service Worker", "CacheStorage"]),
        ]),
        Cache,
        High,
        "Cursor caches",
        "Chromium and extension caches used by Cursor's editor shell. Rebuilt automatically; quit Cursor before removing them."
    ),
    leaf!(
        "cursor.logs",
        Under(&[(RoamingAppData, &["Cursor", "logs"])]),
        Logs,
        High,
        "Cursor logs",
        "Diagnostic logs written by Cursor, one folder per session."
    ),
    leaf!(
        "cursor.workspace-storage",
        Under(&[(RoamingAppData, &["Cursor", "User", "workspaceStorage"])]),
        ApplicationData,
        High,
        "Cursor workspace storage",
        "Per-project state, including agent and chat history for each workspace. Removing it loses that history."
    ),
    leaf!(
        "cursor.global-storage",
        Under(&[(RoamingAppData, &["Cursor", "User", "globalStorage"])]),
        ApplicationData,
        High,
        "Cursor global state",
        "Cursor's global state database (state.vscdb) with settings, extension state and chat history. Removing it resets Cursor."
    ),
    container!(
        "cursor.app-data",
        Under(&[(RoamingAppData, &["Cursor"])]),
        ApplicationData,
        High,
        false,
        "Cursor app data",
        "Cursor's settings, workspace storage, chat history and caches. Workspace storage holds per-project state such as agent conversations."
    ),
    leaf!(
        "cursor.home",
        Under(&[(Home, &[".cursor"])]),
        DeveloperTools,
        High,
        "Cursor extensions & agent data",
        "Cursor's installed extensions and agent configuration."
    ),
    leaf!(
        "vscode.caches",
        Under(&[
            (RoamingAppData, &["Code", "Cache"]),
            (RoamingAppData, &["Code", "CachedData"]),
            (RoamingAppData, &["Code", "Code Cache"]),
            (RoamingAppData, &["Code", "GPUCache"]),
            (RoamingAppData, &["Code", "CachedExtensionVSIXs"]),
            (RoamingAppData, &["Code", "CachedProfilesData"]),
            (RoamingAppData, &["Code", "Service Worker", "CacheStorage"]),
        ]),
        Cache,
        High,
        "VS Code caches",
        "Chromium and extension caches used by VS Code. Rebuilt automatically; quit VS Code before removing them."
    ),
    leaf!(
        "vscode.logs",
        Under(&[(RoamingAppData, &["Code", "logs"])]),
        Logs,
        High,
        "VS Code logs",
        "Diagnostic logs written by VS Code, one folder per session."
    ),
    leaf!(
        "vscode.workspace-storage",
        Under(&[(RoamingAppData, &["Code", "User", "workspaceStorage"])]),
        ApplicationData,
        High,
        "VS Code workspace storage",
        "Per-project state such as open editors, extension data and chat history."
    ),
    container!(
        "vscode.app-data",
        Under(&[(RoamingAppData, &["Code"])]),
        ApplicationData,
        High,
        false,
        "VS Code app data",
        "VS Code settings, workspace storage, logs and caches."
    ),
    leaf!(
        "vscode.extensions",
        Under(&[(Home, &[".vscode", "extensions"])]),
        DeveloperTools,
        High,
        "VS Code extensions",
        "Installed VS Code extensions."
    ),
    leaf!(
        "windsurf.caches",
        Under(&[
            (RoamingAppData, &["Windsurf", "Cache"]),
            (RoamingAppData, &["Windsurf", "CachedData"]),
            (RoamingAppData, &["Windsurf", "Code Cache"]),
            (RoamingAppData, &["Windsurf", "GPUCache"]),
            (RoamingAppData, &["Windsurf", "CachedExtensionVSIXs"]),
            (RoamingAppData, &["Windsurf", "CachedProfilesData"]),
            (RoamingAppData, &["Windsurf", "Service Worker", "CacheStorage"]),
        ]),
        Cache,
        High,
        "Windsurf caches",
        "Chromium and extension caches used by Windsurf's editor shell. Rebuilt automatically; quit Windsurf before removing them."
    ),
    leaf!(
        "windsurf.logs",
        Under(&[(RoamingAppData, &["Windsurf", "logs"])]),
        Logs,
        High,
        "Windsurf logs",
        "Diagnostic logs written by Windsurf, one folder per session."
    ),
    leaf!(
        "windsurf.workspace-storage",
        Under(&[(RoamingAppData, &["Windsurf", "User", "workspaceStorage"])]),
        ApplicationData,
        High,
        "Windsurf workspace storage",
        "Per-project state such as open editors, extension data and chat history."
    ),
    container!(
        "windsurf.app-data",
        Under(&[(RoamingAppData, &["Windsurf"])]),
        ApplicationData,
        High,
        false,
        "Windsurf app data",
        "Windsurf's settings, workspace storage, logs and caches."
    ),
    leaf!(
        "windsurf.home",
        Under(&[(Home, &[".windsurf"])]),
        DeveloperTools,
        High,
        "Windsurf extensions",
        "Windsurf's installed extensions and configuration."
    ),
    container!(
        "windsurf.codeium",
        Under(&[(Home, &[".codeium"])]),
        DeveloperTools,
        High,
        false,
        "Codeium / Windsurf AI data",
        "Data used by Windsurf's and Codeium's AI features, such as the language server and local indexes."
    ),
    // ---- Apps built on web technology ------------------------------------------
    leaf!(
        "apps.chromium-caches",
        Under(&[
            (RoamingAppData, &["*", "Cache", "Cache_Data"]),
            (RoamingAppData, &["*", "Code Cache"]),
            (RoamingAppData, &["*", "GPUCache"]),
            (RoamingAppData, &["*", "DawnGraphiteCache"]),
            (RoamingAppData, &["*", "DawnWebGPUCache"]),
            (LocalAppData, &["Packages", "91750D7E.Slack_8she8kybcnzg4", "LocalCache", "Roaming", "Slack", "Cache"]),
            (LocalAppData, &["Packages", "91750D7E.Slack_8she8kybcnzg4", "LocalCache", "Roaming", "Slack", "Code Cache"]),
            (LocalAppData, &["Packages", "91750D7E.Slack_8she8kybcnzg4", "LocalCache", "Roaming", "Slack", "GPUCache"]),
        ]),
        Cache,
        High,
        "App web caches",
        "Caches that apps built on web technology (Slack, Discord, Notion, Figma and many others) keep for downloaded files, compiled scripts and graphics. The app rebuilds them when needed."
    ),
    leaf!(
        "chromium.http-cache",
        NameUnder {
            names: &["Cache_Data"],
            parent: "Cache",
            contains: "index",
        },
        Cache,
        High,
        "Web cache",
        "The download cache of a browser or of an app built on Chromium. It is rebuilt as pages are loaded again."
    ),
    // ---- AppData containers ------------------------------------------------------
    container!(
        "windows.store-apps",
        Under(&[(LocalAppData, &["Packages"])]),
        ApplicationData,
        Medium,
        false,
        "Microsoft Store app data",
        "Private data folders of apps installed from the Microsoft Store, one per app. Each holds the app's settings, documents and caches."
    ),
    container!(
        "windows.local-appdata",
        Under(&[(LocalAppData, &[])]),
        ApplicationData,
        Medium,
        false,
        "Local app data",
        "Data programs keep on this PC only: caches, databases, settings and some installed programs. Often essential to the program that created it."
    ),
    container!(
        "windows.roaming-appdata",
        Under(&[(RoamingAppData, &[])]),
        ApplicationData,
        Medium,
        false,
        "Roaming app data",
        "Program settings and data that can follow you to other PCs on a work network. Often essential to the program that created it."
    ),
    container!(
        "windows.appdata",
        Under(&[(Home, &["AppData"])]),
        ApplicationData,
        Low,
        false,
        "App data",
        "Per-user settings and data managed by Windows and programs."
    ),
    // ---- User folders -------------------------------------------------------------
    leaf!(
        "windows.onedrive",
        Under(&[(Home, &["OneDrive*"])]),
        UserData,
        High,
        "OneDrive",
        "Files synced with OneDrive. Deleting files here deletes them in the cloud too. Files that are online only use no space on this PC."
    ),
    container!(
        "home.downloads",
        Under(&[(Home, &["Downloads"])]),
        Downloads,
        High,
        false,
        "Downloads",
        "Files you downloaded. Installers and archives are often no longer needed, but this can be the only copy of a file."
    ),
    container!(
        "home.documents",
        Under(&[(Home, &["Documents"])]),
        UserData,
        High,
        true,
        "Documents",
        "Your documents folder."
    ),
    container!(
        "home.desktop",
        Under(&[(Home, &["Desktop"])]),
        UserData,
        High,
        true,
        "Desktop",
        "Files on your desktop."
    ),
    container!(
        "home.pictures",
        Under(&[(Home, &["Pictures"])]),
        UserData,
        High,
        true,
        "Pictures",
        "Your pictures."
    ),
    container!(
        "home.videos",
        Under(&[(Home, &["Videos"])]),
        UserData,
        High,
        true,
        "Videos",
        "Your videos and video projects."
    ),
    container!(
        "home.music",
        Under(&[(Home, &["Music"])]),
        UserData,
        High,
        true,
        "Music",
        "Your music library and audio projects."
    ),
    // ---- Project-level detections only on Windows --------------------------------
    leaf!(
        "dotnet.build-output",
        NameWithSiblingExt {
            names: &["bin", "obj"],
            extensions: &["csproj", "fsproj", "vbproj"],
        },
        BuildArtifacts,
        High,
        ".NET build output",
        "Compiled output (bin) and intermediate files (obj) of a .NET project, next to its project file. Rebuilt by the next `dotnet build` or Visual Studio build."
    ),
];

/// Files recognized by name on Windows, before extension rules apply.
pub static WINDOWS_NAMED_FILES: &[NamedFileRule] = &[
    NamedFileRule {
        names: &["hiberfil.sys"],
        rule: FileRule {
            id: "windows.hibernation-file",
            extensions: &[],
            category: SystemFiles,
            label: "Hibernation file",
            explanation: "Holds the contents of memory while the PC hibernates; Fast Startup uses it too. Windows sizes it; `powercfg /h off` removes it and turns both features off.",
            confidence: High,
        },
    },
    NamedFileRule {
        names: &["pagefile.sys", "swapfile.sys"],
        rule: FileRule {
            id: "windows.page-file",
            extensions: &[],
            category: SystemFiles,
            label: "Virtual memory file",
            explanation: "Windows' virtual memory. Windows sizes it to the memory programs need; change it only in System Properties › Advanced › Performance › Virtual memory.",
            confidence: High,
        },
    },
    NamedFileRule {
        names: &["MEMORY.DMP"],
        rule: FileRule {
            id: "windows.memory-dump",
            extensions: &[],
            category: Logs,
            label: "System memory dump",
            explanation: "A memory dump Windows wrote after a system crash. It helps diagnose the crash; Windows does not need it to run.",
            confidence: High,
        },
    },
];

/// Extension rules on Windows: the macOS ones plus Windows installers.
pub static WINDOWS_FILE_RULES: &[FileRule] = &[
    FileRule {
        id: "file.log",
        extensions: &["log"],
        category: Logs,
        label: "Log file",
        explanation: "A log file, identified by its extension.",
        confidence: Medium,
    },
    FileRule {
        id: "file.installer",
        extensions: &["msi", "msix", "msixbundle", "appx", "appxbundle", "exe"],
        category: BinaryData,
        label: "Installer or program",
        explanation: "An installer (.msi, .msix, .appx) or a program (.exe). Installers are often kept after installing, but a program may be run from here or be the only copy you have.",
        confidence: Medium,
    },
    FileRule {
        id: "file.disk-image",
        extensions: &["iso", "dmg", "pkg"],
        category: BinaryData,
        label: "Installer or disk image",
        explanation: "Installers and disk images are often kept after installing, but may be the only copy of what they contain.",
        confidence: High,
    },
    FileRule {
        id: "file.archive",
        extensions: &["zip", "tar", "gz", "tgz", "bz2", "xz", "zst", "7z", "rar", "cab"],
        category: BinaryData,
        label: "Archive",
        explanation: "A compressed archive. It may be a backup or the only copy of its contents.",
        confidence: High,
    },
    FileRule {
        id: "file.vm-disk",
        extensions: &["vmdk", "qcow2", "vdi", "vhd", "vhdx", "avhdx", "img"],
        category: BinaryData,
        label: "Virtual disk image",
        explanation: "A virtual machine or WSL disk. It contains a whole operating system and its data.",
        confidence: Medium,
    },
    FileRule {
        id: "file.ml-model",
        extensions: &["safetensors", "gguf", "ckpt", "pt", "pth", "onnx", "h5", "tflite"],
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
            "mov", "mp4", "m4v", "mkv", "avi", "wmv", "webm", "jpg", "jpeg", "png", "heic", "gif",
            "tiff", "raw", "cr2", "nef", "arw", "dng", "psd", "mp3", "m4a", "wav", "aiff", "flac",
            "wma",
        ],
        category: UserData,
        label: "Media file",
        explanation: "A photo, video or audio file. Likely personal content.",
        confidence: Medium,
    },
    FileRule {
        id: "file.document",
        extensions: &["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp"],
        category: UserData,
        label: "Document",
        explanation: "A document file. Likely personal content.",
        confidence: Medium,
    },
];
