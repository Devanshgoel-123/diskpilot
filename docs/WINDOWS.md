# Windows support plan

DiskPilot runs on macOS today. The architecture carries over to Windows: the
engine is Rust and the shell is Tauri, which builds Windows installers (MSI
and NSIS). What changes is how files are measured, where tools keep their
data, and what must never be touched. The safety model stays the same:
scanner, classifier, recommendations, planner, then one executor behind the
safety gate.

## What stays the same

- The React interface, recommendation tiers, single-use plans and cleanup
  history.
- The executor's rule that every path is re-authorized immediately before it
  is moved.
- Docker integration through the `docker` CLI with a fixed argument list and
  no shell.
- The optional assistant, with the API key stored in Windows Credential
  Manager (the `keyring` crate supports it) instead of the macOS Keychain.

## Scanner

| Concern            | macOS today                         | Windows                                                                                                                               |
| ------------------ | ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| On-disk size       | `st_blocks * 512`                   | `GetCompressedFileSizeW` (accounts for NTFS compression and sparse files)                                                             |
| Hard links         | device and inode                    | volume serial number and file index from `GetFileInformationByHandle`                                                                 |
| Links              | symlinks are never followed         | symlinks, junctions and other reparse points are never followed                                                                       |
| Staying on a drive | same `st_dev`                       | same volume serial number                                                                                                             |
| Cloud files        | iCloud placeholders are small files | OneDrive placeholders (`FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`) are counted at their local size and never opened, so nothing downloads |
| Path length        | no practical limit                  | `\\?\` extended paths for paths over 260 characters                                                                                   |
| Case               | case-insensitive APFS by default    | case-insensitive NTFS; rules compare case-insensitively                                                                               |

A faster mode that reads the NTFS Master File Table directly, as WizTree does,
needs administrator rights. It is a possible later option, not the default,
because DiskPilot should work without elevation.

## Locations and rules

The classifier needs a Windows rule table. Known locations include:

- Temporary files: `%TEMP%` (`%LOCALAPPDATA%\Temp`).
- Package caches: `%LOCALAPPDATA%\npm-cache`, `%LOCALAPPDATA%\pnpm\store`,
  `%LOCALAPPDATA%\Yarn\Cache`, `%USERPROFILE%\.nuget\packages`,
  `%USERPROFILE%\.cargo`, `%USERPROFILE%\.gradle`, pip and uv caches.
- Editors: Cursor and VS Code under `%APPDATA%\Cursor` and `%APPDATA%\Code`
  (same `User\workspaceStorage` layout, so the deleted-project detection
  works unchanged), extensions under `%USERPROFILE%\.cursor` and
  `%USERPROFILE%\.vscode`.
- AI agents: `%USERPROFILE%\.claude`, `%USERPROFILE%\.codex`,
  `%USERPROFILE%\.copilot`.
- Project output: `node_modules`, `target`, `bin`/`obj` for .NET, `.venv`.

## Docker and WSL

Docker Desktop on Windows stores its data in a WSL 2 virtual disk (`.vhdx`)
under `%LOCALAPPDATA%\Docker\wsl`. Pruning images frees space inside the disk
but does not shrink the file. DiskPilot will:

1. Report the virtual disk's file size next to Docker's own reclaimable figure,
   so the gap is visible.
2. After a prune, explain the compaction steps: quit Docker Desktop, run
   `wsl --shutdown`, then `Optimize-VHD` (Hyper-V, Pro editions) or
   `diskpart` `compact vdisk` (Home editions).

Compaction needs administrator rights and stopping WSL, so DiskPilot shows the
steps rather than running them.

## Protected paths

Refused by the safety gate: `C:\Windows`, `C:\Program Files`,
`C:\Program Files (x86)`, `C:\ProgramData`, `C:\Recovery`,
`C:\System Volume Information`, `C:\$Recycle.Bin`, the root of every drive,
and the user profile's top-level folders. The component store (`WinSxS`),
Windows Update downloads, `hiberfil.sys`, `pagefile.sys` and `Windows.old`
are reported with their sizes and pointed to Windows' own tools (Disk
Cleanup, Storage Sense, `DISM /StartComponentCleanup`); DiskPilot never
removes them itself.

## Cleanup

The `trash` crate already supports the Windows Recycle Bin, so the executor's
Trash-first behavior carries over. Permanent deletion stays limited to
regenerable data.

## Applications

Installed programs come from the registry's Uninstall keys (all users in the
64-bit and 32-bit views, and the current user), filtered like Settings > Apps:
system components, updates (`ParentKeyName`, `ReleaseType`) and entries
without an uninstaller are hidden. Store apps come from the per-user package
repository in the registry (`AppModel\Repository\Packages`), limited to
packages with a data folder in `AppData\Local\Packages`, which leaves out
frameworks; this is much faster than `Get-AppxPackage`.

- Size: the measured install folder, or `EstimatedSize` when the program
  registered no usable folder (a drive root or Program Files itself is
  ignored). Programs inside the Windows folder show no size, like system apps
  on macOS.
- Data: folders in `AppData\Roaming`, `AppData\Local`, `AppData\LocalLow`,
  `AppData\Local\Packages` and `ProgramData` named after a program, with
  vendor folders such as `Google\Chrome` looked into.
- Leftovers: only Store app data in `AppData\Local\Packages` whose package
  is no longer installed, the equivalent of macOS containers. Other AppData
  folders often belong to tools with no uninstaller, so they are not called
  leftovers.
- Uninstall: DiskPilot never deletes a program's folder. After the person
  confirms (the window shows the exact command), it starts the program's own
  `UninstallString` with `ShellExecuteEx`, never through a shell. Windows
  Installer entries run `msiexec.exe /X{product}`. Store apps, and entries
  that would start a script host, open Settings > Apps instead.
- Icons come from the shell (`IShellItemImageFactory` on `DisplayIcon`, or
  the Store app's `shell:AppsFolder` item) instead of `NSWorkspace`, keeping
  the rule that no third-party logos ship with DiskPilot.

## Shell

- Previews: `IShellItemImageFactory` thumbnails, never for an online-only
  cloud file (drawing one would download it).
- Show in File Explorer: `SHOpenFolderAndSelectItems`.
- Tray: left click opens the window, right click shows the menu; closing the
  window keeps DiskPilot in the notification area. A second launch brings the
  running copy's window back.
- Notifications need the AppUserModelID the installer registers; a copy run
  from `target` may show none.

## Distribution

Unsigned installers trigger SmartScreen warnings, and Smart App Control blocks
unknown unsigned apps on Windows 11. Releases should be signed, for example
with Azure Artifact Signing through Tauri's `signCommand`. Reputation builds
over time when every release uses the same certificate.

## Order of work

1. Platform layer for size, file identity and link handling, with tests on a
   Windows CI runner.
2. Windows rule table and protected paths.
3. Analyzer path tables for editors, agents and package managers.
4. Docker and WSL disk reporting.
5. Signed installer and release pipeline.
