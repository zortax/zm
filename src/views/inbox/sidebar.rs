use std::collections::HashMap;
use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::Sizable;
use gpui_component::{
    ActiveTheme, Disableable, Icon, Selectable,
    button::{Button, ButtonGroup},
    divider::Divider,
    h_flex,
    popover::Popover,
    sidebar::SidebarItem,
    v_flex,
};

use crate::assets::IconName;
use crate::components::folder_item::folder_tree_item;
use crate::config::AccountConfig;
use crate::state::folder_tree::build_folder_tree;
use crate::state::mailbox::MailboxState;
use crate::sync::engine::SyncEngine;
use crate::views::inbox::InboxView;

#[derive(IntoElement)]
struct AccountHeaderTrigger {
    element: Stateful<Div>,
    selected: bool,
}

impl AccountHeaderTrigger {
    fn new(element: Stateful<Div>) -> Self {
        Self {
            element,
            selected: false,
        }
    }
}

impl Selectable for AccountHeaderTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for AccountHeaderTrigger {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        self.element
    }
}

pub fn render_sidebar(
    mailbox: &Entity<MailboxState>,
    sync_engines: &HashMap<String, Entity<SyncEngine>>,
    accounts: &[AccountConfig],
    active_account_id: &str,
    collapsed: bool,
    _window: &mut Window,
    cx: &mut Context<InboxView>,
) -> impl IntoElement {
    let mb = mailbox.read(cx);
    let active_folder = mb.active_folder;
    let account_name = mb.account_name.clone();
    let account_email = mb.account_email.clone();

    let is_syncing = sync_engines
        .get(active_account_id)
        .map(|e| e.read(cx).is_syncing())
        .unwrap_or(false);

    let tree = build_folder_tree(&mb.folders);
    let on_select: Rc<dyn Fn(usize, &mut Window, &mut App)> = {
        let listener = cx.listener(|this, idx: &usize, _, cx| {
            this.select_folder(*idx, cx);
        });
        Rc::new(move |idx, window, cx| {
            listener(&idx, window, cx);
        })
    };
    let folder_items: Vec<_> = tree
        .iter()
        .map(|node| folder_tree_item(node, active_folder, on_select.clone()))
        .collect();

    let theme = cx.theme();

    let active_engine = sync_engines.get(active_account_id).cloned();
    let reload_button = Button::new("reload")
        .icon(Icon::new(IconName::RefreshCw))
        .disabled(is_syncing)
        .when(active_engine.is_some(), |btn| {
            let engine = active_engine.unwrap();
            btn.on_click(cx.listener(move |_this, _, _, cx| {
                engine.update(cx, |e, cx| e.trigger_sync(cx));
            }))
        });

    // Build account switcher popover content data
    let accounts_data: Vec<(String, String, String, bool)> = accounts
        .iter()
        .map(|a| {
            (
                a.id.clone(),
                a.display_name.clone(),
                a.email.clone(),
                a.id == active_account_id,
            )
        })
        .collect();

    let on_switch: Rc<dyn Fn(&String, &mut Window, &mut App)> = {
        let listener = cx.listener(|this, id: &String, _, cx| {
            this.switch_account(id, cx);
        });
        Rc::new(move |id, window, cx| {
            listener(id, window, cx);
        })
    };

    let on_add: Rc<dyn Fn(&mut Window, &mut App)> = {
        let listener = cx.listener(|this, _: &(), window, cx| {
            this.add_account(window, cx);
        });
        Rc::new(move |window, cx| {
            listener(&(), window, cx);
        })
    };

    let on_remove: Rc<dyn Fn(&String, &mut Window, &mut App)> = {
        let listener = cx.listener(|this, id: &String, _, cx| {
            this.remove_account(id, cx);
        });
        Rc::new(move |id, window, cx| {
            listener(id, window, cx);
        })
    };

    let account_header_trigger = h_flex()
        .id("account-header")
        .p_2()
        .gap_2()
        .items_center()
        .cursor_pointer()
        .rounded(theme.radius)
        .hover(|s| {
            s.bg(theme.sidebar_accent)
                .text_color(theme.sidebar_accent_foreground)
        })
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .rounded(theme.radius)
                .bg(theme.accent)
                .text_color(theme.accent_foreground)
                .size_8()
                .flex_shrink_0()
                .child(Icon::new(IconName::Mail)),
        )
        .when(!collapsed, |el| {
            el.child(
                v_flex()
                    .gap_0()
                    .text_sm()
                    .flex_1()
                    .line_height(relative(1.25))
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(account_name)
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(account_email),
                    ),
            )
            .child(Icon::new(IconName::ChevronsUpDown).size_4().flex_shrink_0())
        });

    let account_switcher = Popover::new("account-switcher")
        .trigger(AccountHeaderTrigger::new(account_header_trigger))
        .content({
            let accounts_data = accounts_data.clone();
            let on_switch = on_switch.clone();
            let on_add = on_add.clone();
            let on_remove = on_remove.clone();
            move |_state, _window, cx| {
                let popover_entity = cx.entity().clone();

                v_flex()
                    .w(px(240.))
                    .gap_0p5()
                    .children(accounts_data.iter().map(|(id, name, email, is_active)| {
                        let id = id.clone();
                        let name = name.clone();
                        let email = email.clone();
                        let is_active = *is_active;
                        let on_switch = on_switch.clone();
                        let on_remove = on_remove.clone();
                        let popover_for_switch = popover_entity.clone();
                        let popover_for_remove = popover_entity.clone();
                        let id_for_remove = id.clone();

                        h_flex()
                            .id(SharedString::from(format!("account-{}", id)))
                            .w_full()
                            .gap_2()
                            .px_1()
                            .py_0p5()
                            .items_center()
                            .rounded(px(6.))
                            .cursor_pointer()
                            .hover(|s| s.bg(cx.theme().sidebar_accent))
                            .on_click({
                                let id = id.clone();
                                move |_, window, cx| {
                                    popover_for_switch.update(cx, |state, cx| {
                                        state.dismiss(window, cx);
                                    });
                                    on_switch(&id, window, cx);
                                }
                            })
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(6.))
                                    .bg(cx.theme().accent)
                                    .text_color(cx.theme().accent_foreground)
                                    .size_7()
                                    .flex_shrink_0()
                                    .text_sm()
                                    .child(name.chars().next().unwrap_or('?').to_string()),
                            )
                            .child(
                                v_flex()
                                    .flex_1()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .gap_0()
                                    .child(div().text_sm().child(name))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(email),
                                    ),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("action-{}", id)))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(4.))
                                    .size_6()
                                    .flex_shrink_0()
                                    .when(is_active, |el| {
                                        el.child(
                                            Icon::new(IconName::Check)
                                                .size_3p5()
                                                .text_color(cx.theme().foreground),
                                        )
                                    })
                                    .when(!is_active, |el| {
                                        el.cursor_pointer()
                                            .hover(|s| s.bg(cx.theme().danger.opacity(0.15)))
                                            .on_click(move |_, window, cx| {
                                                cx.stop_propagation();
                                                popover_for_remove.update(cx, |state, cx| {
                                                    state.dismiss(window, cx);
                                                });
                                                on_remove(&id_for_remove, window, cx);
                                            })
                                            .child(
                                                Icon::new(IconName::Trash2)
                                                    .size_3p5()
                                                    .text_color(cx.theme().muted_foreground),
                                            )
                                    }),
                            )
                    }))
                    .child(Divider::horizontal().my_1())
                    .child({
                        let on_add = on_add.clone();
                        let popover_for_add = popover_entity.clone();
                        h_flex()
                            .id("add-account-btn")
                            .w_full()
                            .gap_2()
                            .px_1()
                            .py_0p5()
                            .items_center()
                            .rounded(px(6.))
                            .cursor_pointer()
                            .hover(|s| s.bg(cx.theme().sidebar_accent))
                            .on_click(move |_, window, cx| {
                                popover_for_add.update(cx, |state, cx| {
                                    state.dismiss(window, cx);
                                });
                                on_add(window, cx);
                            })
                            .child(
                                div().flex().items_center().justify_center().size_7().child(
                                    Icon::new(IconName::Plus)
                                        .size_4()
                                        .text_color(cx.theme().foreground),
                                ),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().foreground)
                                    .child("Add Account"),
                            )
                    })
            }
        });

    v_flex()
        .id("mail-sidebar")
        .w(if collapsed { px(52.) } else { px(220.) })
        .flex_shrink_0()
        .h_full()
        .overflow_hidden()
        .bg(theme.sidebar)
        .text_color(theme.sidebar_foreground)
        .border_color(theme.sidebar_border)
        .border_r_1()
        // Account switcher popover
        .child(
            div()
                .pt_3()
                .px_3()
                .pb_2()
                .when(collapsed, |this| this.pt_2().px_2())
                .child(account_switcher),
        )
        // Compose + Reload buttons
        .when(!collapsed, |el| {
            let on_compose = cx.listener(|this, _: &ClickEvent, window, cx| {
                this.open_compose(window, cx);
            });
            el.child(
                div().px_3().pb_2().child(
                    ButtonGroup::new("compose-group")
                        .outline()
                        .small()
                        .w_full()
                        .child(
                            Button::new("compose")
                                .icon(Icon::new(IconName::Pencil))
                                .label("Compose")
                                .flex_1()
                                .on_click(on_compose),
                        )
                        .child(reload_button),
                ),
            )
        })
        // Divider
        .child(div().px_3().child(Divider::horizontal()))
        // Folder list
        .child(v_flex().flex_1().min_h_0().px_3().pt_2().children(
            folder_items.into_iter().enumerate().map(|(ix, item)| {
                item.collapsed(collapsed)
                    .render(SharedString::from(format!("folder-{}", ix)), _window, cx)
                    .into_any_element()
            }),
        ))
}
