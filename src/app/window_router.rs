use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use super::*;

#[derive(Clone)]
pub(super) struct WindowRouter {
    inner: Arc<Mutex<WindowRouterState>>,
}

struct WindowRouterState {
    routes: HashMap<Uuid, WindowRoute>,
}

struct WindowRoute {
    ui: slint::Weak<AppWindow>,
    transfer: Option<WorkspaceTransfer>,
    /// The stable identity shown in the workspace Tab strip.
    active_tab_id: Option<Uuid>,
    pane_trees: HashMap<Uuid, PaneTree>,
}

pub(super) struct WindowView {
    pub(super) ui: slint::Weak<AppWindow>,
    pub(super) tabs: Vec<WorkspaceTabSummary>,
    pub(super) active_tab_id: Option<Uuid>,
    pub(super) snapshot: ActiveTabSnapshot,
    pub(super) terminal_panes: Vec<WindowTerminalPane>,
    pub(super) terminal_dividers: Vec<PaneDividerPlacement>,
}

pub(super) struct WindowTerminalPane {
    pub(super) placement: PanePlacement,
    pub(super) snapshot: ActiveTabSnapshot,
    pub(super) closable: bool,
}

pub(super) struct DetachedRoute {
    pub(super) transfer: WorkspaceTransfer,
    pub(super) pane_tree: Option<PaneTree>,
}

pub(super) static GLOBAL_WINDOW_ROUTER: OnceLock<WindowRouter> = OnceLock::new();

impl WindowRouter {
    pub(super) fn new(main_ui: slint::Weak<AppWindow>) -> Self {
        let mut routes = HashMap::new();
        routes.insert(
            MAIN_WINDOW_ID,
            WindowRoute {
                ui: main_ui,
                transfer: None,
                active_tab_id: None,
                pane_trees: HashMap::new(),
            },
        );
        Self {
            inner: Arc::new(Mutex::new(WindowRouterState { routes })),
        }
    }

    pub(super) fn register_detached(
        &self,
        window_id: Uuid,
        ui: slint::Weak<AppWindow>,
        transfer: WorkspaceTransfer,
        pane_tree: Option<PaneTree>,
    ) {
        if let Ok(mut router) = self.inner.lock() {
            let mut pane_trees = HashMap::new();
            if let Some(pane_tree) = pane_tree {
                pane_trees.insert(pane_tree.workspace_tab_id(), pane_tree);
            }
            router.routes.insert(
                window_id,
                WindowRoute {
                    active_tab_id: transfer.active_tab_id,
                    ui,
                    transfer: Some(transfer),
                    pane_trees,
                },
            );
        }
    }

    pub(super) fn set_active(&self, window_id: Uuid, tab_id: Uuid) {
        if let Ok(mut router) = self.inner.lock()
            && let Some(route) = router.routes.get_mut(&window_id)
        {
            if route.pane_trees.contains_key(&tab_id) {
                route.active_tab_id = Some(tab_id);
            } else if let Some((workspace_tab_id, tree)) = route
                .pane_trees
                .iter_mut()
                .find(|(_, tree)| tree.contains(tab_id))
            {
                let _ = tree.set_focused(tab_id);
                route.active_tab_id = Some(*workspace_tab_id);
            } else {
                route.active_tab_id = Some(tab_id);
            }
        }
    }

    pub(super) fn main_ui(&self) -> Option<slint::Weak<AppWindow>> {
        self.inner.lock().ok().and_then(|router| {
            router
                .routes
                .get(&MAIN_WINDOW_ID)
                .map(|route| route.ui.clone())
        })
    }

    pub(super) fn active_tab(&self, window_id: Uuid) -> Option<Uuid> {
        self.inner.lock().ok().and_then(|router| {
            let route = router.routes.get(&window_id)?;
            let workspace_tab_id = route.active_tab_id?;
            Some(
                route
                    .pane_trees
                    .get(&workspace_tab_id)
                    .map(PaneTree::focused_tab_id)
                    .unwrap_or(workspace_tab_id),
            )
        })
    }

