[简体中文](usage.zh.md) · [Project README](../README.md)

# Using AxSSH

## Start the application

AxSSH requires Rust `1.92.0` or newer and a desktop environment supported by
Slint's winit backend. From the repository root, run:

```bash
cargo run --locked
```

## Create and connect to a session

1. Choose **File > New Server**, press `Cmd+N` on macOS or `Ctrl+N` on Windows
   and Linux, or right-click blank sidebar list space and choose **New Server**.
   Change this shortcut in **Settings > Shortcuts**.
2. Choose **SSH**, **Telnet**, or **Serial**, then enter the fields for that
   protocol. SSH accepts host, port, username, and password, private-key, or
   **SSH agent** authentication. Its per-server **X11 forwarding** field offers
   **Off**, **Untrusted (-X)**, and **Trusted (-Y)**; new profiles default to
   Trusted to preserve the previous enabled behavior. Telnet accepts host and port and displays an unencrypted
   transport warning. Serial accepts a port name, baud rate, data bits, stop
   bits, parity, and flow control. AxSSH lists detected ports when the editor
   enters Serial mode; use **Refresh** after a device is attached, or enter a
   path/name manually. In the SSH-only **SFTP directories** section, set the
   remote and local directories that new SFTP Tabs should open first; `~` and
   the platform home directory are the defaults.
3. Save the session, then select it in the session navigator. For a new SSH
   profile, **Save & connect** saves the profile and immediately starts the
   normal host-key flow. The editor password is optional: leave it empty to
   save the profile without a password, or use a non-empty value once for that
   connection. Select **Save password (optional)** to persist it. A save with
   an encrypted-vault selection accepts an empty vault password: AxSSH generates
   a per-server random unlock key, stores it in the protected system credential
   store, and uses it only to unlock the encrypted record. The key is never
   shown; the OS may request authorization when saving or reading it.
   Opening the same saved session more than once creates
   independent terminal tabs with separate connections and output. Each SSH Tab
   can independently wait for host-key confirmation or authentication; the
   security prompt always belongs to the active Tab, and switching Tabs
   preserves the other pending prompt.
4. For SSH, compare the displayed SHA-256 host-key fingerprint on the first
   connection
   with a trusted source before confirming it. A changed key requires another
   explicit confirmation and should be investigated before acceptance.
5. For SSH password or private-key authentication, enter a transient password or private-key passphrase when prompted. A
   private-key passphrase is never persisted. To remember an SSH password,
   first choose **System credential store** or **Encrypted application vault**
   in **Settings > General**. The password prompt starts with that choice, and
   its **Save password in** menu can override the backend for this prompt.
   Select **Save password (optional)** to save after successful authentication.
   A requested encrypted-vault save with an empty vault password generates a
   hidden per-server unlock key in the protected system credential store; a non-empty
   vault password remains a user-managed vault record. Existing user-managed
   vault records still ask for their vault password, while generated records
   unlock in the background without showing the key.
   Password, vault-password, and passphrase fields cannot be copied, cut, or
   selected, and are cleared after a submitted secret is accepted or the prompt
   is cancelled.
   RSA private keys and RSA agent identities are supported for compatibility
   with existing servers. This build intentionally accepts RUSTSEC-2023-0071,
   a Marvin timing-side-channel advisory with no patched release in the pinned
   RSA implementation. Do not use RSA authentication where an attacker can
   observe signing timing; prefer Ed25519 or ECDSA when possible.
   Closing a probing or pending-authentication Tab cancels or discards only that
   Tab's connection flow.
6. For **SSH agent**, AxSSH uses the agent available when the connection starts
   and does not show a password or passphrase prompt. Unix and macOS read the
   current `SSH_AUTH_SOCK`; Windows uses `SSH_AUTH_SOCK` when set, otherwise the
   default OpenSSH agent named pipe. The profile stores only the authentication
   method, not the socket path, identity comments, public keys, private keys, or
   passphrases. AxSSH tries at most five identities within one 30-second agent
   authentication timeout and releases the agent connection when authentication
   finishes or is cancelled. A locked agent may show its own system confirmation.
   If the agent is unavailable, empty, times out, or every offered identity is
   rejected, unlock or update the runtime agent and reconnect the Tab. Host-key
   confirmation remains mandatory before every first or changed-host connection.

Choose the local server under **Settings > X11**. macOS offers Auto, XQuartz,
MacXServer, and Custom; Windows offers Auto, VcXsrv, Xming, and Custom; Linux
offers System DISPLAY and Custom. AxSSH locates known providers through the
macOS application database or the Windows executable search path and Program
Files and displays the detected locations in Settings; choose Custom to provide
an executable path yourself. Trusted (`-Y`) forwarding keeps first-X11-
application startup enabled by default: opening an SSH shell only asks the
server for forwarding and never starts a local X server. Untrusted (`-X`)
forwarding asks the local X server to generate a short-lived restricted
authorization after host-key verification and SSH authentication but before the
X11 forwarding request, so it may prepare or start the selected local server
before the shell opens. It never falls back to no-auth or a real trusted cookie.
The trusted mode still requires an exact
`MIT-MAGIC-COOKIE-1` from `xauth`; XQuartz and system X.Org/Xwayland should use
this mode. **Allow local
connections without X authority** is off by default and should be enabled only
when needed for MacXServer or an AxSSH-started VcXsrv/Xming instance. Those
compatibility launches are loopback-only; the Windows servers receive `-ac`
only after this explicit choice.

The remote SSH server must also allow X11 forwarding, normally through
`X11Forwarding yes` and a usable server-side `xauth`. AxSSH does not change
`sshd_config`. A remote empty `DISPLAY` means that the forwarding request was not
established, commonly because `sshd` rejected it. If local preparation fails when
the remote graphical application opens, AxSSH rejects that graphical channel;
the shell remains connected and reports X11 as unavailable. An untrusted
authorization expires after 20 minutes; use Trusted only for remote programs
you trust with full access to the local X display.
Closing the Tab cancels every active X11 relay.

Telnet traffic, including any login text entered in the terminal, is sent
without encryption. AxSSH does not fill Telnet credentials. A Serial scan only
reads the operating system's available-port metadata; it does not open a port,
send probe bytes, toggle modem lines, or guess communication parameters. After
you explicitly connect, AxSSH scans again and uses saved USB vendor/product and
serial-number metadata to follow a device whose OS port name changed. Missing
or ambiguous matches are rejected so you can select the intended device.

Right-click a group to add a server, copy or duplicate the group, rename the
group, or delete it. Right-click Ungrouped to add a server. Right-click a server
to copy its address, copy its AxSSH configuration, or duplicate it. An SSH
server also provides **Open SFTP** to open a dedicated SFTP Tab without creating
a terminal shell; Telnet and Serial servers keep that action disabled. The same
server menu can still connect, edit, or delete the profile; edit reuses the same
session editor.

Choose **File > Import from Clipboard** to import a versioned AxSSH export. The
default shortcut is `Cmd+Shift+I` on macOS and `Ctrl+Shift+I` on Windows and
Linux. Select a group or server in either sidebar mode, then choose **File >
Export Selected to Clipboard**; its default shortcut is `Cmd+Shift+E` on macOS
and `Ctrl+Shift+E` elsewhere. Both shortcuts are configurable in **Settings >
Shortcuts**. Export reports a status message without changing the clipboard
when no persisted group or server is selected.

**Copy Server** and **Copy Group** write versioned AxSSH JSON to the clipboard.
The export is limited to 256 KiB and 128 servers and excludes profile identity,
remembered-password references, and trusted host-key fingerprints. Import only
adds profiles: it assigns new UUIDs, resolves name/group conflicts, and clears
credential references and host trust again before persistence. It never imports
a password, vault password, private-key passphrase, or trusted-host decision.

Changing an SSH host or port
clears the confirmed host-key fingerprint, so the new endpoint must be trusted
explicitly on its next connection. Switching an SSH profile to Telnet or Serial
removes its remembered SSH credential reference. The session editor never shows
a saved password; leaving the field blank preserves it. Renaming a profile or
changing its host, port, or username also preserves the saved password because
the credential is keyed by the profile's stable UUID. A newly entered password
can be used once by **Save & connect**, or persisted by selecting **Save
password (optional)** and a backend. An encrypted-vault save with an empty vault
password uses a hidden per-server unlock key held in the protected system
credential store. Changing the default in **Settings > General** only
initializes future storage selections and does not migrate the backend referenced
by an existing SSH profile.

