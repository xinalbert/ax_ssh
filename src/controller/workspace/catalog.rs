use super::*;

impl Owner {
    pub(super) async fn mutate_catalog(
        &mut self,
        revision: u64,
        action: CatalogAction,
    ) -> Result<()> {
        ensure!(
            revision == self.revision,
            "session list changed; reopen the editor before saving"
        );
        let mut candidate = self.sessions.clone();
        match action {
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
        candidate.validate()?;
        ensure!(
            candidate.sessions.iter().all(|profile| profile
                .ssh()
                .is_none_or(
                    |ssh| ssh.credential_storage.is_none() && !ssh.credential_vault_key_saved
                )),
            "experimental credential storage is not enabled"
        );
        self.busy = true;
        self.status = "Saving sessions…".into();
        self.publish()?;
        let store = self.store.clone();
        let result = tokio::task::spawn_blocking(move || {
            store.save(&candidate)?;
            Ok::<_, anyhow::Error>(candidate)
        })
        .await
        .context("session persistence task failed");
        self.busy = false;
        self.status = "Sessions ready".into();
        self.sessions = result??;
        self.revision = self
            .revision
            .checked_add(1)
            .context("catalog revision exhausted")?;
        Ok(())
    }
}
