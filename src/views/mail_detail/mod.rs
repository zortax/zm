use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme, Icon, WindowExt as _,
    dialog::{DialogHeader, DialogTitle},
    divider::Divider,
    h_flex,
    input::{Input, InputState},
};

use crate::assets::IconName;
use crate::components::mail_actions::MailActions;
use crate::components::mail_metadata::MailMetadata;
use crate::html_render::HtmlEmailView;
use crate::state::mail::MailMessage;

pub fn open_mail_dialog(msg: &MailMessage, window: &mut Window, cx: &mut App) {
    let subject: SharedString = msg.subject.clone().into();
    let from_line: SharedString = format!("{} <{}>", msg.from_name, msg.from_email).into();
    let to: SharedString = msg.to.join(", ").into();
    let date: SharedString = msg.date.clone().into();
    let body: SharedString = msg.body.clone().into();
    let body_html = msg.body_html.clone();
    let is_starred = msg.is_starred;
    let warning_color = cx.theme().warning;

    let from_input = cx.new(|cx| InputState::new(window, cx).default_value(from_line.clone()));
    let to_input = cx.new(|cx| InputState::new(window, cx).default_value(to.clone()));
    let date_input = cx.new(|cx| InputState::new(window, cx).default_value(date.clone()));
    let body_input = cx.new(|cx| {
        InputState::new(window, cx)
            .multi_line(true)
            .default_value(body.clone())
    });

    let html_view = if !body_html.is_empty() {
        Some(cx.new(|cx| HtmlEmailView::new(body_html, window, cx)))
    } else {
        None
    };

    window.open_dialog(cx, move |dialog, _, _cx| {
        dialog
            .w(px(680.))
            .h(px(520.))
            .overlay(true)
            .overlay_closable(true)
            .p_0()
            .content({
                let subject = subject.clone();
                let from_input = from_input.clone();
                let to_input = to_input.clone();
                let date_input = date_input.clone();
                let body_input = body_input.clone();
                let html_view = html_view.clone();
                move |content, _, _cx| {
                    content
                        .flex()
                        .flex_col()
                        .size_full()
                        .overflow_hidden()
                        .child(
                            DialogHeader::new().p_4().pb_2().child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .when(is_starred, |el| {
                                        el.child(
                                            Icon::new(IconName::Star)
                                                .size_4()
                                                .text_color(warning_color),
                                        )
                                    })
                                    .child(DialogTitle::new().child(subject.clone())),
                            ),
                        )
                        .child(div().px_4().pb_2().child(MailActions::new()))
                        .child(div().px_4().child(Divider::horizontal()))
                        .child(div().p_4().child(MailMetadata::new(
                            &from_input,
                            &to_input,
                            &date_input,
                        )))
                        .child(div().px_4().child(Divider::horizontal()))
                        .child(
                            div().id("mail-body-scroll").flex_1().p_4().min_h_0().overflow_y_scroll().child(
                                if let Some(ref html_view) = html_view {
                                    div()
                                        .w_full()
                                        .child(html_view.clone())
                                        .into_any_element()
                                } else {
                                    div()
                                        .size_full()
                                        .opacity(1.)
                                        .child(
                                            Input::new(&body_input)
                                                .appearance(false)
                                                .disabled(true)
                                                .h_full(),
                                        )
                                        .into_any_element()
                                },
                            ),
                        )
                }
            })
    });
}
