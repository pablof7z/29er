//! The single intent vocabulary the runtime loop understands. Components emit
//! these; `main::apply` is the only place that mutates [`crate::app::App`].

use nmp_nip29::GroupId;

#[derive(Clone, Debug)]
pub enum Action {
    Quit,
    NavigateUp,
    NavigateDown,
    ToggleFocus,
    SelectRoom(GroupId),
    SendMessage(String),
    Tick,
}
