//! Left sidebar: hierarchical NIP-29 channel list rendered with tui-tree-widget.
//! Provides keyboard-driven expand/collapse (h/l), navigation (j/k),
//! and channel selection (Enter). TreeState is ephemeral view state
//! stored here, not in TuiSnapshot.
use crate::actions::Action;
use crate::app::{ChannelListItem, ChannelTier, Focus, TuiSnapshot};
use crate::ui;
use crate::Component;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use nmp_nip29::GroupId;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;
use tui_tree_widget::{Tree, TreeItem, TreeState};

pub struct RoomListComponent {
    items: Vec<ChannelListItem>,
    /// Pre-built tree items, rebuilt on every channel-tree update.
    tree_items: Vec<TreeItem<'static, String>>,
    /// The currently active (chat-open) channel — used to sync tree cursor on change.
    selected_channel: Option<GroupId>,
    focused: bool,
    state: TreeState<String>,
    /// The `local_id`s of rows on-screen after the most recent [`Self::
    /// draw`] call (29er#61) — the group-tree viewport `App::
    /// report_group_tree_viewport` feeds into `SharedProjections::
    /// group_tree_viewport`. A pure by-product of rendering: computing it
    /// here does not touch NMP, so it stays safe to run from inside
    /// `draw`'s own render closure (the reconcile it eventually drives
    /// happens later, outside that closure — see `App::
    /// report_group_tree_viewport`'s doc note).
    visible_local_ids: Vec<String>,
}

impl Default for RoomListComponent {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            tree_items: Vec::new(),
            selected_channel: None,
            focused: false,
            state: TreeState::default(),
            visible_local_ids: Vec::new(),
        }
    }
}

impl RoomListComponent {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, s: &TuiSnapshot) {
        self.items = s.channel_tree.clone();
        self.focused = s.focus == Focus::RoomList;

        // Rebuild tree items when the channel list changes.
        self.tree_items = Self::build_tree_items(&self.items);