Choose **Pane > New Local Shell** or the Local Shell control to open an
independent local terminal. Close a terminal with its tab control or **Window >
Close Current Tab**.

With an SSH or SFTP Tab active, choose **Pane > Switch SSH/SFTP Tab**. The
default shortcut is `Ctrl+M`; change it in **Settings > Shortcuts**. From an
SSH Terminal without a companion, the command creates a dedicated SFTP Tab
immediately after it. From a standalone SFTP Tab, it creates an SSH Terminal
immediately before it. Each new Tab repeats the normal host-key and
authentication flow with its own SSH transport. Once paired, using the command
in either Tab only activates its companion. Closing either Tab leaves the other
open and clears the pairing, so a later command can create a new companion.

The SFTP Tab opens only the `sftp` subsystem: it never allocates a PTY or
terminal shell. The upper area keeps remote and local file browsers side by
side with matching Name, Size, and Modified columns. Drag the vertical divider
to change the two browser widths, or double-click it to restore equal widths.
Drag the horizontal divider to resize the files and Transfers areas;
double-click it to collapse or expand Transfers. Both dividers participate in
Tab focus and accept the matching arrow keys, Home, and End. Enter or Space
performs the divider's double-click action. The layout remains while switching
Tabs in the current application run and returns to its defaults after restart.
The individual file-table columns are not resizable in this phase.

When there are no transfer records, Transfers stays as a compact bar so the file
browsers keep the available height. After a transfer appears, use the bar's
chevron to expand or collapse the transfer list; the horizontal divider remains
available for manual sizing.

Each browser's title row makes **Download** (remote to local) or **Upload**
(local to remote) the primary action. Its arrow shows the direction, and a
selection adds the number of selected entries to the label. The action is
available only with a ready selection and destination. **Refresh** uses an icon;
copy path and remote **Edit** remain secondary controls when there is room.
**Hidden** includes dot files in that browser: it stays in the title row on wide
panes and moves to the status row on narrow panes.

Both path rows place **Back**, **Forward**, and **Parent directory** buttons
before the current path and a **Path: Settings** gear at the right. The path
fills the remaining width. A fitting path is shown in full;
otherwise only the middle is replaced by `…`. Both ends are retained whenever
space allows, with roughly three quarters of the retained characters reserved
for the tail. The display uses the actual font width and updates on resizing
or font changes. Hover an abbreviated path to see the complete path.

Click the path, or choose **Edit path** in the gear menu, to edit the complete
path. Press Enter or choose **Open** in that menu to navigate; Escape returns
to the current path. `Cmd+L` on macOS or `Ctrl+L` on Windows/Linux also starts
editing while a path control has focus. The gear menu includes **Copy current path**, **Back**, **Forward**,
and **Parent directory**. Copying and navigation use the complete path;
the displayed ellipsis never becomes part of a filesystem target.

