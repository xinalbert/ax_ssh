//! Persistent x11 settings and normalization.

use super::*;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum X11ServerProvider {
    #[default]
    Auto,
    System,
    XQuartz,
    MacXServer,
    VcXsrv,
    Xming,
    Custom,
}

impl X11ServerProvider {
    pub const fn as_setting(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::System => "system",
            Self::XQuartz => "xquartz",
            Self::MacXServer => "macxserver",
            Self::VcXsrv => "vcxsrv",
            Self::Xming => "xming",
            Self::Custom => "custom",
        }
    }

    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "system" => Self::System,
            "xquartz" => Self::XQuartz,
            "macxserver" | "mac-x-server" => Self::MacXServer,
            "vcxsrv" => Self::VcXsrv,
            "xming" => Self::Xming,
            "custom" => Self::Custom,
            _ => Self::Auto,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct X11Settings {
    #[serde(default)]
    pub provider: X11ServerProvider,
    #[serde(default)]
    pub app_path: String,
    #[serde(default = "default_true")]
    pub launch_on_connect: bool,
    #[serde(default)]
    pub allow_no_auth: bool,
}

impl X11Settings {
    pub fn normalized(
        provider: &str,
        app_path: &str,
        launch_on_connect: bool,
        allow_no_auth: bool,
    ) -> Self {
        Self {
            provider: X11ServerProvider::from_setting(provider),
            app_path: normalize_x11_app_path(app_path),
            launch_on_connect,
            allow_no_auth,
        }
    }

    pub(super) fn normalize_in_place(&mut self) {
        *self = Self::normalized(
            self.provider.as_setting(),
            &self.app_path,
            self.launch_on_connect,
            self.allow_no_auth,
        );
    }
}

impl Default for X11Settings {
    fn default() -> Self {
        Self {
            provider: X11ServerProvider::Auto,
            app_path: String::new(),
            launch_on_connect: true,
            allow_no_auth: false,
        }
    }
}

fn normalize_x11_app_path(value: &str) -> String {
    let value = value.trim();
    if value.chars().count() <= crate::config::MAX_X11_APP_PATH_CHARS
        && !value.chars().any(char::is_control)
    {
        value.to_owned()
    } else {
        String::new()
    }
}
