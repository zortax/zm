use std::rc::Rc;

use gpui::*;
use gpui_component::ActiveTheme;
use gpui_component::sidebar::SidebarMenuItem;

use crate::state::folder_tree::FolderNode;

pub fn folder_tree_item(
    node: &FolderNode,
    active_index: usize,
    on_select: Rc<dyn Fn(usize, &mut Window, &mut App)>,
) -> SidebarMenuItem {
    let is_active = node.folder_index == Some(active_index);
    let has_children = !node.children.is_empty();

    let children: Vec<SidebarMenuItem> = node
        .children
        .iter()
        .map(|child| folder_tree_item(child, active_index, on_select.clone()))
        .collect();

    let mut item = SidebarMenuItem::new(&node.display_name)
        .icon(node.kind.icon())
        .active(is_active);

    if let Some(folder_index) = node.folder_index {
        let on_select = on_select.clone();
        item = item.on_click(move |_, window, cx| {
            on_select(folder_index, window, cx);
        });
    }

    if has_children {
        item = item.children(children).default_open(true);
    }

    // Show unread count only on leaf nodes (not expandable parents)
    let unread = if has_children { 0 } else { node.unread_count };
    if unread > 0 {
        let count: SharedString = unread.to_string().into();
        item = item.suffix(move |_, cx| {
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(count.clone())
        });
    }

    item
}