        // Only reposition the tree cursor when the active channel changes.
        let new_id = s.selected_channel_id.as_ref().map(|g| g.local_id.as_str());
        let old_id = self.selected_channel.as_ref().map(|g| g.local_id.as_str());
        if new_id != old_id {
            self.selected_channel = s.selected_channel_id.clone();
            if let Some(gid) = &self.selected_channel {
                let path = Self::path_to_id(&self.items, &gid.local_id);
                if !path.is_empty() {
                    self.state.select(path);
                }
            }
        }
    }

    /// Build the full path (root-id → … → leaf-id) for `local_id` from the
    /// flat DFS-ordered channel list.
    fn path_to_id(items: &[ChannelListItem], local_id: &str) -> Vec<String> {
        let Some(pos) = items.iter().position(|it| it.local_id == local_id) else {
            return vec![];
        };
        let mut path = Vec::new();
        let mut depth = items[pos].depth;
        path.push(items[pos].local_id.clone());
        if depth == 0 {
            return path;
        }
        let mut i = pos;
        while i > 0 {
            i -= 1;
            if items[i].depth < depth {
                path.push(items[i].local_id.clone());
                depth = items[i].depth;
                if depth == 0 {
                    break;
                }
            }
        }
        path.reverse();
        path
    }

    /// Format a channel item into a single-line styled Text.
    /// Badge and name style are driven by the channel's `ChannelTier`.
    ///
    /// Tier → rendering:
    /// - Mention : red ⚡[N] badge, bold name
    /// - Unread  : mauve [N] badge, bold name
    /// - Activity: no badge, italic dimmed name
    /// - Normal  : no badge, normal name
    fn item_text(it: &ChannelListItem) -> Text<'static> {
        let mut spans = Vec::new();

        match it.tier {
            ChannelTier::Mention => {
                spans.push(Span::styled(
                    format!("\u{26a1}[{}] ", it.unread),
                    Style::default().fg(ui::RED).add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::styled(
                    it.name.clone(),
                    Style::default().fg(ui::TEXT).add_modifier(Modifier::BOLD),
                ));
            }
            ChannelTier::Unread => {
                spans.push(Span::styled(
                    format!("[{}] ", it.unread),
                    Style::default().fg(ui::MAUVE).add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::styled(
                    it.name.clone(),
                    Style::default().fg(ui::TEXT).add_modifier(Modifier::BOLD),
                ));
            }
            ChannelTier::Activity => {
                spans.push(Span::styled(
                    it.name.clone(),
                    Style::default()
                        .fg(ui::SUBTEXT0)
                        .add_modifier(Modifier::ITALIC),
                ));
            }
            ChannelTier::Normal => {
                spans.push(Span::styled(it.name.clone(), Style::default().fg(ui::TEXT)));
            }
        }

        if it.typing_count > 0 {
            let label = if it.typing_count == 1 {
                " typing".to_string()
            } else {
                format!(" {} typing", it.typing_count)
            };
            spans.push(Span::styled(label, Style::default().fg(ui::GREEN)));
        }

        // Optional preview + timestamp, separated by em-dash
        let has_extra = it.last_preview.is_some() || it.last_timestamp.is_some();
        if has_extra {
            spans.push(Span::styled(
                " \u{2014} ".to_string(),
                Style::default().fg(ui::OVERLAY0),
            ));
        }
        if let Some(preview) = &it.last_preview {
            let trimmed: String = preview.chars().take(20).collect();
            spans.push(Span::styled(trimmed, Style::default().fg(ui::OVERLAY0)));
        }
        if let Some(ts) = it.last_timestamp {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                format!("({})", ui::relative_time(ts)),
                Style::default().fg(ui::OVERLAY0),
            ));
        }

        Text::from(Line::from(spans))
    }

    /// Convert the flat DFS-ordered `ChannelListItem` list into a nested
    /// `TreeItem` hierarchy using depth annotations.
    fn build_tree_items(items: &[ChannelListItem]) -> Vec<TreeItem<'static, String>> {
        let mut pos = 0;
        Self::build_subtree(items, 0, &mut pos)
    }

    fn build_subtree(
        items: &[ChannelListItem],
        depth: usize,
        pos: &mut usize,
    ) -> Vec<TreeItem<'static, String>> {
        let mut result = Vec::new();
        while *pos < items.len() {
            let item = &items[*pos];
            if item.depth < depth {
                break; // back up to parent level
            }
            if item.depth > depth {
                // Malformed DFS: skip
                *pos += 1;
                continue;
            }
            *pos += 1;
            let children = Self::build_subtree(items, depth + 1, pos);
            let text = Self::item_text(item);
            let id = item.local_id.clone();
            let tree_item = if children.is_empty() {
                TreeItem::new_leaf(id, text)
            } else {
                // Duplicate sibling IDs would be a NIP-29 bug; fall back gracefully.
                TreeItem::new(id, text, children).unwrap_or_else(|_| {
                    TreeItem::new_leaf(item.local_id.clone(), Self::item_text(item))
                })
            };
            result.push(tree_item);
        }
        result
    }

    /// Return the GroupId for the tree item currently under the cursor.
    fn selected_group_id(&self) -> Option<GroupId> {
        let path = self.state.selected();
        let leaf_id = path.last()?;
        self.items
            .iter()
            .find(|it| &it.local_id == leaf_id)
            .map(|it| it.group_id.clone())
    }

    /// The `local_id`s of rows on-screen after the most recent [`Component::
    /// draw`] call (29er#61) — `App::report_group_tree_viewport` reads this
    /// once per frame, after `terminal.draw` returns.
    pub fn visible_local_ids(&self) -> &[String] {
        &self.visible_local_ids
    }

    /// Recompute the visible-rows viewport from the tree's current
    /// scroll/expand state (`self.state`) and this frame's rendered
    /// `area`. `TreeState::flatten` already accounts for expand/collapse;
    /// `get_offset` + the block's inner height (area minus its 1-row
    /// top/bottom border) bound that flattened list to what's actually
    /// on-screen. Every rendered row is exactly one line tall (`item_text`
    /// always builds a single-line `Text`), so no per-row height lookup is
    /// needed. A pure computation over already-rendered state — it does
    /// not touch NMP, so it is safe to call from inside `draw` itself.
    fn compute_visible_local_ids(&self, area: Rect) -> Vec<String> {
        let inner_height = area.height.saturating_sub(2) as usize;
        if inner_height == 0 {
            return Vec::new();
        }
        let flat = self.state.flatten(&self.tree_items);
        let offset = self.state.get_offset().min(flat.len());
        let end = (offset + inner_height).min(flat.len());
        flat[offset..end]
            .iter()
            .filter_map(|entry| entry.identifier.last().cloned())
            .collect()
    }
}