    pub(super) fn activate_tab(&self, window_id: Uuid, tab_id: Uuid, app: &mut AppState) -> bool {
        let is_terminal = app
            .terminal(tab_id)
            .is_some_and(|terminal| !terminal.is_sftp());
        let Ok(mut router) = self.inner.lock() else {
            return false;
        };
        if !route_tab_ids(&router, window_id, app).contains(&tab_id) {
            return false;
        }
        let Some(route) = router.routes.get_mut(&window_id) else {
            return false;
        };
        route.active_tab_id = Some(tab_id);
        let active_session_id = if is_terminal {
            route
                .pane_trees
                .entry(tab_id)
                .or_insert_with(|| PaneTree::new(tab_id))
                .focused_tab_id()
        } else {
            tab_id
        };
        app.activate_tab(active_session_id)
    }

    pub(super) fn focus_terminal_pane(
        &self,
        window_id: Uuid,
        tab_id: Uuid,
        app: &mut AppState,
    ) -> Option<PaneLayout> {
        if app
            .terminal(tab_id)
            .is_none_or(|terminal| terminal.is_sftp())
        {
            return None;
        }
        let Ok(mut router) = self.inner.lock() else {
            return None;
        };
        let route = router.routes.get_mut(&window_id)?;
        let (workspace_tab_id, tree) = route
            .pane_trees
            .iter_mut()
            .find(|(_, tree)| tree.contains(tab_id))?;
        let _ = tree.set_focused(tab_id);
        route.active_tab_id = Some(*workspace_tab_id);
        app.activate_tab(tab_id).then(|| tree.layout())
    }

    pub(super) fn focus_pane_direction(
        &self,
        window_id: Uuid,
        direction: PaneDirection,
        app: &mut AppState,
    ) -> Option<PaneLayout> {
        let Ok(mut router) = self.inner.lock() else {
            return None;
        };
        let route = router.routes.get_mut(&window_id)?;
        let workspace_tab_id = route.active_tab_id?;
        let tab_id = route
            .pane_trees
            .get_mut(&workspace_tab_id)
            .and_then(|tree| tree.focus_direction(direction))?;
        let tree = route.pane_trees.get(&workspace_tab_id)?;
        app.activate_tab(tab_id).then(|| tree.layout())
    }

    pub(super) fn prepare_pane_split(
        &self,
        window_id: Uuid,
        tab_id: Uuid,
        app: &mut AppState,
    ) -> bool {
        if self.focus_terminal_pane(window_id, tab_id, app).is_none() {
            return false;
        }
        self.inner.lock().is_ok_and(|router| {
            router
                .routes
                .get(&window_id)
                .and_then(|route| route.active_tab_id.and_then(|id| route.pane_trees.get(&id)))
                .is_some_and(|tree| tree.pane_count() < MAX_TERMINAL_PANES)
        })
    }

    pub(super) fn resize_terminal_divider(
        &self,
        window_id: Uuid,
        divider_id: i32,
        ratio: f32,
    ) -> Option<PaneLayout> {
        let Ok(mut router) = self.inner.lock() else {
            return None;
        };
        let route = router.routes.get_mut(&window_id)?;
        let workspace_tab_id = route.active_tab_id?;
        let tree = route.pane_trees.get_mut(&workspace_tab_id)?;
        tree.resize_split(divider_id, ratio).then(|| tree.layout())
    }

    pub(super) fn complete_pane_split(
        &self,
        window_id: Uuid,
        source_tab_id: Uuid,
        direction: PaneDirection,
        new_tab_id: Uuid,
        app: &mut AppState,
    ) -> bool {
        if app
            .terminal(new_tab_id)
            .is_none_or(|terminal| terminal.is_sftp())
        {
            return false;
        }
        let Ok(mut router) = self.inner.lock() else {
            return false;
        };
        let Some(route) = router.routes.get_mut(&window_id) else {
            return false;
        };
        let Some((workspace_tab_id, tree)) = route
            .pane_trees
            .iter_mut()
            .find(|(_, tree)| tree.contains(source_tab_id))
        else {
            return false;
        };
        let workspace_tab_id = *workspace_tab_id;
        let split = tree.set_focused(source_tab_id) && tree.split_focused(direction, new_tab_id);
        if !split {
            return false;
        }
        if let Some(transfer) = &mut route.transfer
            && !transfer.tab_ids.contains(&new_tab_id)
        {
            transfer.tab_ids.push(new_tab_id);
        }
        route.active_tab_id = Some(workspace_tab_id);
        app.activate_tab(new_tab_id)
    }

