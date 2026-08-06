# Worker Registry

## Current Goal

- Goal: none
- Goal ID: none
- Tracker: `docs/project-implementation-tracker/current.md`
- Tracker Step: none
- Coordinator: main-thread
- Last Updated: 2026-08-06 16:40 +0800

## Reuse Rules

- Reuse only when task context matches (`goal_id` or `task_slice`) and there is at least 1 strong + 1 weak signal, unless exact `owned_paths` + `deliverable` match allows direct reuse.
- Prefer reachable live agents over file-only records.
- Do not use wide `owned_paths` as the main reuse signal for research/chat/no-file tasks.
- Exclude `reuse_hint=do-not-reuse` rows from normal scoring unless the current request explicitly asks to resume them.
- Mark uncertain liveness as `suspected-stale` before replacement; promote to `stale` only after explicit checks.

## Agents

No active workers. The multi-agent implementation was closed after the P1-P9 deliverables were integrated.

| name | agent_id | status | goal_id | task_slice | responsibility | owned_paths | workstream | execution_lane | worker_class | reuse_hint | deliverable | deliverable_kind | task_mode | dependency_boundary | session_id | reachability | last_heartbeat_at | last_checked_at | ttl_hint | progress_marker | overlap_keywords | notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |

## History

| name | agent_id | final_status | goal_id | task_slice | summary | closed_at | notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| coordinator | main-thread | completed | 20260806-sftp-icons-local-open | P1,P4,P6-P9 integration | Integrated the icon provider, local snapshot opener, remote transfer worker, UI bridge, bilingual docs, tracker updates, and verification. | 2026-08-06 16:10 +0800 | Strict Clippy remains blocked by pre-existing baseline lints; target-platform GUI and real SFTP acceptance remain manual. |
| icon-worker | /root/icon_worker | completed | 20260806-sftp-icons-local-open | P1-P2 icon provider | Implemented bounded `src/app/file_icons.rs` with platform resolvers, cache identity, LRU eviction, prewarm limits, native-handle cleanup, and fallbacks. | 2026-08-06 16:10 +0800 | Owned path was limited to the icon provider. |
| sftp-ui-worker | /root/sftp_ui_worker | completed | 20260806-sftp-icons-local-open | P3,P7 Slint UI contract | Added icon slots, regular-file double-click callbacks, bounded transfer DTO/list, progress counters, and cancellation callback wiring in `ui/sftp-pane.slint`. | 2026-08-06 12:36 +0800 | Main thread completed Rust ABI wiring and validation. |
| transfer-worker | /root/transfer_worker | completed | 20260806-sftp-icons-local-open | P5 transfer domain | Implemented chunked SFTP download, metadata/path checks, private cache publication, cancellation, cleanup, permissions, and loopback regressions. | 2026-08-06 12:56 +0800 | Main thread completed worker scheduling and UI opener dispatch. |
| worker-review | /root/worker_review | errored | 20260806-sftp-icons-local-open | P9 final static review | The review attempt did not execute because the selected model was at capacity; the coordinator completed the final static review. | 2026-08-06 16:40 +0800 | Nested platform review failed for the same capacity reason; target-platform validation remains manual. |
