use super::*;

impl Owner {
    pub(super) async fn mutate_catalog(
        &mut self,
        revision: u64,
        action: CatalogAction,
    ) -> Result<Option<String>> {
        ensure!(
            revision == self.revision,
            "session list changed; reopen the editor before saving"
        );
        let mut candidate = self.sessions.clone();
        let mut import = None;
        match action {
            CatalogAction::Import(text) => import = Some(text),
            CatalogAction::Export(target) => return self.export_catalog(target).await.map(Some),
            CatalogAction::Settings(settings) => {
                settings.shortcuts.validate()?;
                candidate.settings = *settings;
                // Reuse the schema's canonical load normalization; untouched domains survive.
                candidate = serde_json::from_slice(&serde_json::to_vec(&candidate)?)
                    .context("invalid settings")?;
            }
            CatalogAction::Upsert(profile) => {
                profile.validate()?;
                candidate.upsert(*profile);
            }
            CatalogAction::Delete(id) => {
                ensure!(
                    candidate
                        .sessions
                        .iter()
                        .find(|profile| profile.id == id)
                        .and_then(SessionProfile::ssh)
                        .is_none_or(|ssh| ssh.credential_storage.is_none()
                            && !ssh.credential_vault_key_saved),
                    "credential-backed profiles require credential management before deletion"
                );
                ensure!(candidate.remove(id), "session no longer exists");
            }
            CatalogAction::AddGroup(name) => {
                candidate.add_group(&name)?;
            }
            CatalogAction::RenameGroup { old, new } => {
                ensure!(candidate.rename_group(&old, &new)?, "group was not renamed");
            }
            CatalogAction::RemoveGroup(name) => {
                ensure!(candidate.remove_group(&name), "group no longer exists");
            }
        }
        self.busy = true;
        self.status = "Saving sessions…".into();
        self.publish()?;
        let store = self.store.clone();
        let result = tokio::task::spawn_blocking(move || {
            if let Some(text) = import {
                crate::session_transfer::import_session_transfer_into_store(
                    &mut candidate,
                    &text,
                    crate::session_transfer::SessionImportMode::Automatic,
                    "",
                )?;
            }
            candidate.validate()?;
            ensure!(
                candidate.sessions.iter().all(|profile| profile
                    .ssh()
                    .is_none_or(
                        |ssh| ssh.credential_storage.is_none() && !ssh.credential_vault_key_saved
                    )),
                "experimental credential storage is not enabled"
            );
            store.save(&candidate)?;
            Ok::<_, anyhow::Error>(candidate)
        })
        .await
        .context("session persistence task failed");
        self.busy = false;
        self.status = "Sessions ready".into();
        let previous_uploads = self.sessions.settings.sftp_transfers.global_uploads;
        self.sessions = result??;
        if self.sessions.settings.sftp_transfers.global_uploads != previous_uploads {
            (self.configure_uploads)(usize::from(
                self.sessions.settings.sftp_transfers.global_uploads,
            ));
        }
        // Only committed settings reach workers. A failed disk write leaves both
        // the published settings and every active worker on the previous values.
        for pane in self.panes.values() {
            pane.terminal
                .configure(self.sessions.settings.terminal.clone());
            pane.terminal
                .configure_x11(self.sessions.settings.x11.clone());
            pane.terminal.configure_audio(&self.sessions.settings.audio);
            pane.terminal.configure_refresh(
                self.sessions
                    .settings
                    .appearance
                    .focused_terminal_refresh_fps,
                self.sessions
                    .settings
                    .appearance
                    .unfocused_terminal_refresh_fps,
            );
            if let Some(sftp) = &pane.sftp {
                sftp.configure(
                    self.sessions.settings.sftp_transfers.clone(),
                    self.sessions.settings.sftp_transfer_filters.clone(),
                );
            }
        }
        self.revision = self
            .revision
            .checked_add(1)
            .context("catalog revision exhausted")?;
        Ok(None)
    }

    async fn export_catalog(
        &mut self,
        target: crate::session_transfer::SessionExportTarget,
    ) -> Result<String> {
        use crate::session_transfer::{
            SessionExportTarget, export_session_group, export_session_profile,
        };
        self.busy = true;
        self.publish()?;
        let sessions = self.sessions.clone();
        let result = tokio::task::spawn_blocking(move || match target {
            SessionExportTarget::Server(id) => export_session_profile(&sessions, &id.to_string()),
            SessionExportTarget::Group(name) => export_session_group(&sessions, &name),
        })
        .await
        .context("session export task failed");
        self.busy = false;
        result?
    }
}