    pub(super) fn remove_terminal_child_pane(
        &self,
        window_id: Option<Uuid>,
        tab_id: Uuid,
        app: &mut AppState,
    ) -> Option<ClosedTab> {
        if app
            .terminal(tab_id)
            .is_none_or(|terminal| terminal.is_sftp())
        {
            return None;
        }
        let focused_tab_id = {
            let mut router = self.inner.lock().ok()?;
            let owner = match window_id {
                Some(window_id) => router.routes.get(&window_id).and_then(|route| {
                    route
                        .pane_trees
                        .iter()
                        .find_map(|(workspace_tab_id, tree)| {
                            tree.contains(tab_id)
                                .then_some((window_id, *workspace_tab_id))
                        })
                }),
                None => router.routes.iter().find_map(|(window_id, route)| {
                    route
                        .pane_trees
                        .iter()
                        .find_map(|(workspace_tab_id, tree)| {
                            tree.contains(tab_id)
                                .then_some((*window_id, *workspace_tab_id))
                        })
                }),
            }?;
            let route = router.routes.get_mut(&owner.0)?;
            let tree = route.pane_trees.get_mut(&owner.1)?;
            if tab_id == tree.workspace_tab_id() {
                return None;
            }
            let focused_tab_id = tree.remove(tab_id)?;
            if let Some(transfer) = &mut route.transfer {
                transfer.tab_ids.retain(|candidate| *candidate != tab_id);
                if transfer.active_tab_id == Some(tab_id) {
                    transfer.active_tab_id = Some(focused_tab_id);
                }
            }
            if route.active_tab_id == Some(tab_id) {
                route.active_tab_id = Some(owner.1);
            }
            focused_tab_id
        };
        let activate_survivor = app.active_tab_id() == Some(tab_id);
        let closed = app.close_tab(tab_id)?;
        if activate_survivor {
            let _ = app.activate_tab(focused_tab_id);
        }
        Some(closed)
    }

    pub(super) fn owns_terminal_pane(&self, window_id: Uuid, tab_id: Uuid, app: &AppState) -> bool {
        app.terminal(tab_id)
            .is_some_and(|terminal| !terminal.is_sftp())
            && self.inner.lock().is_ok_and(|router| {
                router.routes.get(&window_id).is_some_and(|route| {
                    route.pane_trees.values().any(|tree| tree.contains(tab_id))
                })
            })
    }

    pub(super) fn tab_ids(&self, window_id: Uuid, app: &AppState) -> Vec<Uuid> {
        self.inner
            .lock()
            .map(|router| route_tab_ids(&router, window_id, app))
            .unwrap_or_default()
    }

    pub(super) fn cycle_tab(&self, window_id: Uuid, next: bool, app: &mut AppState) -> bool {
        let (tab_ids, active_tab_id) = match self.inner.lock() {
            Ok(router) => {
                let tab_ids = route_tab_ids(&router, window_id, app);
                let active_tab_id = router
                    .routes
                    .get(&window_id)
                    .and_then(|route| route.active_tab_id);
                (tab_ids, active_tab_id)
            }
            Err(_) => return false,
        };
        if tab_ids.len() < 2 {
            return false;
        }
        let Some(active_index) = active_tab_id
            .and_then(|active_tab_id| tab_ids.iter().position(|id| *id == active_tab_id))
        else {
            return false;
        };
        let target_index = if next {
            (active_index + 1) % tab_ids.len()
        } else {
            active_index.checked_sub(1).unwrap_or(tab_ids.len() - 1)
        };
        self.activate_tab(window_id, tab_ids[target_index], app)
    }

    pub(super) fn include_tab(&self, window_id: Uuid, tab_id: Uuid) -> bool {
        let Ok(mut router) = self.inner.lock() else {
            return false;
        };
        let Some(route) = router.routes.get_mut(&window_id) else {
            return false;
        };
        if let Some(transfer) = &mut route.transfer
            && !transfer.tab_ids.contains(&tab_id)
        {
            transfer.tab_ids.push(tab_id);
        }
        true
    }

    pub(super) fn take_pane_tree_for_detach(
        &self,
        window_id: Uuid,
        tab_id: Uuid,
    ) -> Option<PaneTree> {
        self.inner.lock().ok().and_then(|mut router| {
            let route = router.routes.get_mut(&window_id)?;
            let workspace_tab_id =
                route
                    .pane_trees
                    .iter()
                    .find_map(|(workspace_tab_id, tree)| {
                        tree.contains(tab_id).then_some(*workspace_tab_id)
                    })?;
            route.pane_trees.remove(&workspace_tab_id)
        })
    }

