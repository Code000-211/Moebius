//! The line icons of the sidebar, drawn on a 16 by 16 grid as in the mockups.

use iced::widget::svg;
use iced::widget::svg::Svg;
use iced::{Color, Length};

use crate::theme;

/// An icon of the sidebar.
#[derive(Debug, Clone, Copy)]
pub enum Icon {
    Inbox,
    Chat,
    /// Four tiles: every agent at work.
    Fleet,
    /// A clock: what runs on a schedule.
    Automations,
    Pencil,
    Agents,
    Skills,
    McpServers,
    Rules,
    Retro,
    Experiments,
    DecisionModel,
    Usage,
    Models,
    Plugins,
    Terminal,
    /// Two arrows past each other: a port carried to this computer.
    Ports,
    Settings,
    Start,
    Search,
    Panel,
    /// A step that is folded: it opens on a click.
    ChevronRight,
    /// A step that is open.
    ChevronDown,
    /// A thought: a spark.
    Spark,
    /// A quarter of a ring, for what is still working.
    Spinner,
    /// A line: a window becomes small again.
    Minimise,
    /// A directory of a repository.
    Folder,
    /// A file of a repository.
    File,
    /// A cross: a tab closes.
    Close,
}

impl Icon {
    fn paths(self) -> &'static str {
        match self {
            Icon::Inbox => r#"<path d="M2 9h3l1 2h4l1-2h3"/><path d="M2 9l2-5h8l2 5v4H2z"/>"#,
            Icon::Chat => r#"<path d="M2.5 3.5h11v7h-6l-3 3v-3h-2z"/>"#,
            Icon::Fleet => {
                r#"<rect x="2.5" y="2.5" width="4.5" height="4.5" rx="1"/><rect x="9" y="2.5" width="4.5" height="4.5" rx="1"/><rect x="2.5" y="9" width="4.5" height="4.5" rx="1"/><rect x="9" y="9" width="4.5" height="4.5" rx="1"/>"#
            }
            Icon::Automations => r#"<circle cx="8" cy="8" r="5.5"/><path d="M8 5v3l2 1.5"/>"#,
            Icon::Pencil => {
                r#"<path d="M3 13l.6-3L10.4 3.2a1.2 1.2 0 0 1 1.7 0l.7.7a1.2 1.2 0 0 1 0 1.7L6 12.4z"/><path d="M9.2 4.4l2.4 2.4"/>"#
            }
            Icon::Agents => {
                r#"<rect x="3" y="5" width="10" height="8" rx="2"/><path d="M8 2v3M6 9h.01M10 9h.01"/>"#
            }
            Icon::Skills => {
                r#"<path d="M2.5 3.5h4a1.5 1.5 0 0 1 1.5 1.5v8.5a1 1 0 0 0-1-1H2.5z"/><path d="M13.5 3.5h-4A1.5 1.5 0 0 0 8 5v8.5a1 1 0 0 1 1-1h4.5z"/>"#
            }
            Icon::McpServers => {
                r#"<path d="M6 2v3M10 2v3M4.5 5h7v2.5a3.5 3.5 0 0 1-7 0zM8 11v3"/>"#
            }
            Icon::Rules => r#"<path d="M8 2l5 2v4c0 3-2.2 5-5 6-2.8-1-5-3-5-6V4z"/>"#,
            Icon::Retro => r#"<path d="M3 8a5 5 0 1 0 1.5-3.5"/><path d="M3 2.5V5h2.5"/>"#,
            Icon::Experiments => {
                r#"<path d="M6 2h4M7 2v4l-3.5 6.5a1 1 0 0 0 .9 1.5h7.2a1 1 0 0 0 .9-1.5L9 6V2"/>"#
            }
            Icon::DecisionModel => {
                r#"<path d="M8 2v12M4.5 14h7M3 5h10M3 5l-1.5 4h3zM13 5l-1.5 4h3z"/>"#
            }
            Icon::Usage => r#"<path d="M2.5 13.5h11M4.5 11V8M8 11V4M11.5 11V6.5"/>"#,
            Icon::Models => {
                r#"<rect x="4" y="4" width="8" height="8" rx="1.5"/><path d="M6 2v2M10 2v2M6 12v2M10 12v2M2 6h2M2 10h2M12 6h2M12 10h2"/>"#
            }
            Icon::Plugins => r#"<path d="M3 5h3a1.5 1.5 0 1 1 3 0h3v3a1.5 1.5 0 1 0 0 3v3H3z"/>"#,
            Icon::Terminal => {
                r#"<rect x="2" y="3" width="12" height="10" rx="2"/><path d="M5 7l2 1.5L5 10M8.5 10.5H11"/>"#
            }
            Icon::Ports => {
                r#"<path d="M2.5 5.5h9M9 3l2.5 2.5L9 8M13.5 10.5h-9M7 8l-2.5 2.5L7 13"/>"#
            }
            Icon::Settings => {
                r#"<circle cx="8" cy="8" r="2.2"/><path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4"/>"#
            }
            Icon::Start => r#"<path d="M3.5 14V2.5"/><path d="M3.5 3h8.5l-2 3 2 3H3.5"/>"#,
            Icon::Search => r#"<circle cx="7" cy="7" r="4.5"/><path d="M10.5 10.5L14 14"/>"#,
            Icon::ChevronRight => r#"<path d="M6 3.5L10.5 8 6 12.5"/>"#,
            Icon::ChevronDown => r#"<path d="M3.5 6L8 10.5 12.5 6"/>"#,
            Icon::Spark => {
                r#"<path d="M8 2l1.4 4.1L13.5 8 9.4 9.9 8 14l-1.4-4.1L2.5 8l4.1-1.9z"/>"#
            }
            Icon::Spinner => r#"<path d="M8 2a6 6 0 1 0 6 6"/>"#,
            Icon::Minimise => r#"<path d="M3.5 8h9"/>"#,
            Icon::Folder => {
                r#"<path d="M2 4.5a1 1 0 0 1 1-1h3.2l1.5 1.5H13a1 1 0 0 1 1 1V12a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z"/>"#
            }
            Icon::File => r#"<path d="M4 2h5l3 3v9H4z"/><path d="M9 2v3h3"/>"#,
            Icon::Close => r#"<path d="M4.5 4.5l7 7M11.5 4.5l-7 7"/>"#,
            Icon::Panel => {
                r#"<rect x="2" y="2.5" width="12" height="11" rx="2"/><path d="M6 2.5v11"/>"#
            }
        }
    }

    /// The icon in `color`, `size` pixels wide.
    pub fn view<'a>(self, color: Color, size: f32) -> Svg<'a> {
        let source = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#000" stroke-width="1.5">{}</svg>"##,
            self.paths()
        );
        svg(svg::Handle::from_memory(source.into_bytes()))
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .style(move |_, _| svg::Style { color: Some(color) })
    }
}

/// The Moebius mark, a loop with one twist.
pub fn logo<'a>() -> Svg<'a> {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 26 18"><path d="M13 9 C 9 1, 2 1, 2 9 C 2 17, 9 17, 13 9 C 17 1, 24 1, 24 9 C 24 17, 17 17, 13 9" fill="none" stroke="#000" stroke-width="2.2"/></svg>"##;
    svg(svg::Handle::from_memory(source.as_bytes()))
        .width(Length::Fixed(theme::LOGO_WIDTH))
        .height(Length::Fixed(theme::LOGO_HEIGHT))
        .style(|_, _| svg::Style {
            color: Some(theme::TEXT),
        })
}