Windows local paths use the familiar drive or network-share format, such as
`D:\software\Xming` or `\\server\share`, in display, editing and copying.
The internal `\\?\` prefix remains only when needed to preserve a special path's
meaning. Directory refresh, history and transfers retain the canonical path;
remote addresses are displayed as returned by the server.

Back and Forward follow that browser's visited directories; Parent directory
opens the parent of its currently displayed path, even if the previous visit
was elsewhere. A successful parent navigation can be reversed with Back. The
parent control is disabled at the filesystem root and while a directory is
loading.
Refresh reloads the displayed directory without changing
Back or Forward history; Forward becomes available after going Back.

Enter an absolute path, a path relative to the
current directory, or `~` in the remote browser through **Edit path**, then press Enter or **Open**; double-click
a folder to enter it. The remote browser starts at the SSH profile's configured
default directory. The local browser starts at its configured directory (or the
platform home directory when that value is empty), and reads only bounded file
metadata.
Use **More** to release the next 250-item
local page or request the next bounded remote page. Click **Name**, **Size**, or
**Modified** to sort either file list; the default is newest Modified first.
Remote pages show the server's first available batch, up to 250 rows, so a slow
connection does not have to fill a page before displaying files. **More** may
remain available until the next request confirms the end of the directory.
Uploads and downloads use up to 16 concurrent 64 KiB chunk requests per file;
this is separate from the number of simultaneous files configured in Settings.
Progress updates are throttled to 100 ms, and unchanged file rows retain their
models during transfers. Remote editor operations run in a bounded background
queue so browsing and cancellation remain responsive.
Long names are elided to the available column width; hover an elided name to
see the complete name with wrapping. Rows use the target platform's file-type icon when one is available
and a built-in folder, link, or generic-file icon otherwise.

Double-click a regular file in the local pane to open the current snapshot entry
with the platform's default application. Directories continue to navigate and
symbolic links are not opened. AxSSH verifies the opened file's platform identity
outside the UI thread, copies that exact handle into its private bounded cache,
and opens the completed read-only snapshot. Replacing the original path after
validation cannot redirect the open request.
Set the maximum size of this local-open snapshot under **Settings > SFTP**
(default 512 MiB; 1 MiB to 20 GiB for a finite limit). Set 0 to remove both
the per-file limit and the private cache byte quota. The cache quota grows to
fit any finite per-file limit. Opening large files still requires enough local
disk space and copying time; 0 may use substantial disk space.

Right-click a local row to open a file or folder, reveal a non-link entry in its
local folder, or upload the selected files and folders. The local and remote list checkboxes,
including their header controls, update the active SFTP Tab's selection before a
menu action runs.

Right-click a remote file or folder and choose **Download**. Right-clicking an
unselected row first makes it the only selected row; right-clicking a selected
row keeps the current multi-selection, so the menu action applies to that
selection. Each file is written into
the current **Local files** directory; directory downloads recursively preserve
their selected directory tree. Remote symbolic links and non-regular entries
are skipped or rejected, existing local files are never overwritten, and each
file is subject to the configured SFTP file-size limit (20 GiB by default).
Recursive discovery is bounded to 4,096 scanned entries, 512 files, 256
directories, 16 levels, and 512 KiB of path text; there is no separate batch-byte cap.

SFTP transfers can omit generated system files. In **Settings > SFTP**, enable
**Filter system files** to use the current platform preset (macOS includes
`.DS_Store`, `._*`, `.Spotlight-V100`, `.Trashes`, and `.fseventsd`; Windows and
Linux have their corresponding common metadata names). Add one custom filename
pattern per line; `*` matches any characters. The same filter applies to local
uploads, system file-manager drops, remote downloads, and every level of recursive directory
transfers. **Restore platform defaults** clears custom patterns and re-enables the
platform preset; settings changes follow the normal Settings draft/save flow.

The Transfers area has separate **Transferring**, **Failed**, and **Success**
pages. Select active rows with their checkboxes to pause, resume, or cancel
them in a batch from the right side of the **Transferring** page bar. These
actions share the page bar instead of reserving a separate row. Pause/resume
preserves the downloaded prefix through the live
worker and continues from that offset; it is available only while this
application and SFTP worker remain running. Each SFTP Tab runs at most two
active transfers at once by default; **Transfers per SFTP tab** in
**Settings > SFTP** changes this limit from 1 to 16. **Global simultaneous
uploads** changes the process-wide upload limit from 1 to 32 (the default is
8); additional files wait in the queue or are subject to the existing transfer
state. Cancel removes the task's partial content,
including a file published just before cancellation wins; failures remove the
`.part` file, while completed local downloads remain in the chosen directory.
Closing the SFTP Tab cancels and joins pending discovery, subsystem-opening, and
active download work before closing the browser and SSH transport.

Every transfer row prefixes its filename with **Upload** or **Download**,
including queued, in-progress, failed, cancelled, and successful records. The
progress column continues to show the percentage or terminal result such as
`Uploaded` or `Downloaded`.
When an upload finds a remote file with the same name, a dialog offers **Skip**,
**Keep both**, or **Overwrite**. **Apply this choice to this upload batch** uses
the same choice for the remaining files in that upload/drop batch. Skip appears
as a completed `Skipped` transfer. Overwrite requires server support for atomic
replacement; if the remote file changes during the upload, it fails and leaves
the existing file intact.

Right-click an active transfer for its applicable Pause, Resume, or Cancel
action. Right-click a failed, cancelled, or successful record to remove it, or
to reveal its local source or downloaded file when that path is available.
After an upload, remote deletion, rename, or external edit upload succeeds, AxSSH
refreshes **Remote files** when that pane still shows the affected directory or
an ancestor. After a download succeeds, it likewise refreshes **Local files**
when that pane still shows the destination directory or an ancestor. Changes
completed while a listing is loading are coalesced into one follow-up refresh;
navigation to another directory is not interrupted. Failed, cancelled, and
skipped operations do not trigger an automatic refresh.
Uploads can still target the current remote directory while that same directory
is refreshing. Navigation to another remote directory pauses new uploads until
its destination is known. Returning to an SFTP Tab keeps the loaded Local files
selection, including after an SFTP reconnect. Changes made to the displayed
local directory by Finder, Explorer, or another local program trigger a fresh
listing. Use **Refresh** when a filesystem, such as a network mount, does not
deliver change notifications.
An unexpected SSH or SFTP subsystem disconnect is detected even while the
browser is idle. Reconnection restores the last displayed remote directory (or the Tab's
initial target if no page loaded yet); unfinished transfers are not replayed.
Directory permission errors stay on the current connection. A password prompt
or host-key confirmation may still be required by the normal SSH security flow.

The remote row context menu offers **Download**, **Rename**, and **Delete**
(directory deletion is non-recursive). **Rename** accepts one selected entry
and opens a dialog with its current name; Enter confirms and Escape cancels.
On an unselected row, choosing an action selects only that row. Rename is
disabled on a selected row when multiple entries are selected. The directory
toolbar retains **Edit** in the system-associated application. Edit downloads
a private local copy and opens the program associated with its extension on
Windows, macOS, or Linux. AxSSH has no built-in SFTP editor. Text encodings and
binary bytes are preserved. **Settings > SFTP > Remote edit file limit (MiB)**
controls both downloaded and saved copies (default 4 MiB, range 1–64 MiB).
Files above the configured limit can use the normal Download/Edit/Upload workflow. Set file
associations in your operating system; a launcher failure is reported, and
AxSSH never falls back to an internal editor.

Saving the local copy uploads changes after a 500 ms debounce. **Auto upload**
is on by default and applies to all files edited in this tab; turn it off to use
**Upload changes** for the file shown in the edit status bar. Repeating Edit for
the same remote file reopens its existing local copy, preserving pending changes.
Each tab supports eight edited files. **Show local copy** reveals the copy in the
system file manager. **Stop editing**, disconnect, reconnect, or closing the tab
ends monitoring; the copy remains available, and reconnect does not replay an
old edit. Up to 128 copies are retained in the application cache's `sftp-edit`
folder. Preserve needed files and remove old copies there if that limit is reached.
Saving under a different local filename does not change the watched file or
remote destination; upload that new file through the normal upload action.

Before uploading and again before publishing, AxSSH checks the original remote
size/mtime. A conflict or failed upload retains local changes and pauses automatic
retries; **Upload changes** can retry a transient failure. The server must support
`posix-rename@openssh.com` for atomic replacement; otherwise the original remote
file is retained. These checks cannot detect a remote rewrite that preserves both
size and timestamp, and there is no three-way merge or cross-process edit recovery.
The local toolbar uploads all selected files and folders.
Dragging a selected local row carries the current selection; dragging an unselected
row carries that row. Local files and folders, or files and folders dragged from a
system file manager such as Finder, Windows Explorer, or a Linux file manager,
dropped onto Remote files upload to the current remote directory. Folder uploads recursively discover regular
files, preserve relative paths, and create needed remote directories. Empty folders
without eligible files do not create a remote directory. Symbolic links and filtered
names are skipped. A batch is bounded to 512 files, 256 directories,
16 levels, 4,096 scanned entries, and 512 KiB of path text. Each file is
limited by the configured maximum (20 GiB by default); there is no separate
batch-byte cap. Exceeding a limit rejects the batch before upload.
Dragging a remote file or folder onto Local files queues a download
into the current local directory. Completed downloads remain there and never
open automatically. On macOS, dragging a remote regular file can
also be dropped into Finder as a native file: AxSSH downloads it only after the
target accepts the drop. Returning that same drag to its originating AxSSH SFTP
window downloads it into the Local files directory that was visible when the
drag started. Folders, links, and filtered entries retain the in-app drag
behavior; Finder-to-Remote uploads are unchanged.

## Workspace and terminal controls

AxSSH automatically saves the open workspace after layout changes settle and
again on exit in a separate private `workspace.json`. After restart it restores
main/detached window sizes, positions and maximized state, tab order, the active
tab, terminal split panes and ratios, bounded terminal text, and SFTP browser paths.
Fullscreen and minimized states are not restored. If a display has been removed,
windows are fitted to a remaining display; Wayland controls absolute positioning.
Changes normally save about one second after you stop moving/resizing, while
forced termination can lose the latest changes that have not reached disk. Saved connections are recreated as new
workers rather than persisted live connections. SSH still follows the normal
trusted host-key and authentication flow, and missing profiles are skipped.

Use **File > Save Workspace** to write the same bounded snapshot to the path in
the workspace dialog. The default path is the private `workspace.json`, so
saving it there also defines what the next AxSSH launch restores. **File > Open
Workspace** validates the selected JSON before changing the current workspace,
then stops its workers, restores all valid Tabs and pane/window layouts, and
starts fresh workers through the normal trust and authentication flow. A custom
path can be used for named snapshots. Successful opens are kept in **File >
Open Recent** (up to eight paths). The next launch first restores the private
`workspace.json` checkpoint; only when it is missing or invalid does AxSSH try
recent files from newest to oldest and remove unavailable entries. Autosave does
not overwrite named workspace files; use Save Workspace to update them.
**Clear Recent** removes only this path history. Saving a snapshot does not
change the recent-open order, and the Save dialog still defaults to the private
`workspace.json` to avoid accidental overwrites.

Saved sessions are organized beneath collapsible group rows in the expanded
navigator. Right-click blank list space to create an empty group or an
Ungrouped server even before any profile exists. Use the search field to find a
group or server by name or connection details; matching groups expand to show
results. Groups show a folder icon, name, count, and disclosure chevron. Click a
group row, or focus it and press Enter or Space, to expand or collapse it.
Both sidebar modes use the same small chevron to the folder's right: down for
expanded groups and right for collapsed groups. The expanded sidebar keeps a
compact server inset and aligns server names with group names. Every
visible server has a system icon, name, and masked endpoint on a single indented
row. After a successful SSH or SFTP connection, AxSSH tries to read a short
remote release label for the tab and its sidebar icon. An unavailable release
keeps the connected session usable and uses a generic server icon.

**View > Toggle Session Sidebar** switches to a narrow icon rail. Groups use
folder icons, and servers use their detected system's icon. Linux distributions
such as Ubuntu, Debian, Fedora, Rocky Linux and openSUSE use their own artwork;
Tux is the fallback for Linux with no recognized dedicated mark.
The Local Shell entry and its tabs use the host operating system's icon,
regardless of the chosen shell or tab title; unsupported systems use a terminal
icon. Hover for names and connection details;
keyboard focus and accessible labels retain each action's identity. The rail
keeps group disclosure and the same row context menus. The obsolete collapsed
group-label picker is removed from Settings; existing saved values remain
compatible and the rail always stays narrow. System icons, including Local
Shell's, retain their source colors by default. With AxSSH closed, edit
`settings.appearance.system_icons_follow_theme` in `sessions.json`: `false` is
the default; set it to `true` and restart to tint system icons in the sidebar,
tabs, detached title bars, and `+` connection list with the current theme.
This option has no Settings control, and Settings
preview/save preserves its value. Folder, terminal, and generic-server UI
symbols continue to use theme colors. Group folders, server icons, and Local Shell
use a shared 20px icon box in the sidebar; the `+` connection list uses the same
size for server icons. Indentation and disclosure arrows express the sidebar
hierarchy. Tab and detached title-bar icons retain their compact 16px size.

In the server editor, **System icon** is a compact dropdown with the current icon
and system display name. Its separate search popup shows up to six rows at once,
with centered icons and a check beside the current choice; it leaves the form's
height unchanged. Search by name, ID, or alias. Use arrow keys and Enter to select,
or Escape/click outside to dismiss. The **Automatic** row restores automatic
selection. A manual choice takes priority.
**Automatic** stays first, followed by a common-system shortlist beginning with
Ubuntu, Debian, Windows, macOS, CentOS, and Rocky Linux. Other choices are sorted
alphabetically by display name, ignoring case. Search results use the same order
in both the server and Local Shell pickers.
The profile's optional `system_icon_override` stores the canonical ID. Successful
SSH/SFTP discovery saves a bounded release label and icon ID in the profile's
`detected_system`; later connections, closed tabs, and application restarts reuse
it without probing again. Cache validity includes host, port, username, and host-key
fingerprint. Failed detection is not cached; successful unknown systems retain a
generic icon without repeated probes. To update an automatic choice after changing
the server OS, select **Detect again on next connection**, save, and reconnect.
Manual choices skip the icon probe. Editing the destination invalidates automatic
metadata while preserving the manual choice. Exports preserve manual choices and
omit detected metadata.

**Settings > General > Local shell icon** uses the same picker and saves immediately
to `settings.appearance.local_shell_icon_override`; **Automatic** restores host
identification. Unknown or invalid IDs in configuration fall back to automatic.
Local Shell's platform icon is available before opening
a shell. On Linux, AxSSH reads local os-release once in the background to select
the distribution icon; unavailable or unrecognized information keeps Tux.
The [system name index](../assets/os-icons/names.md) lists canonical IDs,
display names, accepted aliases and shared icon paths.
The selected group or server remains highlighted across expanded and compact
views, and release updates preserve the current group's disclosure state.
Deleting a group moves its servers to Ungrouped. Deleting a profile also removes
its remembered password but does not close terminal tabs already open.

Text buttons, including the session editor's Cancel, Save & connect, and
Save session actions, reserve enough width for their actual font when space
allows. When constrained, their label shows an ellipsis; hover to read the
complete text in a bounded tooltip. Form labels/descriptions, dropdown values
and options, menu labels, and SFTP columns use the same behavior. Empty-field
instructions can also show a full-text hint; password values never do.
Icon-only controls continue to describe their
action with their existing tooltip and accessible name.

The sidebar masks usernames and IPv4 addresses by default: a username keeps its
first and last two characters when available, and `192.168.1.202` becomes
`192.*.202`. Change the single mask character in **Settings > Workspace**.
Hostnames remain visible so they can still be distinguished at a glance.
New-session editors and terminal sessions share the workspace tab bar, while
Settings opens as a separate workbench view. The `+` at the right end of that
bar lists every saved connection with its system icon, name, and masked endpoint,
and connects the selected profile. Its icons use the same manual choice, valid
cache, current release, and generic fallback priority as the sidebar, including
before any Tab is opened; opening the list never probes a server. **File >
New Server** and the sidebar blank-area context menu open the session editor.
Activating a connection or opening a new editor synchronizes keyboard focus to
the active workspace Tab and scrolls it into view; the editor then focuses its
**Session name** field.

Connected SSH and SFTP Tabs use the status dot to show smoothed SSH round-trip
response time: green below 100 ms, yellow from 100 to below 250 ms, orange from
250 to below 500 ms, and red at 500 ms or above. Hover the Tab's title, icon,
order or status dot for its full title, available system details, latency and
last update time, even when its title fits. Measuring, timed-out, expired or
unavailable data shows the corresponding state; disconnecting removes the
connection data. Dragging temporarily hides the tooltip, and close/window
buttons keep their own action areas. Colours use a 20% exponentially weighted moving
average and require two consecutive samples in a new band before changing.
The first sample and recovery after a timeout use the fresh value immediately.

AxSSH sends a reply-requesting keepalive on the existing authenticated SSH
connection immediately after connection, then every 10 seconds for the focused
session in the active window and every 30 seconds for other sessions. This
measures SSH response time, including transport and server processing; it does
not use ICMP, open another connection, or execute a remote command. A probe
that exceeds 5 seconds turns the dot red without declaring the connection lost.
Only one probe remains outstanding; sampling resumes after its late response.
Unmeasured, unavailable, or 90-second-old data is grey. Latency is kept in memory
and cleared on reconnection. Local Shell, Telnet, and Serial keep their ordinary
green connected dot. In a split workspace the visible Tab dot belongs to its
root session; each pane samples according to its own focus.

Workspace Tabs participate in Tab navigation; `Enter` or `Space` activates the
focused Tab.
When switching between host-backed workspace Tabs, the matching server row in
the sidebar becomes selected and highlighted. Non-host Tabs leave the sidebar
selection unchanged, so terminal and editor keyboard input keeps its focus.
Drag a workspace Tab to reorder it. Its leading number changes with its current
position, while an instance suffix such as `#1` remains unchanged.
Use **Window > Previous Tab** / **Next Tab** to cycle through that current order;
the selection wraps at either end. The fixed shortcuts are `Cmd+Shift+[` /
`Cmd+Shift+]` on macOS and `Ctrl+Shift+[` / `Ctrl+Shift+]` on Windows and Linux.
They are available when at least two Tabs are open and are temporarily disabled
while recording a shortcut or answering a security prompt.
When activation moves to a Tab outside the visible strip, the Tab bar scrolls
that Tab into view automatically.
Activating a connected Terminal Tab restores its native input focus after the
Tab layout update, so the next keystroke is ready for the selected session.
During connection, the new terminal page already takes focus, so Enter does
not repeatedly reopen the sidebar server. Input typed while waiting is discarded
and is never replayed after login. Restored history also reflows to the actual
pane width before the connection completes. Explicitly activating a server
again still opens an independent connection.
Returning to AxSSH from another application or window also restores the active
Terminal Tab's input focus automatically (unless a blocking dialog is open).
Moving between existing split panes focuses the destination immediately.