    pub(super) fn pane_tab_ids(&self, window_id: Uuid, tab_id: Uuid) -> Vec<Uuid> {
        self.inner
            .lock()
            .ok()
            .and_then(|router| {
                router
                    .routes
                    .get(&window_id)
                    .and_then(|route| route.pane_trees.values().find(|tree| tree.contains(tab_id)))
                    .map(PaneTree::tab_ids)
            })
            .unwrap_or_else(|| vec![tab_id])
    }

    pub(super) fn take_workspace_tab_ids(&self, tab_id: Uuid) -> Vec<Uuid> {
        let Ok(mut router) = self.inner.lock() else {
            return vec![tab_id];
        };
        let owner = router.routes.iter().find_map(|(window_id, route)| {
            route
                .pane_trees
                .iter()
                .find_map(|(workspace_tab_id, tree)| {
                    tree.contains(tab_id)
                        .then_some((*window_id, *workspace_tab_id))
                })
        });
        let workspace_tab_id = owner
            .map(|(_, workspace_tab_id)| workspace_tab_id)
            .unwrap_or(tab_id);
        let removed = owner
            .and_then(|(window_id, workspace_tab_id)| {
                router
                    .routes
                    .get_mut(&window_id)?
                    .pane_trees
                    .remove(&workspace_tab_id)
            })
            .map(|tree| tree.tab_ids())
            .unwrap_or_else(|| vec![tab_id]);
        for route in router.routes.values_mut() {
            if let Some(transfer) = &mut route.transfer {
                transfer
                    .tab_ids
                    .retain(|candidate| !removed.contains(candidate));
            }
            if route.active_tab_id == Some(workspace_tab_id) {
                route.active_tab_id = None;
            }
        }
        removed
    }

    pub(super) fn remove_detached(&self, window_id: Uuid) -> Option<DetachedRoute> {
        self.inner
            .lock()
            .ok()
            .and_then(|mut router| router.routes.remove(&window_id))
            .and_then(|route| {
                let mut transfer = route.transfer?;
                transfer.active_tab_id = route.active_tab_id.or(transfer.active_tab_id);
                Some(DetachedRoute {
                    transfer,
                    pane_tree: route.pane_trees.into_values().next(),
                })
            })
    }

    pub(super) fn restore_detached(&self, detached: &DetachedRoute) -> Option<Uuid> {
        let mut router = self.inner.lock().ok()?;
        let main = router.routes.get_mut(&MAIN_WINDOW_ID)?;
        let pane_tree = detached.pane_tree.clone();
        main.active_tab_id = detached.transfer.active_tab_id.map(|active_tab_id| {
            pane_tree
                .as_ref()
                .filter(|tree| tree.contains(active_tab_id))
                .map(PaneTree::workspace_tab_id)
                .unwrap_or(active_tab_id)
        });
        if let Some(pane_tree) = pane_tree {
            main.pane_trees
                .insert(pane_tree.workspace_tab_id(), pane_tree);
        }
        let workspace_tab_id = main.active_tab_id?;
        Some(
            main.pane_trees
                .get(&workspace_tab_id)
                .map(PaneTree::focused_tab_id)
                .unwrap_or(workspace_tab_id),
        )
    }

