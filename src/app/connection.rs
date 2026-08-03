use super::*;

mod authentication;
mod direct;
mod host_key;
mod request;
mod worker_start;

pub(super) use self::host_key::wire_host_key_confirmation;
pub(in crate::app) use self::request::{request_profile_connection, wire_connection_request};

use self::authentication::begin_authentication;
pub(super) use self::authentication::wire_authentication;
use self::direct::{start_serial_connection, start_telnet_connection};
use self::worker_start::{
    AuthenticationStart, set_awaiting_authentication, set_loading_stored_credential,
    start_session_worker, terminal_has_phase,
};