To make a connected Terminal and any terminal panes in its current workspace a
separate native window, use the overlapping-window button on a connection Tab or
choose **Window > Move Current Workspace to New Window**. The terminal panes and
their SSH/SFTP companions move as one workspace group and keep existing terminal
output, SFTP directory state, transfers, host-key prompts, and authentication
phases. SSH, Telnet, and Serial tabs automatically reconnect after an unexpected
disconnect using at most five attempts with 1, 2, 4, 8, and 16 second backoff
(capped at 30 seconds). Connection errors and retry prompts for the active
pane appear in a centered card in the current window; its details can scroll
without blocking other Tabs. A detached Terminal window shows only its
terminal panes, while a detached SFTP view shows only SFTP. In the detached
macOS window, the native title bar shows the connection's system icon and matches
the active Terminal or SFTP surface
and its overlapping-window return icon merges the same workspace layout back. Hovering
the icon shows **Return workspace to main
window**. Closing the detached window performs the same merge and
leaves workers running. Settings and session-editor Tabs remain in the main
window.
The main and detached macOS windows share the same split and overlapping-window
icon artwork. Their split icons and button spacing match; detached
buttons have no system bezel, and their icon colors update with the AxSSH theme.
Windows/Linux use the shared return icon in their detached control row too.

Application popup menus follow the selected AxSSH palette on Windows, Linux,
and macOS, including terminal, session, SFTP, and text-edit menus. Use Up/Down
or Home/End to select an enabled action, Enter/Space to activate, and Escape
or a click outside to close. Recent workspaces open a submenu on hover, click,
or Right; Left returns to the parent menu. Long menus scroll. Secret inputs
continue to offer Paste only. The macOS screen menu retains the system appearance.

