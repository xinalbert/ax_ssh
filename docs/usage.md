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
   **SSH agent** authentication. **Forward X11 applications** is SSH-only and defaults to on
   for new profiles. Telnet accepts host and port and displays an unencrypted
   transport warning. Serial accepts a port name, baud rate, data bits, stop
   bits, parity, and flow control. AxSSH lists detected ports when the editor
   enters Serial mode; use **Refresh** after a device is attached, or enter a
   path/name manually.
3. Save the session, then select it in the session navigator. For a new SSH
   profile, **Save & connect** saves the profile and immediately starts the
   normal host-key flow. A password entered in the session editor is one-time by
   default and is used for that connection without requiring a vault password.
   Select **Remember password** to persist it, then choose the credential
   backend; the vault-password field appears only for **Encrypted application
   vault**. Opening the same saved session more than once creates
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
   its **Credential storage** menu can override the backend for this prompt.
   Select **Remember password** to save after successful authentication. The
   encrypted vault also asks for a vault password; later uses ask only for that
   vault password to unlock the saved SSH password.
   Password, vault-password, and passphrase fields cannot be copied, cut, or
   selected, and are cleared after a submitted secret is accepted or the prompt
   is cancelled.
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
an executable path yourself. Start for first X11 application is enabled by
default: opening an SSH shell only asks the server for forwarding and never
starts a local X server. The secure default still requires an exact
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
the shell remains connected and reports X11 as unavailable.
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
a saved password; leaving the field blank preserves it. A newly entered password
can be used once by **Save & connect**, or persisted by selecting **Remember
password** and a backend. Changing the default in **Settings > General** only
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

Enter an absolute path, a path relative to the
current directory, or `~` in the remote browser, then use **Open**; double-click
a folder to enter it. The local browser starts at the platform home directory,
accepts an explicit local directory path, and reads only bounded file metadata.
Use **Hidden** and **More** to include dot files or request the next bounded
remote page. Rows use the target platform's file-type icon when one is available
and a built-in folder, link, or generic-file icon otherwise.

Double-click a regular file in the local pane to open the current snapshot entry
with the platform's default application. Directories continue to navigate and
symbolic links are not opened. AxSSH rechecks the directory, file type, and
resolved parent outside the UI thread before asking the operating system to open
the file.

Double-click a regular file in the remote pane to download a read-only copy into
AxSSH's private cache and open that completed copy with the default application.
Directories and symbolic links are rejected. A file may be at most 512 MiB, and
each SFTP Tab runs at most two downloads at once. The Transfers area shows
progress, success, cancellation, or a bounded failure message; use its cancel
button to stop a queued or active download. Closing the SFTP Tab cancels its
pending and active downloads before closing the browser and SSH transport.
Partial or failed files are never opened. Completed cache copies are best-effort
temporary files and are removed by a later startup cleanup after they become
stale.

This first file-open phase does not provide upload, Save As, delete, rename,
drag-and-drop, modification monitoring, automatic upload, or remote edit sync.

## Workspace and terminal controls

Saved sessions are organized beneath collapsible group rows in the expanded
navigator. Right-click blank list space to create an empty group or an
Ungrouped server even before any profile exists. Expanded group rows show their
name, count, and a centered
drawn down chevron; collapsed groups use the matching drawn up chevron. This
avoids repeating the name as a badge. Select the group row, or focus it and
press Enter or Space, to change that state. Every visible server remains a
single indented row: its name is on the left and its masked endpoint is on the
right. **View > Toggle Session Sidebar** switches between this view and the
compact activity bar. The compact bar uses the first 1-4 characters of each
group name by default; choose **Full name** in **Settings > Workspace** to show
the complete group name. Full-name mode widens the collapsed rail to 180px and
uses a dense single-line list with a trailing sidebar control, labeled Local
Shell action, group disclosure and counts, and indented full server names.
Long labels are elided and available in tooltips. The same row context menus
are available in the compact rail. The most recently selected group or server
remains highlighted across
expanded and compact sidebar views; hover and keyboard focus use separate
feedback. Deleting a group moves its servers to Ungrouped. Deleting a profile
also removes its remembered password but does not close terminal tabs that are
already open.

