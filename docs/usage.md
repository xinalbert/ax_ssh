[简体中文](usage.zh.md) · [Project README](../README.md)

# Using AxSSH

## Start the application

AxSSH requires Rust `1.92.0` or newer and a desktop environment supported by
Slint's winit backend. From the repository root, run:

```bash
cargo run --locked
```

## Create and connect to a session

1. Choose **File > New Session**, or right-click blank sidebar list space and
   choose **New Server**.
2. Enter a name, optional group, host, port, and username. Select password or
   private-key authentication. Private keys discovered in the user's `.ssh`
   directory can be selected, or a path can be entered manually.
3. Save the session, then select it in the session navigator. Opening the same
   saved session more than once creates independent terminal tabs with separate
   connections and output.
4. On the first connection, compare the displayed SHA-256 host-key fingerprint
   with a trusted source before confirming it. A changed key requires another
   explicit confirmation and should be investigated before acceptance.
5. Enter a transient password or private-key passphrase when prompted. A
   password can optionally be remembered in the platform credential store; a
   private-key passphrase is never persisted.

Right-click a group to add a server, rename the group, or delete it. Right-click
Ungrouped to add a server. Right-click a server to connect, edit, or delete it;
the edit action reuses the same session editor. Changing its host or port clears
the confirmed host-key fingerprint, so the new endpoint must be trusted
explicitly on its next connection. Leaving an existing saved password blank
keeps it; clearing the save-password checkbox removes it from the platform
credential store.

Choose **Pane > New Local Shell** or the Local Shell control to open an
independent local terminal. Close a terminal with its tab control or **Window >
Close Current Tab**.

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
compact activity bar. Only the compact bar uses the first two characters of the
group name as a text badge; opening a group there expands the sidebar and that
group. The same row context menus are available in the compact rail. Deleting a
group moves its servers to Ungrouped. Deleting a profile also removes its
remembered password but does not close terminal tabs that are already open.

The sidebar masks usernames and IPv4 addresses by default: a username keeps its
first and last two characters when available, and `192.168.1.202` becomes
`192.*.202`. Change the single mask character in **Settings > Workspace**.
Hostnames remain visible so they can still be distinguished at a glance.
New-session editors and terminal sessions share the workspace tab bar, while
Settings opens as a separate workbench view. The `+` at the right end of that
bar lists every saved SSH session and connects the selected profile; **File >
New Session** and the sidebar blank-area context menu open the session editor.
Drag a workspace Tab to reorder it. Its leading number changes with its current
position, while an instance suffix such as `#1` remains unchanged.

The terminal supports bounded scrollback, ANSI colors, text selection, native
input methods, and common xterm-style control and navigation sequences. Plain
`Ctrl+C` is sent to the active terminal as an interrupt. Default clipboard
shortcuts are `Cmd+C` / `Cmd+V` on macOS and `Ctrl+Shift+C` / `Ctrl+Shift+V` on
Windows and Linux. These shortcuts can be changed in Settings.

On macOS, Settings and About are in the standard AxSSH application menu. On
Windows and Linux, Settings is under Edit and About is under Help. Settings
contains General, Appearance, Terminal, Workspace, Shortcuts, and About pages;
changes are persisted only when **Save** is selected.

In **Settings > Appearance**, Display mode selects **Follow system**, **Light**,
or **Dark**. Color palette independently selects **AxSSH**, **Solarized**, or
**Custom**, so either fixed palette can be used in both Light and Dark modes.
Custom exposes separate Light and Dark semantic colors. When saved, invalid hex
values or colors that would hide text, essential borders, focus/status states,
or terminal text are replaced with readable defaults for that side.

## Local data and credentials

AxSSH stores profiles, non-secret group names, and settings in a versioned
`sessions.json` inside the platform-local application data directory. A profile may contain a
confirmed host-key fingerprint, a private-key path, and a marker indicating
that a password is available. It does not contain passwords, private-key
passphrases, private-key contents, terminal output, or live process state.

Remembered passwords are stored through macOS Keychain, Windows Credential
Manager, or Unix Secret Service. Daily logs are written to the `logs`
subdirectory of the same application data directory, with at most 15 files
retained. `RUST_LOG` overrides the default `ax_ssh=info,russh=warn` filter.
Credentials and terminal contents must not be logged.

## Current limitations

Shared OpenSSH-compatible known-hosts storage, host-key revocation, SFTP, SSH
agent integration, reconnect, persisted workspace restoration, and complete
full-screen terminal mouse reporting remain planned work.