Text fields in SFTP, Settings, session editors, and dialogs use the platform's
standard editing shortcuts: `Cmd+A/C/X/V/Z` on macOS and `Ctrl+A/C/X/V/Z` on
Windows/Linux. macOS also supports `Ctrl+A/E` for the beginning/end of the
current paragraph (the whole field for single-line input), `Ctrl+B/F` for
backward/forward movement, and `Ctrl+H/D` for backward/forward deletion.
Multiline fields support `Ctrl+P/N` for the previous/next visual line. Shift
extends ordinary selections for these movement commands. Password fields
permit navigation and paste while blocking select-all, copy, and cut; Control
movement never extends a password selection. Read-only fields permit movement
but block deletion. Added Control actions wait until IME composition ends.
These field shortcuts do not change terminal key handling.

On Windows and Linux, the main window's menu shares the top row with the
window controls; there is no separate visible system title bar. Drag the empty
part of that row to move the window, or double-click it to maximize or restore.
The right-hand controls show a minimize line, a maximize window (overlapping
windows when maximized, to restore), and a close cross. These vector icons
remain visible when the application font changes. Hover for the action name,
or focus a control with Tab and activate it with Enter or Space.
The detached window uses the same frameless controls and places Return at the
left of its top row. Closing it still merges its workspace into the main
window. Its top-row blank area also moves that detached window. The Tab-row
blank area never moves a Windows or Linux window. The detached top row shows
the system icon and connection name. It reuses manual choices and existing
discovery results without probing again; Local Shell uses its host icon or saved
override. In a split Terminal, both title and icon follow the focused pane.
Changes to a connected profile's icon or arriving system information update the
title bar too. Its colors follow the same configuration as Tab icons, and hover
shows the connection name and system information. Window titles remain
available to the taskbar and window switcher.

The terminal Tab toolbar has two controls beside the saved-connection button.
On macOS, a detached Terminal window places the same pair in its native title
bar immediately before the Return icon, leaving its client area for full-height
panes. The left control splits vertically and opens a new pane to the right,
while the right control splits horizontally and opens a new pane below.
They always act on the active terminal pane. Splitting does not add another
top-level Tab: one visible Terminal Tab owns the complete pane layout. Use `Alt+H`, `Alt+J`,
`Alt+K`, and `Alt+L` in a terminal to focus the left, down, up, and right
pane. Use `Alt+Shift+H`, `Alt+Shift+J`, `Alt+Shift+K`, and
`Alt+Shift+L` to create an independent terminal session on that side. Each
pane has its own local PTY or profile connection; SSH panes repeat normal trust
and authentication, including any required password or passphrase prompt. SFTP
cannot be split into a terminal pane and remains an independent visible Tab.
Each non-root pane has a small close control in its upper-right corner. Closing
it removes only that pane and its session, then collapses the remaining layout;
the root pane has no independent close control. A normal `exit` from a local or
SSH shell closes its root visible Terminal Tab and selects the following visible
Tab, or the preceding Tab if it was last; the same normal exit from a child
closes only that pane. SSH recognizes this only when the server explicitly sends
the shell channel EOF/Close. An intentional Disconnect performs the matching
close. An SSH transport disconnect without EOF/Close, or an unexpected
Telnet/Serial disconnect, keeps the Tab and terminal scrollback, shows the
current retry countdown, and retries with a fresh worker. Password SSH sessions
without a readable credential stay at an authentication prompt; encrypted-vault
sessions require an unlock. Unknown or changed host keys always stop for the
existing explicit confirmation flow. After the retry limit, the Tab remains open
with a manual recovery message.
AxSSH also reads the platform user's OpenSSH `~/.ssh/known_hosts` (or the Windows
equivalent), including aliases, non-default ports, hashed hosts, multiple keys,
and `@revoked`. A valid non-revoked match can proceed to authentication; changed
keys still require confirmation, and revoked keys are always rejected. Explicit
confirmation appends the observed key atomically without replacing unrelated
records. Read failures and malformed lines only reduce trust, and the normal
confirmation button cannot bypass a revoked record.
Connection and authentication failures remain visible for diagnosis. Closing
the visible Terminal Tab still closes every terminal pane in that layout.
When a connection attempt fails or an unexpected disconnect exhausts its retry
budget, the owning pane shows a non-blocking notice with **Retry** and **Close**;
a failed local shell offers **Restart** instead. Host-key, authentication, and
vault-unlock prompts remain the blocking security dialogs for their own Tab.

Each split has a visible divider. Drag a vertical divider to change pane widths
or a horizontal divider to change pane heights; double-click it to restore an
equal split. Dividers participate in Tab focus and accept the matching arrow
keys, Home, End, and Enter or Space for reset. Each side remains between 10%
and 90% of that split. The ratios survive Tab switching and detached-window
round trips during the current run, then return to equal splits after restart.
Terminal rows, the cursor, preedit text, and the native input proxy remain
clipped to their pane even when nested splits make one side unusually small.
Every pane anchors its first complete terminal row at the pane top. Height that
is not a whole cell stays below the final complete row instead of clipping it;
space beyond the maximum row count also remains below the grid. Only a pane
shorter than the three-row floor clips older top rows.
This clipping and top-aligned grid are established on the pane's initial layout,
not only after the first window or divider resize.
Releasing or cancelling a mouse drag returns focus to the focused terminal
pane's input target, including its waiting area while connecting;
keyboard and accessibility divider actions retain
divider focus.
The window client area has no additional application-drawn outer frame;
individual Terminal panes remain borderless, including split panes.