The sidebar masks usernames and IPv4 addresses by default: a username keeps its
first and last two characters when available, and `192.168.1.202` becomes
`192.*.202`. Change the single mask character in **Settings > Workspace**.
Hostnames remain visible so they can still be distinguished at a glance.
New-session editors and terminal sessions share the workspace tab bar, while
Settings opens as a separate workbench view. The `+` at the right end of that
bar lists every saved connection and connects the selected profile; **File >
New Server** and the sidebar blank-area context menu open the session editor.
Drag a workspace Tab to reorder it. Its leading number changes with its current
position, while an instance suffix such as `#1` remains unchanged.
Use **Window > Previous Tab** / **Next Tab** to cycle through that current order;
the selection wraps at either end. The fixed shortcuts are `Cmd+Shift+[` /
`Cmd+Shift+]` on macOS and `Ctrl+Shift+[` / `Ctrl+Shift+]` on Windows and Linux.
They are available when at least two Tabs are open and are temporarily disabled
while recording a shortcut or answering a security prompt.

To make a connected Terminal and any terminal panes in its current workspace a
separate native window, use the external-link button on a connection Tab or
choose **Window > Move Current Workspace to New Window**. The terminal panes and
their SSH/SFTP companions move as one workspace group and keep existing terminal
output, SFTP directory state, transfers, host-key prompts, and authentication
phases; AxSSH does not reconnect. A detached Terminal window shows only its
terminal panes, while a detached SFTP view shows only SFTP. In the detached
macOS window, use the same-row return icon in the title bar to merge the same
workspace layout back. Hovering the icon shows **Return workspace to main
window**. Closing the detached window performs the same merge and
leaves workers running. Settings and session-editor Tabs remain in the main
window.

The terminal Tab toolbar has two controls beside the saved-connection button:
the left control splits vertically and opens a new pane to the right, while the
right control splits horizontally and opens a new pane below. They always act
on the active terminal pane. Splitting does not add another top-level Tab: one
visible Terminal Tab owns the complete pane layout. Use `Alt+H`, `Alt+J`,
`Alt+K`, and `Alt+L` in a terminal to focus the left, down, up, and right
pane. Use `Alt+Shift+H`, `Alt+Shift+J`, `Alt+Shift+K`, and
`Alt+Shift+L` to create an independent terminal session on that side. Each
pane has its own local PTY or profile connection; SSH panes repeat normal trust
and authentication, including any required password or passphrase prompt. SFTP
cannot be split into a terminal pane and remains an independent visible Tab.
Closing the visible Terminal Tab closes every terminal pane in that layout.

Each split has a visible divider. Drag a vertical divider to change pane widths
or a horizontal divider to change pane heights; double-click it to restore an
equal split. Dividers participate in Tab focus and accept the matching arrow
keys, Home, End, and Enter or Space for reset. Each side remains between 10%
and 90% of that split. The ratios survive Tab switching and detached-window
round trips during the current run, then return to equal splits after restart.
Releasing or cancelling a mouse drag returns input focus to the focused,
connected terminal pane; keyboard and accessibility divider actions retain
divider focus.

The terminal supports bounded scrollback, ANSI colors, text selection, native
input methods, F1-F12, and common xterm-style control and navigation sequences.
Home and End follow application-cursor mode in full-screen programs. Plain
`Ctrl+C` is sent to the active terminal as an interrupt. With a Terminal Tab
active, **Edit > Copy**, **Paste**, and **Select All** affect only the focused
terminal pane. Default Copy/Paste shortcuts are `Cmd+C` / `Cmd+V` on macOS and
`Ctrl+Shift+C` / `Ctrl+Shift+V` on Windows and Linux; these can be changed in
Settings. Select All is fixed to `Cmd+A` on macOS and `Ctrl+Shift+A` elsewhere.
Plain `Ctrl+A`, `Ctrl+C`, and `Ctrl+V` remain terminal input on Windows/Linux.
The same keyboard shortcuts work in a detached Terminal window even though it
has no client-area menu. Non-secret text fields retain their native editing
shortcuts and context menus; secret fields remain non-copyable.
The default **New Server** shortcut is `Cmd+N` on macOS and `Ctrl+N` elsewhere.
The default File-menu transfer shortcuts are `Cmd/Ctrl+Shift+I` for import and
`Cmd/Ctrl+Shift+E` for export of the selected group or server. Menu commands
show these configured shortcuts as native accelerators. They are temporarily
disabled while recording a shortcut or answering a security prompt.

