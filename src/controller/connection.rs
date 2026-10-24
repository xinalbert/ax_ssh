//! Non-secret connection identities and bounded reconnect state.

use uuid::Uuid;

mod reconnect;

pub use reconnect::ReconnectState;

/// Protocol, profile and live attempt identity; this value grants no trust or credentials.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionIdentity {
    Ssh {
        profile_id: Uuid,
        attempt_id: Option<Uuid>,
    },
    Sftp {
        profile_id: Uuid,
        attempt_id: Option<Uuid>,
    },
    Telnet {
        profile_id: Uuid,
        attempt_id: Option<Uuid>,
    },
    Serial {
        profile_id: Uuid,
        attempt_id: Option<Uuid>,
    },
    Local,
}

impl ConnectionIdentity {
    /// Saved profile identity, independent of any workspace tab UUID.
    pub fn profile_id(&self) -> Option<Uuid> {
        match *self {
            Self::Ssh { profile_id, .. }
            | Self::Sftp { profile_id, .. }
            | Self::Telnet { profile_id, .. }
            | Self::Serial { profile_id, .. } => Some(profile_id),
            Self::Local => None,
        }
    }

    /// SSH route shared by terminal and independently owned SFTP connections.
    pub fn ssh_route(&self) -> Option<(Uuid, Option<Uuid>)> {
        match *self {
            Self::Ssh {
                profile_id,
                attempt_id,
            }
            | Self::Sftp {
                profile_id,
                attempt_id,
            } => Some((profile_id, attempt_id)),
            Self::Telnet { .. } | Self::Serial { .. } | Self::Local => None,
        }
    }

    /// Telnet route; identities for other protocols are rejected.
    pub fn telnet_route(&self) -> Option<(Uuid, Option<Uuid>)> {
        match *self {
            Self::Telnet {
                profile_id,
                attempt_id,
            } => Some((profile_id, attempt_id)),
            Self::Ssh { .. } | Self::Sftp { .. } | Self::Serial { .. } | Self::Local => None,
        }
    }

    /// Serial route; identities for other protocols are rejected.
    pub fn serial_route(&self) -> Option<(Uuid, Option<Uuid>)> {
        match *self {
            Self::Serial {
                profile_id,
                attempt_id,
            } => Some((profile_id, attempt_id)),
            Self::Ssh { .. } | Self::Sftp { .. } | Self::Telnet { .. } | Self::Local => None,
        }
    }

    /// Assigns an SSH attempt only to an SSH terminal or SFTP identity.
    pub fn set_ssh_attempt(&mut self, attempt_id: Option<Uuid>) -> bool {
        match self {
            Self::Ssh {
                attempt_id: current,
                ..
            }
            | Self::Sftp {
                attempt_id: current,
                ..
            } => {
                *current = attempt_id;
                true
            }
            Self::Telnet { .. } | Self::Serial { .. } | Self::Local => false,
        }
    }

    /// Assigns a Telnet attempt only to a Telnet identity.
    pub fn set_telnet_attempt(&mut self, attempt_id: Option<Uuid>) -> bool {
        match self {
            Self::Telnet {
                attempt_id: current,
                ..
            } => {
                *current = attempt_id;
                true
            }
            Self::Ssh { .. } | Self::Sftp { .. } | Self::Serial { .. } | Self::Local => false,
        }
    }

    /// Assigns a serial attempt only to a serial identity.
    pub fn set_serial_attempt(&mut self, attempt_id: Option<Uuid>) -> bool {
        match self {
            Self::Serial {
                attempt_id: current,
                ..
            } => {
                *current = attempt_id;
                true
            }
            Self::Ssh { .. } | Self::Sftp { .. } | Self::Telnet { .. } | Self::Local => false,
        }
    }

    /// Whether the connection has no remote profile or authentication flow.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }

    /// Whether the SSH connection is owned by an independent SFTP tab.
    pub fn is_sftp(&self) -> bool {
        matches!(self, Self::Sftp { .. })
    }
}

#[cfg(test)]
mod tests;
