//! The page of a screen that is not built yet: it says which milestone brings it.

use iced::widget::{column, text};
use iced::{Element, Length};

use crate::app::{Message, Page};
use crate::page;

pub fn view(page: Page) -> Element<'static, Message> {
    let when = match page.milestone() {
        Some(milestone) => format!("This screen comes with {milestone}."),
        None => "This screen has no milestone yet.".to_string(),
    };
    column![
        page::header(
            page.group(),
            page.title(),
            None,
            page::context(when.clone()),
            text("").into(),
        ),
        page::boxed(page.title(), page::body(when)),
    ]
    .spacing(crate::theme::PAGE_GAP)
    .width(Length::Fill)
    .into()
}
