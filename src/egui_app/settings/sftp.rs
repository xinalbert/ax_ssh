//! SFTP settings draft only. Files and connections remain controller-owned.

use super::super::i18n::tr;
use ax_ssh::config::{self, AppSettings, SftpTransferFilterSettings};
use eframe::egui;

pub(super) fn show(ui: &mut egui::Ui, settings: &mut AppSettings, filter_text: &mut String) {
    let transfers = &mut settings.sftp_transfers;
    ui.add(
        egui::Slider::new(
            &mut transfers.per_tab_transfers,
            config::MIN_SFTP_PER_TAB_TRANSFERS..=config::MAX_SFTP_PER_TAB_TRANSFERS,
        )
        .text(tr!("Concurrent transfers per connection", ui.ctx())),
    );
    ui.add(
        egui::Slider::new(
            &mut transfers.max_file_gib,
            0..=config::MAX_SFTP_MAX_FILE_GIB,
        )
        .text(tr!("Maximum file size (GiB, 0 = unlimited)", ui.ctx())),
    );
    ui.add(
        egui::Slider::new(
            &mut transfers.rate_limit_mib_per_second,
            0..=config::MAX_SFTP_RATE_LIMIT_MIB_PER_SECOND,
        )
        .text(tr!("Bandwidth limit (MiB/s; 0 unlimited)", ui.ctx())),
    );
    ui.label(tr!("Connection limits apply on the next SFTP connection. Saved server overrides take precedence.", ui.ctx()));
    ui.separator();
    ui.add(
        egui::Slider::new(
            &mut transfers.global_uploads,
            config::MIN_SFTP_GLOBAL_UPLOADS..=config::MAX_SFTP_GLOBAL_UPLOADS,
        )
        .text(tr!("Global simultaneous uploads", ui.ctx())),
    );
    ui.label(tr!("Maximum uploads shared by all SFTP tabs", ui.ctx()));
    ui.separator();
    ui.add(
        egui::Slider::new(
            &mut transfers.local_open_max_file_mib,
            0..=config::MAX_SFTP_LOCAL_OPEN_MAX_FILE_MIB,
        )
        .text(tr!("Local file open snapshot (MiB)", ui.ctx())),
    );
    ui.label(tr!("Local file copy before opening; default 512 MiB, 0 removes file and cache byte limits and may use substantial disk space", ui.ctx()));
    ui.separator();
    ui.add(
        egui::Slider::new(
            &mut transfers.remote_edit_max_file_mib,
            1..=config::MAX_SFTP_REMOTE_EDIT_MAX_FILE_MIB,
        )
        .text(tr!("External edit maximum size (MiB)", ui.ctx())),
    );
    ui.separator();
    let filters = &mut settings.sftp_transfer_filters;
    ui.checkbox(
        &mut filters.enabled,
        tr!("Filter upload and download names", ui.ctx()),
    );
    ui.add_enabled_ui(filters.enabled, |ui| {
        ui.checkbox(
            &mut filters.use_platform_defaults,
            tr!("Skip platform metadata files", ui.ctx()),
        );
        ui.label(tr!(
            "Custom filters: one name pattern per line; * matches any characters.",
            ui.ctx()
        ));
        if ui
            .add(
                egui::TextEdit::multiline(filter_text)
                    .desired_rows(5)
                    .char_limit(16 * 1024),
            )
            .changed()
        {
            *filters = SftpTransferFilterSettings::normalized(
                filters.enabled,
                filters.use_platform_defaults,
                filter_text,
            );
        }
        egui::CollapsingHeader::new(tr!("Platform defaults", ui.ctx()))
            .id_salt("platform-default-filters")
            .show(ui, |ui| {
                ui.monospace(SftpTransferFilterSettings::platform_default_patterns_text());
            });
    });
    if ui
        .button(tr!("Reset filters to platform defaults", ui.ctx()))
        .clicked()
    {
        *filters = SftpTransferFilterSettings::reset_to_platform_defaults();
        filter_text.clear();
    }
    ui.label(tr!("Filters and edit limits apply after Save. Filters omit invalid names and keep up to 64 unique patterns.", ui.ctx()));
}
