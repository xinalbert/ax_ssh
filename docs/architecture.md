[中文说明](architecture.zh.md)

# AxSSH Architecture

## Boundary

AxSSH is an independent Rust binary. The reference checkout at
`third_package/axshell` is deliberately outside the build graph. It may inform
product behavior and review questions, but no source file, type, or dependency
is imported from it.

The implementation keeps UI, application, persistence, transport, and process
services in separate ownership boundaries:

```text
Slint UI (.slint)
       │ generated callbacks / properties
       ▼
Application controller (src/app.rs)
       │ domain values + UI event-loop dispatch
       ├──────────────► Config store (src/config.rs)
       │                 JSON schema + atomic replace
       └──────────────► SSH boundary (src/ssh.rs)
                         Tokio tasks + russh handles/channels

Process startup (src/main.rs)
       └──────────────► Logging lifecycle (src/logging.rs)
                         rolling writer + flush guard
```

## Module responsibilities

| Area | Owns | Must not own |
| --- | --- | --- |
| `ui/` | Layout, visual states, user gestures, generated callback contracts | Filesystem access, Tokio tasks, russh handles |
| `src/app.rs` | Slint setup, domain-to-row mapping, callback wiring, event-loop updates | SSH protocol details or JSON schema details |
| `src/config.rs` | `SessionProfile`, validation, JSON persistence, atomic replacement | Slint types, network connections, plaintext password storage |
| `src/ssh.rs` | russh handler, host-key decision, authentication, shell channel boundary | Window updates, persistent session mutation, UI formatting |
| `src/logging.rs` | Global tracing subscriber, log directory, daily rolling writer, retention and flush guard | Credentials, feature state, UI or SSH handles |
| `src/main.rs` | Process startup and logging-guard lifetime | Feature logic |

## Event flow

1. A Slint callback produces a small value such as a session ID, draft fields,
   a trust decision, or one transient password.
2. The application controller validates and maps that value to a domain type.
   An unknown host starts a cancellable probe that records the SHA-256
   fingerprint while the transport is still rejected.
3. After explicit confirmation, the controller atomically persists the exact
   fingerprint and opens the password prompt. The password moves directly into
   one worker command and is never added to application state or configuration.
4. File operations run synchronously only for the short configuration path. SSH
   connection, authentication, health checks, and disconnect run on Tokio.
5. A worker sends bounded results back as owned values. UI updates are re-entered with
   `slint::invoke_from_event_loop` and use a `Weak<AppWindow>` so shutdown does
   not keep a window alive.

## SSH security contract

`russh::client::Handler::check_server_key` is the trust boundary. Unknown and
mismatched keys are rejected before authentication. A rejected first-contact
handshake may expose its SHA-256 fingerprint to the confirmation UI, but only
an explicit user decision adds that exact fingerprint to the profile. A changed
key requires a second explicit decision. Passwords are transient callback
inputs and are not part of `SessionStore`; private-key loading and OS keychain
integration remain follow-up work.

The authenticated connection follows this lifecycle:

- one worker owns the russh handle for its full lifetime;
- the current bounded command channel carries disconnect/cancel intent;
- bounded worker events report connected, disconnected, host-key rejection, or
  a capped error message;
- cancel interrupts connection/authentication as well as an established session;
- a 20-second keepalive with three missed-reply limit keeps healthy idle
  sessions open while retaining a 90-second inactivity bound;
- window shutdown requests disconnect, waits for the worker join with a timeout,
  and only then shuts down Tokio.

## Logging lifecycle

`src/main.rs` creates exactly one `LoggingGuard` before constructing the UI and
keeps it alive until after the Slint and Tokio lifecycles finish. `src/logging.rs`
writes through a bounded non-lossy queue to daily UTC files, retains at most 15
files, and mirrors `INFO` and higher events to stderr. Dropping the guard writes
the shutdown event, drains the queue, flushes the active file, and joins the
writer thread. Operational fields may include session ID, host, port, and host
fingerprint; credentials and terminal contents are forbidden.

## Staged scope

The current application validates and persists profiles, confirms per-profile
host fingerprints, authenticates with transient passwords, and owns one live
connection through disconnect. The following remain separate steps:

- private-key loading and OS credential integration;
- shared OpenSSH-compatible known-hosts storage and host-key revocation;
- a VT/ANSI terminal model and bounded scrollback;
- SFTP as a separate worker sharing an authenticated transport policy;
- shell channel commands, resize, reconnect, and multi-session lifecycle tests.