The terminal supports bounded scrollback, ANSI colors, text selection, native
input methods, F1-F24, and common xterm-style control and navigation sequences.
Character input keeps the layout-resolved text produced by the native event;
Shift punctuation is not reconstructed from a hard-coded US layout. Physical
numeric-keypad events follow the same standard text or logical-key path as
other keyboard input; their physical identity is retained only as event
metadata. NumLock, IME, AltGr, and modified keypad input keep their platform
standard behavior. Unsupported modified Escape, Tab, or Return combinations do not emit
private Kitty/CSI-u or `modifyOtherKeys` sequences.
Standard terminal events are preserved: OSC 0/2 can update the runtime Terminal
Tab title, CSI 22/23 restores the title stack, and BEL produces a short visual
hint. Dynamic titles are runtime-only and are not saved in profiles or workspaces.
F13-F16 use the xterm shifted-F1-F4 forms (`CSI 1;2P` through `CSI 1;2S`),
and F17-F24 use the standard extended tilde forms. The terminal answers
`CSI 14 t`, `CSI 16 t`, and `CSI 18 t` from measured text-area metrics; it does
not fabricate `CSI 13 t`, `CSI 15 t`, or `CSI 19 t` without window/screen geometry.
OSC 8 links use a bounded URI and can open only explicit HTTP(S) targets; other
schemes are inert. Remote OSC 52 clipboard writes are disabled by default. To
allow a remote program to copy text into the local default clipboard, enable
**Settings > Terminal > Allow remote OSC 52 clipboard access**. Remote writes
use only the default clipboard and are limited to 64 KiB of decoded text. A
remote read shows an Allow/Deny notice for that tab; Allow reads only the
default clipboard (maximum 64 KiB), while Deny, a 20-second timeout, disabling access, disconnect, retry, or tab
close cancels the request. Selection clipboard access and image protocols stay
disabled.
The terminal deliberately leaves unsupported reports unanswered: DA3/tertiary
device attributes, DSR queries other than status and cursor position, and
window-position/full-screen queries cannot be answered without corresponding
device or geometry data. Kitty keyboard/CSI-u, `modifyOtherKeys`, Sixel, Kitty
graphics, and iTerm2 inline images remain off. Telnet reports `xterm-256color`
through TTYPE and sends NAWS only after negotiation; Serial is a raw byte stream
with no TERM/PTY/NAWS contract.
When a normal terminal grows taller, actual scrollback may become visible above
the current viewport. If no history is available, existing output stays at the
top and the added blank rows remain below it.
Full-screen programs that enable xterm mouse reporting receive clicks,
releases, wheel events, drag motion, and cell motion using their selected SGR,
UTF-8, or legacy encoding. **Standard xterm mouse routing** is the default:
reporting receives ordinary clicks, releases, drags, and motion; `Shift`
bypasses reporting for local selection, and `Alt`/`Option` is only a reported
modifier. Enable **Local selection priority** in **Settings > Terminal** when
left-dragging should select local text while `Alt` (`Option` on macOS) sends a
button gesture to the TUI. A stationary single left click is still forwarded to
a reporting TUI; once the pointer moves, the gesture remains a local selection.
Wheel events go to the TUI while reporting is active in both modes; `Shift` +
wheel scrolls local history. Without mouse reporting, an alternate-screen
program can still receive wheel movement as Up/Down keys through alternate
scroll. For example, tmux with `mouse off` lets `Ctrl+B`, `Page Up` enter copy
mode, then the wheel moves its copy-mode cursor without any tmux-specific AxSSH
setting. **Settings > Terminal > Mouse > Keep cursor at line start while scrolling**
is off by default. Enable it to send one unmodified Home after each Up/Down
key produced by a wheel step, requesting line start on the destination line.
Multiple steps repeat the complete `Up/Down + Home` pair. This applies to all
alternate-screen programs, including editors, and Home follows the current
application-cursor mode. The program's key bindings determine the final cursor
position; this setting does not change keyboard input or locally reposition
the displayed cursor.
Programs with mouse reporting enabled continue to receive mouse events without
the extra Home.
The setting takes effect immediately and is saved when Settings closes.
These mouse and wheel rules apply in both the main window and detached
Terminal windows, including split panes. On the normal screen, direct left-drag
selection and wheel scrolling remain local. A pointer gesture keeps
the owner chosen at button press through release or cancellation, including
motion outside the grid, so one drag is never handled by both AxSSH and the
remote program. Release reports the current modifier state; cancellation uses
the last observed pointer state. High-frequency motion keeps only the newest
cell per display frame, while the final motion is flushed before release.
When reporting does not own the gesture, left-button double-click selects a
complete valid HTTP(S) URL first, including across soft-wrapped cells while
omitting trailing terminal punctuation. Otherwise it selects one semantic
terminal word using the terminal core's punctuation, whitespace, wide-character,
and matching-bracket boundaries. The range stays local to the pane and is not
sent to the remote program. Shift bypasses this behavior, and Cmd/Ctrl target
activation and remote mouse reporting retain priority. The existing
copy-on-select preference also applies to the resulting URL or semantic
selection. A third left click in the same short click sequence selects the
complete logical terminal line, including soft-wrapped cells; the terminal core
provides the line boundaries. The line selection remains local and uses the same
reporting, modifier, focus, refresh, and copy rules as the semantic selection.
Alternate scroll is enabled by the terminal's default mode; DEC mode 1007 can
explicitly toggle it, and it never takes ownership of button gestures. Moving
focus to another pane, a divider, or another window
control clears a terminal selection; an actual grid resize or an effective local
scroll clears it when the coordinates no longer describe the same viewport.
Screen output refreshes update the grid without cancelling the selection, and
**Copy** reads the latest cells under the selected coordinates. The terminal
context menu keeps a stable selection long enough to use **Copy**. While reporting is active, standard mode uses
`Shift` + right-click for that local menu; Local selection priority uses an
ordinary right-click, while `Alt`/`Option` + right-click remains remote.
Home and End follow application-cursor mode in full-screen programs. Plain
`Ctrl+C` is sent to the active terminal as an interrupt. With a Terminal Tab
active, **Edit > Copy**, **Paste**, and **Select All** affect only the focused
terminal pane. Default Copy/Paste shortcuts are `Cmd+C` / `Cmd+V` on macOS and
`Ctrl+Shift+C` / `Ctrl+Shift+V` on Windows and Linux; these can be changed in
Settings. Select All is fixed to `Cmd+A` on macOS and `Ctrl+Shift+A` elsewhere.
Plain `Ctrl+A`, `Ctrl+C`, and `Ctrl+V` remain terminal input on Windows/Linux.
The same keyboard shortcuts work in a detached Terminal window. On macOS, the
screen menu follows the active window, so Copy/Paste/Select All act on that
window's focused pane. Switching between main and detached windows keeps each
command with its window, for both newly detached and restored workspaces.
Main-window management actions are disabled in a detached window; the macOS
application menu's Settings/About commands open their pages in the main window.
Blocking dialogs disable terminal menu actions. Non-secret text fields retain
their native editing shortcuts and themed context menus; secret fields allow
paste but remain non-copyable.
Hold `Cmd` on macOS, or `Ctrl` on Windows/Linux, and click a visible terminal
target to open it. While that modifier is held, the complete recognized URL or
path is underlined; the indicator clears when the modifier is released or text
selection begins. AxSSH recognizes `http://` and `https://` URLs and opens them
with the local default application; it does not fetch them itself. It also
recognizes remote Unix paths beginning with `/`, `./`, or `../`, including
diagnostic locations such as `/srv/app/main.rs:42:7`. A path activates its SSH
Tab's SFTP companion and navigates to that location. If no companion exists,
AxSSH opens a separate SFTP Tab at that initial location through the ordinary
SSH host-key and authentication flow. Relative paths use the current SFTP
directory when an already-open companion is available. If the companion is
still completing host-key confirmation, authentication, or browser startup,
the path is held only on that runtime Tab and applied when the normal flow is
ready.
Textual URLs use the same range for double-click selection and modifier-key
activation. Chinese sentence punctuation and quotation marks end that range:
`https://www.rfc-editor.org/rfc/rfc9110.html#section-15)。HTTP` selects only the
URL through `#section-15`. Balanced ASCII parentheses, IPv6 brackets, Unicode
hosts/paths, query parameters, and fragments remain supported. If a URL needs
literal Chinese prose punctuation, percent-encode it. Explicit OSC 8 links keep
the URI supplied by the program.
Optional semantic highlighting uses regex rules for standard log severity names
and HTTP status codes with explicit context. It is disabled by default.
`TRACE`, `DEBUG`, `INFO`, `NOTICE`, and `INFORMATIONAL` use Information;
`WARN`/`WARNING` use Warning; `ERROR`, `FATAL`, `CRITICAL`, `ALERT`, and
`EMERGENCY` use Error. Names match complete ASCII words without case sensitivity.
These names follow [OpenTelemetry log levels](https://opentelemetry.io/docs/specs/otel/logs/data-model/#severity-fields)
and the [syslog severity table](https://www.rfc-editor.org/rfc/rfc5424#section-6.2.1).
HTTP codes are recognized in response summaries such as `HTTP/1.1 200 OK` or
`HTTP/2 200`, Common/Combined access-log request fields such as
`"GET / HTTP/1.1" 404`, and explicit attributes such as
`http.response.status_code=503`, `http.status_code=503`, or `http_status=503`
(`http_status_code` also works). HTTP `1xx`/`3xx` use Information, `2xx` Success,
`4xx` Warning, and `5xx` Error. The categories follow
[HTTP semantics](https://www.rfc-editor.org/rfc/rfc9110.html#section-15);
the color mapping is an AxSSH preference. Bare numbers, IP octets, and ordinary
state words such as `OK`, `connected`, `pending`, and `timed out` are not colored.
Recognition is bounded to plain ASCII text runs of at most 512 bytes, without
joining different style runs or wrapped screen rows.
URLs and paths use the separate Cmd/Ctrl target interaction above: they stay at
their normal foreground and receive only a temporary underline while the
platform primary modifier is held. When semantic highlighting is enabled, its
status colors follow the selected terminal palette unless Settings overrides a
category; explicit ANSI or true-color foregrounds are never replaced. Non-default
backgrounds, inverse, dim, and hidden styles also keep the program's styling.
In **Settings > Terminal**, **Local selection priority** is an opt-in
selection-first interaction; standard xterm mouse routing remains the default.
**Copy selection on select** is disabled by default.
When enabled, completed pointer selections and **Select All** copy immediately,
and a direct right-click always pastes.
The default **New Server** shortcut is `Cmd+N` on macOS and `Ctrl+N` elsewhere.
The default File-menu transfer shortcuts are `Cmd/Ctrl+Shift+I` for import and
`Cmd/Ctrl+Shift+E` for export of the selected group or server. macOS displays
these shortcuts as native menu accelerators; Windows/Linux retain the same
shortcuts with the custom menu titlebar. They are temporarily disabled while
recording a shortcut or answering a security prompt.

On macOS, Option continues to enter native characters, dead keys, and IME text
by default. In **Settings > Terminal**, enable **Option acts as Meta** only when
you want Option-modified keys sent as Escape-prefixed terminal Meta input.
Windows/Linux Alt behavior remains terminal Meta input, while local keyboard
layouts can submit AltGr characters through the text-input path.

On macOS, Settings and About are in the standard AxSSH application menu, and
the Settings item follows its configured shortcut. On
Windows and Linux, Settings is under Edit and About is under Help. Settings
contains General, Appearance, Terminal, X11, Workspace, Shortcuts, and About pages.
The Shortcuts page lists both configurable shortcuts and fixed platform shortcuts such as
Select All, Previous Tab, and Next Tab.
The search field above the detail area finds category names, setting titles, and
descriptions across all pages. Select a result to open its category. Every
category detail area scrolls independently when its content exceeds the window.
Changes take effect immediately in the current application and are persisted
when the Settings tab is closed with its tab `x` control, except renderer
selection, which is applied only after restart. Using the Settings shortcut
while that tab is already open activates the existing singleton Tab.
About identifies
AxSSH as `GPL-3.0-only` and includes Slint's standard, clickable attribution.
The About page also provides **Report a bug**, **Open log folder**, and **Copy
diagnostics** actions. The first opens the AxSSH issue tracker, the second opens
the local rolling-log directory, and the third copies only version, build
revision, operating system, architecture, and build profile. No diagnostic
action uploads data.
**Settings > General**
selects **Follow system**, **English**, **Deutsch**, **Español**, **Français**,
**English (United Kingdom)**, **日本語**, **한국어**, **Português (Brasil)**,
**Русский**, **Українська**, **简体中文**, or **繁體中文** for the AxSSH
interface, and also selects the default backend for a password you choose to
remember on a future connection. Language changes are persisted before they are
applied and update the main and detached windows immediately; a failed save
keeps the previous selection. **Follow system** uses Simplified Chinese for
`zh`, `zh-CN`, `zh-SG`, and `zh-Hans`, and Traditional Chinese for `zh-TW`,
`zh-HK`, `zh-MO`, and `zh-Hant`. It selects the matching bundled language for
other supported system locales, including regional French, Spanish, German,
Japanese, Korean, and Portuguese locales. Portuguese uses the Brazilian variant;
unsupported languages fall back to English.
Russian and Ukrainian systems select **Русский** and **Українська** respectively.
`en-GB` selects British English, while other English regions keep the original
English option. Interface options represent languages and regional variants;
`uk` is the Ukrainian language code, not the British English locale.
AxSSH translates its application-owned Slint interface; remote terminal content, user-provided
names/paths, logs, and runtime technical error details remain unchanged.
The SFTP page owns the transfer filter described above, the single-file maximum
(0 for no client-side file-size cap, or 1-1024 GiB; default 20 GiB), and a
per-connection upload-plus-download bandwidth cap (0-1024 MiB/s; 0 means
unlimited). In an SSH server's editor,
**Use server-specific limits** overrides the global file-size, bandwidth, and
per-tab concurrency values for new SFTP connections to that server. The
process-wide simultaneous-upload limit remains global. A changed policy takes
effect for a connection when its SFTP worker starts or reconnects; it does not
interrupt transfers already running.
The unlimited transfer-size setting applies to normal streaming uploads and
downloads, including a remote file dragged to Finder. The separately
configurable local-open snapshot limit affects only the cache copy used to
open a local file from the SFTP pane, not those transfers.
**Remote edit file limit (MiB)** separately controls remote Edit downloads and
external save uploads (default 4 MiB, range 1–64 MiB). The next download or local
save check uses the changed value without reconnecting; queued network operations
retain their submitted limit. Lowering it preserves existing copies and suspends
oversized saves; raising it triggers another check. Server-specific transfer
policies and the local-open snapshot limit do not override this setting.

In **Settings > Appearance**, Font family changes the application interface
without changing terminal cell metrics. Renderer selects **Automatic**, **GPU**,
or **Software** for the next application launch: Automatic uses GPU/Skia on
macOS and software on Windows/Linux; GPU uses Skia and Software uses the
software renderer. `SLINT_BACKEND` overrides this saved choice for the current
process. On macOS, **Software presentation** selects the CPU renderer's
presentation path for the next launch: **Damage backing store (default)** uses
the lower-CPU `CALayerDelegate` damage invalidation path, while **Layer images
(fallback)** keeps the independently owned `CGImage` compatibility path. This choice is
ignored when the active renderer does not use the macOS software surface and
does not change the FPS policy. Display mode selects **Follow system**,
**Light**, or **Dark**. Color palette independently selects **AxSSH**,
**Solarized**, **Arctic**, **Tokyo**, **Ember**, **Forest**, or **Custom**, so
every fixed palette can be used in both Light and Dark modes. Arctic is a cool
technical palette, Tokyo is a night-oriented palette, Ember is warm, and Forest
uses green contrast. In Dark mode, all palettes share axshell's vivid ANSI-16
colors while retaining their own terminal background, default foreground, and
selection colors; Light mode retains the readable light ANSI palette.
Custom exposes separate Light and Dark semantic colors. During the immediate
preview and on persistence, invalid hex values or colors that would hide text,
essential borders, focus/status states, or terminal text are replaced with
readable defaults for that side.

**Compact terminal rendering** is enabled by default and removes redundant run
wrappers and default-background items. Disable it only to compare the legacy item
tree. **Cache unchanged terminal rows** is disabled by default; on GPU/Skia it can
reuse static row layers at the cost of additional graphics memory. It does not
provide an equivalent cache in Software mode. Both choices preview immediately
and are persisted when the Settings tab closes.

**Focused refresh rate** and **Unfocused refresh rate** in the same Rendering
section set terminal presentation caps on every renderer in frames per second. They
accept 1-120 FPS and default to 60 FPS for the focused pane and 4 FPS for a
visible pane that is not focused. The non-software focused strategy still adapts
sustained output through its 16/33/50 ms stages; a lower configured cap limits
those stages. Software follows the configured focused rate without the adaptive
reduction. The first focused output after an idle period uses a short frame
window, at most about 16.7 ms, to merge nearby redraw fragments and cursor
restoration. Unrelated UI refreshes retain the admitted terminal frame until the
next output frame is ready. Longer gaps can still expose intermediate positions
unless the application uses synchronized output. When an AxSSH window loses
native activation, all visible panes use the Unfocused refresh cap, including the
pane that was last focused. Hidden tabs do not publish terminal frames, and
parser responses, errors, disconnects, and shutdown remain immediate. Changes
preview immediately and are persisted when the Settings tab closes.

**Software block rows** in **Settings > Terminal** controls the macOS CPU
presentation block height for terminal panes. It accepts 1-16 terminal rows and
defaults to 4. The backend aligns pane presentation boundaries to that many
measured row heights; sidebar and tab areas remain on the fallback presentation
grid. This is a presentation hint only and does not limit FPS or change terminal
content, input, or the Slint row model. Changes preview immediately and are
persisted when the Settings tab closes.

**Blink terminal cursor** in the same Rendering section is enabled by default.
Disable it to keep the focused terminal cursor continuously visible; this does
not change terminal cursor visibility, IME input, or selection behavior. The
choice previews immediately and is persisted when the Settings tab closes.

**Settings > Terminal** independently controls the Terminal font, font size,
line height, software presentation block rows, text brightness, bright ANSI
colors for bold text,
optional semantic highlighting, scrollback, mouse behavior, and the platform-specific Option-as-Meta
preference. Text brightness ranges from 60% to 120% in 5% steps and defaults to
100%, which preserves resolved ANSI/256/true-color foregrounds exactly. Rendering
first resolves the program color and inverse state, then an enabled semantic
foreground, and finally applies the brightness once to the visible text. `dim`
participates in that same final adjustment. Backgrounds, selection, and the cursor
are not adjusted. When semantic highlighting is enabled, enter an opaque `#RRGGBB`
for Success, Information, Warning, or Error; leave a field empty to follow the
active terminal palette. URL/path target colors are not configurable because the
target indication is an interaction-only underline. Bundled fonts appear before
discovered system monospace fonts in both font lists. The selected Terminal font remains the
primary family. When it lacks a Han glyph, AxSSH tries bundled Maple Mono NF CN
first; Iosevka Term then supplies missing symbols such as `↴`. Choosing another
Terminal font does not replace the saved selection or change this fallback order.

**Settings > X11** controls the platform-local X server provider, first-X11-
application startup behavior, and the explicit loopback-only no-auth
compatibility mode. Known provider locations are detected and shown read-only;
an application path is available only for Custom and must name an executable
file. These settings are non-secret; X11 cookies remain transient and are never
saved.

## Local data and credentials

AxSSH stores profiles, non-secret group names, and settings in a versioned
`sessions.json` inside the platform application configuration directory. On
Linux this normally resolves to `~/.config/ax_ssh/sessions.json` while respecting
`XDG_CONFIG_HOME`; macOS and Windows use their standard application directories. Each
profile contains one explicit SSH, Telnet, or Serial configuration. Only SSH
may contain a confirmed host-key fingerprint, private-key path, or non-secret
reference to the backend holding a remembered password, plus the non-secret
per-profile X11 forwarding mode. X11 cookies are never stored. A Serial profile may
store non-secret USB identity metadata for stable matching. Profiles do not
contain passwords, vault passwords, private-key passphrases, private-key
contents, terminal output, or live process state.

Remembered passwords are stored through macOS Keychain, Windows Credential
Manager, or Unix Secret Service only when **System credential store** is
explicitly selected. The **Encrypted application vault** is the default and
stores one private encrypted record per profile in the application
configuration directory. When its vault password is left blank, a random
unlock key is held in the protected system credential store and is never
shown; a user-entered vault password is never saved. System authorization may
be required. Legacy local unlock files migrate only after a protected save
succeeds; a backend failure preserves the source and is reported. Daily logs are written separately to the `logs` subdirectory of the
platform-local application data directory, with at most 15 files retained. `RUST_LOG`
overrides the default `ax_ssh=info,russh=warn` filter. Credentials and terminal
contents must not be logged. Operational logs may still contain connection
metadata such as a host, port, session ID, or host-key fingerprint, so review
them before attaching them to an issue.

To manage one server's password, open its editor and check **Authentication >
Saved password**. This shows the backend referenced by the profile, without
reading the password. Select **Clear saved password** and confirm **Clear** to
remove it immediately from that backend and clear its saved-password reference.
An application-vault record's protected automatic unlock entry and any legacy
local unlock file are removed too. Canceling
the confirmation leaves the saved password intact. Other unsaved editor fields,
trusted host keys, private-key files, and connected terminals are preserved.
If writing the profile fails, AxSSH restores the removed credential. You can
save a replacement by entering a new password and selecting **Save password
(optional)**. An externally removed record is detected on the next connection;
the editor status describes the profile reference rather than enumerating the store.

System-password entries use service `com.axsoft.ax_ssh` and account
`session:<profile UUID>`, independent of the server's display name or endpoint.
On macOS, open Keychain Access and search for the service, then inspect the
account to match it to the profile's `id` in `sessions.json`. The default
application vault uses `credentials/<UUID>.json` under the application
configuration directory. Its automatic unlock key uses system account
`vault-unlock:<UUID>` under the same service. Existing local
`credentials/<UUID>.unlock` files are migrated into that protected account
and removed only after saving succeeds; existing keyring entries remain valid.
AxSSH does not enumerate or clean unreferenced system entries. Use the editor
before removing a profile's UUID or credential reference manually; editing JSON
alone does not trigger credential deletion.

A connected terminal may legitimately have no shell output. AxSSH uses SSH
transport keepalive and inactivity policy, not a shell-output timer, to decide
when such a connection is no longer live.

## Current limitations

In-app known-hosts administration beyond the shared parser,
cross-process edit recovery and three-way merge remain planned work. Serial availability, device permissions, and supported parameter
combinations depend on the target operating system and hardware; Telnet has no
encryption or automated login.

## Clipboard images and voice for terminal agents

Paste uses the platform clipboard with visible errors. Text is sent through the existing bracketed-paste path (4 MiB maximum). If an image is present, AxSSH encodes a PNG in the background (at most 4096 × 4096 pixels and 16 MiB). In an SSH terminal it uploads over a separate SFTP channel on the same authenticated connection, then inserts the remote path. Local Shell inserts a local temporary path. Telnet and Serial report that image attachments are unavailable. SFTP must be enabled on the server. The image is an attachment file; it does not enable inline terminal image protocols. A CLI must support a pasted image path or an image argument such as `codex -i <path>`; inserting a path alone does not guarantee attachment recognition in every CLI.

Attachment directories/files use `0700`/`0600` on Unix and the user's inherited temporary-directory ACL on Windows. Each session retains at most 32 attachments and removes its own files on close. Remote cleanup is best effort after transport loss; an interrupted connection can leave `.axssh-attachment-*` in the remote home directory. OSC 52 remains text only, off by default and subject to individual read confirmation. Its read and write limits are both 64 KiB; disabling it also revokes pending requests.

**Settings > Terminal > Microphone** saves the remote sample rate and channel count. Click **Save audio settings**; new SSH connections use the saved format. Remote PCM is signed 16-bit little-endian, with 8/16/22.05/24/32/44.1/48/96/192 kHz and 1/2/4/6/8 channels; default is 48 kHz stereo. AxSSH prefers matching hardware, otherwise resamples and adapts channels. Mono averages input channels; extra output channels repeat existing channels.

Use the CLI's own recording action. Local Shell programs use the operating system's native audio interface directly. For Linux SSH terminals, AxSSH prepares a unique PulseAudio/pipewire-pulse input after authentication, then observes standard recording streams attached to that input. A non-paused stream starts local capture; closing or pausing all its recording streams stops capture. CLI names, commands and terminal text do not affect this behavior. There are no manual microphone/dictation buttons or transcript panel. Speech recognition belongs to the CLI; AxSSH does not require whisper-cli or a speech model for this flow. [PulseAudio recording API](https://github.com/pulseaudio/pulseaudio/blob/master/src/pulse/def.h)

The remote Linux user needs a running PulseAudio or pipewire-pulse service, `pactl`, and `module-pipe-source`; AxSSH does not install them. The source uses a private FIFO and a separate authenticated SSH channel. AxSSH requests `PULSE_SOURCE` for the shell and temporarily selects its source as the user's default input, covering servers that reject environment requests. This fallback affects other applications using that remote user's default input. Programs choosing a different source or reading hardware directly do not use the bridge; ALSA `default` must route to Pulse/PipeWire. An unavailable audio service leaves the normal SSH terminal usable. A failure after recording starts is reported through the connection status; reconnect after fixing permission/device issues to prepare a fresh input. [PipeWire pipe source](https://pipewire.pages.freedesktop.org/pipewire/page_pulse_module_pipe_source.html), [PulseAudio FAQ](https://wiki.freedesktop.org/www/Software/PulseAudio/FAQ/)

At most one AxSSH bridge owns the local microphone at a time. Idle terminals do not request microphone permission or open the device. Closing, disconnecting or retrying stops the bridge, unloads its module and removes its FIFO. The old default is restored only if the default still points to this bridge, preserving external changes. A hard crash or lost transport can leave remote resources requiring cleanup.

On macOS, run the packaged `.app` and grant microphone permission when the CLI starts recording; after denial, enable AxSSH in **Privacy & Security > Microphone**. Windows desktop microphone access must be enabled in system privacy settings. Linux needs a usable local ALSA input. Audio forwarding still requires the CLI to support the system audio interface used on the remote host.

For acceptance, check both main and detached terminals have no idle media row; use a standard recording client to open, pause, resume and close the generated input. Check mono/stereo and higher-rate formats, permission denial and disconnect during capture/upload. Confirm that session close removes the owned source/FIFO and preserves any externally changed default. Native Windows/Linux CI and real devices/remote services remain required checks.