On macOS, Option continues to enter native characters, dead keys, and IME text
by default. In **Settings > Terminal**, enable **Option acts as Meta** only when
you want Option-modified keys sent as Escape-prefixed terminal Meta input.
Windows/Linux Alt behavior remains terminal Meta input, while local keyboard
layouts can submit AltGr characters through the text-input path.

On macOS, Settings and About are in the standard AxSSH application menu, and
the Settings item follows its configured shortcut. On
Windows and Linux, Settings is under Edit and About is under Help. Settings
contains General, Appearance, Terminal, X11, Workspace, Shortcuts, and About pages.
Changes take effect immediately in the current application; they are persisted
when the Settings tab is closed with its tab `x` control. Using the Settings
shortcut while that tab is already open activates the existing singleton Tab.
About identifies
AxSSH as `GPL-3.0-only` and includes Slint's standard, clickable attribution.
The About page also provides **Report a bug**, **Open log folder**, and **Copy
diagnostics** actions. The first opens the AxSSH issue tracker, the second opens
the local rolling-log directory, and the third copies only version, build
revision, operating system, architecture, and build profile. No diagnostic
action uploads data.
**Settings > General**
also selects the default backend for a password you choose to remember on a
future connection.

In **Settings > Appearance**, Font family changes the application interface
without changing terminal cell metrics. Display mode selects **Follow system**,
**Light**, or **Dark**. Color palette independently selects **AxSSH**,
**Solarized**, **Arctic**, **Tokyo**, **Ember**, **Forest**, or **Custom**, so
every fixed palette can be used in both Light and Dark modes. Arctic is a cool
technical palette, Tokyo is a night-oriented palette, Ember is warm, and Forest
uses green contrast. In Dark mode, each fixed palette also selects its matching
Terminal ANSI colors; Light mode retains the readable light ANSI palette.
Custom exposes separate Light and Dark semantic colors. During the immediate
preview and on persistence, invalid hex values or colors that would hide text,
essential borders, focus/status states, or terminal text are replaced with
readable defaults for that side.

**Settings > Terminal** independently controls the Terminal font, font size,
line height, minimum contrast ratio, bright ANSI colors for bold text, scrollback,
mouse behavior, and the platform-specific Option-as-Meta preference. The contrast
ratio ranges from 1.0:1 to 21.0:1 and defaults to 4.5:1; 1.0:1 keeps the original
ANSI/256/true-color foregrounds. Rendering checks each cell's actual background
and changes only a foreground that is below the target, leaving backgrounds and
already-readable colors intact. Bundled fonts appear before discovered system
monospace fonts in both font lists.

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
X11 forwarding toggle. X11 cookies are never stored. A Serial profile may
store non-secret USB identity metadata for stable matching. Profiles do not
contain passwords, vault passwords, private-key passphrases, private-key
contents, terminal output, or live process state.

Remembered passwords are stored through macOS Keychain, Windows Credential
Manager, or Unix Secret Service when **System credential store** is selected.
The **Encrypted application vault** stores one private encrypted record per
profile in the application configuration directory; its vault password is not
saved. Daily logs are written separately to the `logs` subdirectory of the
platform-local application data directory, with at most 15 files retained. `RUST_LOG`
overrides the default `ax_ssh=info,russh=warn` filter. Credentials and terminal
contents must not be logged. Operational logs may still contain connection
metadata such as a host, port, session ID, or host-key fingerprint, so review
them before attaching them to an issue.

A connected terminal may legitimately have no shell output. AxSSH uses SSH
transport keepalive and inactivity policy, not a shell-output timer, to decide
when such a connection is no longer live.

## Current limitations

Shared OpenSSH-compatible known-hosts storage, host-key revocation, SFTP upload,
explicit Save As, mutation/edit sync, reconnect, persisted
workspace restoration, and complete full-screen terminal mouse reporting remain
planned work. Serial availability, device permissions, and supported parameter
combinations depend on the target operating system and hardware; Telnet has no
encryption or automated login.