impl Component for RoomListComponent {
    fn draw(&mut self, f: &mut Frame, area: Rect) {
        // 29er#61: recompute BEFORE the early-return below, so an empty
        // tree correctly reports zero visible rows too.
        self.visible_local_ids = self.compute_visible_local_ids(area);

        let border_style = if self.focused {
            Style::default().fg(ui::MAUVE)
        } else {
            Style::default().fg(ui::OVERLAY0)
        };
        let block = Block::default()
            .title(" channels ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style);

        if self.tree_items.is_empty() {
            let p = Paragraph::new(Span::styled(
                "discovering channels\u{2026}",
                Style::default().fg(ui::SUBTEXT0),
            ))
            .block(block);
            f.render_widget(p, area);
            return;
        }

        // Catppuccin Mocha: SURFACE0 (#313244) bg, TEXT (#cdd6f4) fg when focused.
        let highlight = if self.focused {
            Style::default()
                .bg(ui::SURFACE0)
                .fg(ui::TEXT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().bg(ui::MANTLE)
        };

        match Tree::new(&self.tree_items) {
            Ok(tree) => {
                let tree = tree
                    .block(block)
                    .highlight_style(highlight)
                    // Tree connector lines use OVERLAY0 (#6c7086) via the widget's
                    // default rendering; we configure the node glyphs here.
                    .node_closed_symbol("\u{25b8} ") // ▸
                    .node_open_symbol("\u{25be} ") // ▾
                    .node_no_children_symbol("  ");
                f.render_stateful_widget(tree, area, &mut self.state);
            }
            Err(_) => {
                let p = Paragraph::new("(tree render error)").block(block);
                f.render_widget(p, area);
            }
        }
    }

    fn handle_event(&mut self, event: &Event) -> Option<Action> {
        let Event::Key(key) = event else { return None };
        if key.kind != KeyEventKind::Press {
            return None;
        }

        match key.code {
            KeyCode::Down | KeyCode::Char('j') | KeyCode::PageDown => {
                self.state.key_down();
                Some(Action::Noop)
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::PageUp => {
                self.state.key_up();
                Some(Action::Noop)
            }
            // l / right arrow: expand node
            KeyCode::Right | KeyCode::Char('l') => {
                self.state.key_right();
                Some(Action::Noop)
            }
            // h / left arrow: collapse node (or move to parent)
            KeyCode::Left | KeyCode::Char('h') => {
                self.state.key_left();
                Some(Action::Noop)
            }
            KeyCode::Enter => self.selected_group_id().map(Action::SelectChannel),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};
    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn one_item() -> ChannelListItem {
        ChannelListItem {
            group_id: GroupId::new("wss://h", "a"),
            local_id: "a".into(),
            name: "A".into(),
            depth: 0,
            unread: 0,
            typing_count: 0,
            member_count: 1,
            admin_count: 0,
            is_branch: false,
            last_preview: None,
            last_timestamp: None,
            tier: crate::app::ChannelTier::Normal,
        }
    }

    #[test]
    fn arrows_and_vim_keys_return_noop_or_none() {
        let mut c = RoomListComponent::new();
        // Empty tree — navigation returns Noop (widget handles gracefully).
        assert!(matches!(
            c.handle_event(&key(KeyCode::Char('j'))),
            Some(Action::Noop)
        ));
        assert!(matches!(
            c.handle_event(&key(KeyCode::Up)),
            Some(Action::Noop)
        ));
        assert!(matches!(
            c.handle_event(&key(KeyCode::Char('h'))),
            Some(Action::Noop)
        ));
        assert!(matches!(
            c.handle_event(&key(KeyCode::Char('l'))),
            Some(Action::Noop)
        ));
    }

    #[test]
    fn enter_selects_channel_at_cursor() {
        let mut c = RoomListComponent::new();
        c.items = vec![one_item()];
        c.tree_items = RoomListComponent::build_tree_items(&c.items);
        c.state.select(vec!["a".to_string()]);
        assert!(matches!(
            c.handle_event(&key(KeyCode::Enter)),
            Some(Action::SelectChannel(_))
        ));
    }

    #[test]
    fn enter_with_no_selection_returns_none() {
        let mut c = RoomListComponent::new();
        c.items = vec![one_item()];
        c.tree_items = RoomListComponent::build_tree_items(&c.items);
        // No explicit select → state.selected() is empty → None
        assert!(c.handle_event(&key(KeyCode::Enter)).is_none());
    }

    #[test]
    fn test_navigation_down() {
        let mut c = RoomListComponent::new();
        assert!(matches!(
            c.handle_event(&key(KeyCode::Down)),
            Some(Action::Noop)
        ));
        assert!(matches!(
            c.handle_event(&key(KeyCode::PageDown)),
            Some(Action::Noop)
        ));
        assert!(matches!(
            c.handle_event(&key(KeyCode::Char('j'))),
            Some(Action::Noop)
        ));
    }

    #[test]
    fn test_navigation_up() {
        let mut c = RoomListComponent::new();
        assert!(matches!(
            c.handle_event(&key(KeyCode::Up)),
            Some(Action::Noop)
        ));
        assert!(matches!(
            c.handle_event(&key(KeyCode::PageUp)),
            Some(Action::Noop)
        ));
        assert!(matches!(
            c.handle_event(&key(KeyCode::Char('k'))),
            Some(Action::Noop)
        ));
    }

    #[test]
    fn test_expand_collapse_keys() {
        let mut c = RoomListComponent::new();
        assert!(matches!(
            c.handle_event(&key(KeyCode::Right)),
            Some(Action::Noop)
        ));
        assert!(matches!(
            c.handle_event(&key(KeyCode::Left)),
            Some(Action::Noop)
        ));
        assert!(matches!(
            c.handle_event(&key(KeyCode::Char('l'))),
            Some(Action::Noop)
        ));
        assert!(matches!(
            c.handle_event(&key(KeyCode::Char('h'))),
            Some(Action::Noop)
        ));
    }

    #[test]
    fn path_to_id_finds_nested_item() {
        use crate::app::ChannelTier;
        let items = vec![
            ChannelListItem {
                group_id: GroupId::new("wss://h", "root"),
                local_id: "root".into(),
                name: "Root".into(),
                depth: 0,
                unread: 0,
                typing_count: 0,
                member_count: 1,
                admin_count: 0,
                is_branch: true,
                last_preview: None,
                last_timestamp: None,
                tier: ChannelTier::Normal,
            },
            ChannelListItem {
                group_id: GroupId::new("wss://h", "child"),
                local_id: "child".into(),
                name: "Child".into(),
                depth: 1,
                unread: 0,
                typing_count: 0,
                member_count: 1,
                admin_count: 0,
                is_branch: false,
                last_preview: None,
                last_timestamp: None,
                tier: ChannelTier::Normal,
            },
        ];
        let path = RoomListComponent::path_to_id(&items, "child");
        assert_eq!(path, vec!["root".to_string(), "child".to_string()]);
    }

    #[test]
    fn build_tree_items_nests_children() {
        use crate::app::ChannelTier;
        let items = vec![
            ChannelListItem {
                group_id: GroupId::new("wss://h", "root"),
                local_id: "root".into(),
                name: "Root".into(),
                depth: 0,
                unread: 0,
                typing_count: 0,
                member_count: 1,
                admin_count: 0,
                is_branch: true,
                last_preview: None,
                last_timestamp: None,
                tier: ChannelTier::Normal,
            },
            ChannelListItem {
                group_id: GroupId::new("wss://h", "child"),
                local_id: "child".into(),
                name: "Child".into(),
                depth: 1,
                unread: 0,
                typing_count: 0,
                member_count: 1,
                admin_count: 0,
                is_branch: false,
                last_preview: None,
                last_timestamp: None,
                tier: ChannelTier::Normal,
            },
        ];
        let tree = RoomListComponent::build_tree_items(&items);
        // Should produce one root with one child
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].children().len(), 1);
    }

    fn n_items(n: usize) -> Vec<ChannelListItem> {
        (0..n)
            .map(|i| ChannelListItem {
                group_id: GroupId::new("wss://h", format!("room-{i}")),
                local_id: format!("room-{i}"),
                name: format!("Room {i}"),
                depth: 0,
                unread: 0,
                typing_count: 0,
                member_count: 1,
                admin_count: 0,
                is_branch: false,
                last_preview: None,
                last_timestamp: None,
                tier: crate::app::ChannelTier::Normal,
            })
            .collect()
    }

    // 29er#61: `visible_local_ids` is the group-tree viewport input `App::
    // report_group_tree_viewport` feeds NMP. It must reflect only the rows
    // that actually fit the rendered area — not every discovered room —
    // otherwise the whole point of gating (not eagerly opening reads for
    // off-screen groups) is moot.
    #[test]
    fn visible_local_ids_reflects_the_rendered_window_not_the_whole_tree() {
        let mut c = RoomListComponent::new();
        c.items = n_items(10);
        c.tree_items = RoomListComponent::build_tree_items(&c.items);
        // Height 5 minus the block's 1-row top/bottom border leaves 3
        // visible rows.
        let visible = c.compute_visible_local_ids(Rect::new(0, 0, 28, 5));
        assert_eq!(visible, vec!["room-0", "room-1", "room-2"]);
    }

    #[test]
    fn visible_local_ids_is_empty_when_the_area_has_no_inner_rows() {
        let mut c = RoomListComponent::new();
        c.items = n_items(3);
        c.tree_items = RoomListComponent::build_tree_items(&c.items);
        // 2 rows total is entirely consumed by the block's own border.
        assert!(c.compute_visible_local_ids(Rect::new(0, 0, 28, 2)).is_empty());
    }

    #[test]
    fn draw_updates_visible_local_ids_as_a_side_effect() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut c = RoomListComponent::new();
        c.items = n_items(10);
        c.tree_items = RoomListComponent::build_tree_items(&c.items);
        assert!(
            c.visible_local_ids().is_empty(),
            "nothing rendered yet, so nothing should be reported visible"
        );

        let mut terminal = Terminal::new(TestBackend::new(28, 5)).unwrap();
        terminal.draw(|f| c.draw(f, f.area())).unwrap();
        assert_eq!(
            c.visible_local_ids().len(),
            3,
            "draw() must populate visible_local_ids from the actually-rendered rows"
        );
    }
}
