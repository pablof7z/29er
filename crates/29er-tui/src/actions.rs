//! The single intent vocabulary the runtime understands. Components emit these;
//! `main::apply` is the only place that mutates `App`.
use nmp_nip29::GroupId;
use crate::app::{Focus, FormKind};

#[derive(Clone, Debug)]
pub enum Action {
    Quit,
    // identity
    LoginSubmit(String),
    // navigation
    NavigateUp,
    NavigateDown,
    NavigateTop,     // 'g' — jump to first channel
    NavigateBottom,  // 'G' — jump to last channel
    SelectChannel(GroupId),
    CycleFocus,
    ReverseCycleFocus,
    SetFocus(Focus),
    ScrollUp,
    ScrollDown,
    // chat / outbox
    SendMessage(String),
    RetryOutbox(String),
    // palette
    OpenPalette,
    ClosePalette,
    // help overlay
    OpenHelp,
    CloseHelp,
    // membership / admin (typed dispatch happens in App)
    Join { group: GroupId, invite_code: Option<String> },
    Leave { group: GroupId },
    ShowMembers(GroupId),
    CreateInvite { group: GroupId, codes: Vec<String> },
    PutUser { group: GroupId, target_pubkey: String, role: Option<String> },
    CreateChild { parent: GroupId, name: String },
    MoveChannel { group: GroupId, parent: Option<String> },
    // forms
    OpenForm(FormKind),
    CloseForm,
    /// Alt+A: jump to the next channel with a Mention-tier unread notification.
    JumpToNextMention,
    Noop,
}
