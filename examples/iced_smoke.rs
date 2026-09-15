//! Smallest possible iced app, to confirm the toolchain and backend work here
//! before committing to a rewrite.
use iced::widget::{column, text};

#[derive(Debug, Clone)]
enum Message {}

fn update(_state: &mut (), _message: Message) {}

fn view(_state: &()) -> iced::Element<'_, Message> {
    column![text("castty on iced")].padding(20).into()
}

fn main() -> iced::Result {
    iced::run(update, view)
}