    pub(super) fn views(&self, app: &AppState) -> Vec<WindowView> {
        let Ok(mut router) = self.inner.lock() else {
            return Vec::new();
        };
        let detached_ids = router
            .routes
            .values()
            .filter_map(|route| route.transfer.as_ref())
            .flat_map(|transfer| transfer.tab_ids.iter().copied())
            .collect::<HashSet<_>>();
        router
            .routes
            .iter_mut()
            .map(|(_, route)| {
                let is_detached = route.transfer.is_some();
                let transfer_active_tab_id = route
                    .transfer
                    .as_ref()
                    .and_then(|transfer| transfer.active_tab_id);
                let hidden_pane_ids = route
                    .pane_trees
                    .iter()
                    .flat_map(|(workspace_tab_id, tree)| {
                        let workspace_tab_id = *workspace_tab_id;
                        tree.tab_ids()
                            .into_iter()
                            .filter(move |tab_id| *tab_id != workspace_tab_id)
                    })
                    .collect::<HashSet<_>>();
                let tabs = route
                    .transfer
                    .as_ref()
                    .map(|transfer| app.tab_summaries_for(&transfer.tab_ids))
                    .unwrap_or_else(|| {
                        app.tab_summaries()
                            .into_iter()
                            .filter(|tab| !detached_ids.contains(&tab.id))
                            .collect()
                    })
                    .into_iter()
                    .filter(|tab| !hidden_pane_ids.contains(&tab.id))
                    .collect::<Vec<_>>();
                let workspace_tab_for = |tab_id| {
                    route
                        .pane_trees
                        .iter()
                        .find_map(|(workspace_tab_id, tree)| {
                            tree.contains(tab_id).then_some(*workspace_tab_id)
                        })
                        .unwrap_or(tab_id)
                };
                let active_tab_id = route
                    .active_tab_id
                    .filter(|id| tabs.iter().any(|tab| tab.id == *id))
                    .or_else(|| {
                        transfer_active_tab_id
                            .map(&workspace_tab_for)
                            .filter(|id| tabs.iter().any(|tab| tab.id == *id))
                    })
                    .or_else(|| {
                        (!is_detached)
                            .then(|| app.active_tab_id())
                            .flatten()
                            .map(workspace_tab_for)
                            .filter(|id| tabs.iter().any(|tab| tab.id == *id))
                    })
                    .or_else(|| tabs.first().map(|tab| tab.id));
                if let Some(tab_id) = active_tab_id
                    && app
                        .terminal(tab_id)
                        .is_some_and(|terminal| !terminal.is_sftp())
                {
                    route
                        .pane_trees
                        .entry(tab_id)
                        .or_insert_with(|| PaneTree::new(tab_id));
                }
                let active_session_id = active_tab_id.map(|tab_id| {
                    route
                        .pane_trees
                        .get(&tab_id)
                        .map(PaneTree::focused_tab_id)
                        .unwrap_or(tab_id)
                });
                let snapshot = app.snapshot_for(active_session_id);
                route.active_tab_id = active_tab_id;
                let (terminal_panes, terminal_dividers) = active_tab_id
                    .and_then(|tab_id| route.pane_trees.get(&tab_id))
                    .filter(|_| snapshot.kind == "terminal")
                    .map(|tree| {
                        let workspace_tab_id = tree.workspace_tab_id();
                        let layout = tree.layout();
                        let panes = layout
                            .panes
                            .into_iter()
                            .filter_map(|placement| {
                                let pane_snapshot = app.snapshot_for(Some(placement.tab_id));
                                (pane_snapshot.kind == "terminal").then_some(WindowTerminalPane {
                                    closable: placement.tab_id != workspace_tab_id,
                                    placement,
                                    snapshot: pane_snapshot,
                                })
                            })
                            .collect();
                        (panes, layout.dividers)
                    })
                    .unwrap_or_default();
                WindowView {
                    ui: route.ui.clone(),
                    tabs,
                    active_tab_id,
                    snapshot,
                    terminal_panes,
                    terminal_dividers,
                }
            })
            .collect()
    }
}

fn route_tab_ids(router: &WindowRouterState, window_id: Uuid, app: &AppState) -> Vec<Uuid> {
    let Some(route) = router.routes.get(&window_id) else {
        return Vec::new();
    };
    let hidden_pane_ids = route
        .pane_trees
        .iter()
        .flat_map(|(workspace_tab_id, tree)| {
            let workspace_tab_id = *workspace_tab_id;
            tree.tab_ids()
                .into_iter()
                .filter(move |tab_id| *tab_id != workspace_tab_id)
        })
        .collect::<HashSet<_>>();
    route
        .transfer
        .as_ref()
        .map(|transfer| transfer.tab_ids.clone())
        .unwrap_or_else(|| {
            let detached_ids = router
                .routes
                .values()
                .filter_map(|route| route.transfer.as_ref())
                .flat_map(|transfer| transfer.tab_ids.iter().copied())
                .collect::<HashSet<_>>();
            app.tab_summaries()
                .into_iter()
                .filter(|tab| !detached_ids.contains(&tab.id))
                .map(|tab| tab.id)
                .collect()
        })
        .into_iter()
        .filter(|tab_id| !hidden_pane_ids.contains(tab_id))
        .collect()
}

pub(super) fn global_window_router() -> Option<WindowRouter> {
    GLOBAL_WINDOW_ROUTER.get().cloned()
}

pub(super) fn sync_window_active(
    router: &WindowRouter,
    window_id: Uuid,
    state: &Arc<Mutex<AppState>>,
) {
    let Some(tab_id) = router.active_tab(window_id) else {
        return;
    };
    if let Ok(mut app) = state.lock() {
        let _ = app.activate_tab(tab_id);
    }
}

#[cfg(test)]
mod tests;
