use cosmic::cosmic_config::{self, cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry};
use cosmic::iced::core::widget::Id as TextInputId;
use cosmic::iced::event::{listen_with, Event as IcedEvent};
use cosmic::iced::futures::SinkExt;
use cosmic::iced::keyboard::{key::Named, Event as KeyEvent, Key};
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{
    destroy_popup, get_popup,
};
use cosmic::iced::{Alignment, Length, Limits, Subscription};
use cosmic::iced::{window::Id, Task};
use cosmic::widget::{self, icon, image, text_input};
use cosmic::{theme, Element};

use crate::clipboard::{self, Clip};
use crate::history::{
    ensure_private_dir, rel_time, write_private, Entry, History, Kind,
};
use crate::fl;

pub const APP_ID: &str = "dev.clipit.Clipit";

/// Do not render more rows than this in the popup; search narrows the rest.
const MAX_RENDER: usize = 200;
const PREVIEW_CHARS: usize = 96;
const SIZE_PRESETS: [usize; 5] = [50, 100, 250, 500, 1000];
const POLL_PRESETS: [u64; 4] = [300, 800, 1500, 3000];
const EXPIRE_PRESETS: [u64; 4] = [0, 1, 7, 30];
const EXPANDED_TEXT_HEIGHT: f32 = 160.0;
const EXPANDED_IMAGE_HEIGHT: f32 = 240.0;
const THUMBNAIL_HEIGHT: f32 = 56.0;

#[derive(Clone, Debug, CosmicConfigEntry, Eq, PartialEq)]
#[version = 3]
pub struct Config {
    pub history_size: usize,
    pub poll_ms: u64,
    pub expire_days: u64,
    pub capture_images: bool,
    pub capture_primary: bool,
    pub ignore_patterns: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            history_size: 500,
            poll_ms: 800,
            expire_days: 0,
            capture_images: true,
            capture_primary: false,
            ignore_patterns: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    Escape,
    Config(Config),
    ClipboardContent(Clip),
    Copy(String),
    Copied(Option<u64>),
    Delete(String),
    UndoDelete,
    TogglePin(String),
    ToggleExpand(String),
    ClearHistory,
    Search(String),
    NavUp,
    NavDown,
    Activate,
    ShowSettings(bool),
    HistorySize(i32),
    PollRate(i32),
    ExpireDays(i32),
    ToggleImages(bool),
    TogglePrimary(bool),
    ToggleHtmlView,
    IgnoreInput(String),
    IgnoreAdd,
    IgnoreRemove(String),
    Export,
}

pub struct App {
    core: cosmic::Core,
    popup: Option<Id>,
    config: Config,
    history: History,
    search: String,
    search_id: TextInputId,
    /// Hash of content we just placed on the clipboard, so the watcher
    /// does not re-record our own copy. Stays set until fresh external
    /// content arrives, so both clipboard targets are suppressed.
    last_set: Option<u64>,
    /// Entry id currently expanded to full view.
    expanded: Option<String>,
    /// Whether the expanded entry shows its HTML source instead of text.
    expanded_html: bool,
    /// Index into the visible list highlighted via keyboard navigation.
    selected: Option<usize>,
    /// Recently deleted entries for undo, newest delete first.
    last_deleted: Vec<Entry>,
    show_settings: bool,
    ignore_input: String,
    export_msg: Option<String>,
}

fn preview_text(entry: &Entry) -> String {
    match entry.kind {
        Kind::Image => "[image]".to_string(),
        Kind::Text => {
            let first = entry.text.lines().next().unwrap_or_default();
            let mut preview: String = first.chars().take(PREVIEW_CHARS).collect();
            if first.chars().count() > PREVIEW_CHARS {
                preview.push('…');
            }
            preview
        }
    }
}

fn localized_time(entry: &Entry) -> String {
    let time = rel_time(entry.ts);
    if time == "now" {
        fl!("time-now")
    } else {
        time
    }
}

fn cycle<T: Copy + PartialEq + PartialOrd>(presets: &[T], current: T, direction: i32) -> T {
    let idx = presets.iter().position(|p| *p >= current).unwrap_or(presets.len() - 1);
    let next = (idx as i32 + direction).clamp(0, presets.len() as i32 - 1);
    presets[next as usize]
}

fn step_button(id: &str, on_press: Message) -> Element<'static, Message> {
    widget::button::custom(icon::from_name(id).size(14).symbolic(true))
        .on_press(on_press)
        .class(theme::Button::Text)
        .into()
}

impl App {
    /// Entries visible in the popup (search filter + pin order + render cap).
    fn visible(&self) -> Vec<Entry> {
        let query = self.search.trim().to_lowercase();
        self.history
            .display()
            .into_iter()
            .filter(|e| {
                query.is_empty()
                    || e.text.to_lowercase().contains(&query)
                    || (e.kind == Kind::Image && "[image]".contains(&query))
            })
            .take(MAX_RENDER)
            .collect()
    }

    fn write_config(&self) {
        if let Ok(context) = cosmic_config::Config::new(APP_ID, Config::VERSION) {
            let _ = self.config.write_entry(&context);
        }
    }

    fn search_view(&self) -> Element<'_, Message> {
        let visible = self.visible();

        let mut list = widget::column::with_capacity(visible.len().min(MAX_RENDER));
        if visible.is_empty() {
            let message = if self.search.trim().is_empty() {
                fl!("no-history")
            } else {
                fl!("no-matches")
            };
            list = list.push(
                widget::container(widget::text::body(message))
                    .align_x(Alignment::Center)
                    .width(Length::Fill)
                    .padding(16),
            );
        }

        for (idx, entry) in visible.iter().enumerate() {
            if self.expanded.as_deref() == Some(entry.id.as_str()) {
                list = list.push(self.expanded_row(entry));
            } else {
                list = list.push(self.preview_row(entry, self.selected == Some(idx)));
            }
        }

        let stats = fl!(
            "stats",
            items = self.history.total(),
            pinned = self.history.pinned_count(),
        )
        ;

        let mut footer = widget::row::with_capacity(5)
            .push(widget::text::caption(stats))
            .push(widget::space::horizontal().width(Length::Fill))
            .align_y(Alignment::Center)
            .spacing(4);
        if !self.last_deleted.is_empty() {
            footer = footer.push(
                widget::button::custom(
                    icon::from_name("edit-undo-symbolic").size(14).symbolic(true),
                )
                .on_press(Message::UndoDelete)
                .class(theme::Button::Text),
            );
        }
        footer = footer
            .push(
                widget::button::custom(
                    icon::from_name("settings-symbolic").size(14).symbolic(true),
                )
                .on_press(Message::ShowSettings(true))
                .class(theme::Button::Text),
            )
            .push(
                widget::button::text(fl!("clear"))
                    .on_press(Message::ClearHistory)
                    .class(theme::Button::Text),
            );

        let content = widget::column::with_capacity(4)
            .push(
                widget::text_input(fl!("search-placeholder"), &self.search)
                    .id(self.search_id.clone())
                    .on_input(Message::Search)
                    .width(Length::Fill),
            )
            .push(
                widget::scrollable(list)
                    .width(Length::Fill)
                    .height(Length::Fixed(380.0)),
            )
            .push(widget::divider::horizontal::default())
            .push(footer)
            .spacing(8)
            .padding(8)
            // Fixed root size: autosize measures 1x1 when every child is
            // Fill/Shrink, which renders the popup invisibly small.
            .width(Length::Fixed(356.0));

        self.core.applet.popup_container(content).into()
    }

    fn preview_row(&self, entry: &Entry, selected: bool) -> Element<'_, Message> {
        let time = localized_time(entry);
        let content = match entry.kind {
            Kind::Image => Element::from(
                widget::image(
                    image::Handle::from_path(crate::history::image_path(
                        &crate::history::data_dir(),
                        entry.image.as_deref(),
                    )
                    .unwrap_or_default()),
                )
                .height(Length::Fixed(THUMBNAIL_HEIGHT)),
            ),
            Kind::Text => widget::column::with_capacity(2)
                .push(widget::text::body(preview_text(entry)))
                .push_maybe(
                    (!time.is_empty()).then(|| widget::text::caption(time.clone())),
                )
                .into(),
        };

        widget::row::with_capacity(4)
            .push(
                widget::button::custom(
                    widget::container(content)
                        .align_x(Alignment::Start)
                        .width(Length::Fill),
                )
                .on_press(Message::Copy(entry.id.clone()))
                .class(if selected {
                    theme::Button::Standard
                } else {
                    theme::Button::Text
                })
                .width(Length::Fill),
            )
            .push(
                widget::button::custom(pin_icon(entry.pinned))
                    .on_press(Message::TogglePin(entry.id.clone()))
                    .class(theme::Button::Text),
            )
            .push(
                widget::button::custom(
                    icon::from_name("pan-down-symbolic").size(16).symbolic(true),
                )
                .on_press(Message::ToggleExpand(entry.id.clone()))
                .class(theme::Button::Text),
            )
            .push(
                widget::button::custom(
                    icon::from_name("edit-delete-symbolic")
                        .size(16)
                        .symbolic(true),
                )
                .on_press(Message::Delete(entry.id.clone()))
                .class(theme::Button::Text),
            )
            .align_y(Alignment::Center)
            .spacing(4)
            .into()
    }

    fn expanded_row(&self, entry: &Entry) -> Element<'_, Message> {
        let mut body = widget::column::with_capacity(2).spacing(4);
        match entry.kind {
            Kind::Image => {
                if let Some(path) =
                    crate::history::image_path(&crate::history::data_dir(), entry.image.as_deref())
                {
                    body = body.push(
                        widget::image(image::Handle::from_path(path))
                            .height(Length::Fixed(EXPANDED_IMAGE_HEIGHT)),
                    );
                }
            }
            Kind::Text => {
                if self.expanded_html
                    && let Some(html) = &entry.html
                {
                    body = body.push(
                        widget::scrollable(widget::text::body(html.clone()))
                            .height(Length::Fixed(EXPANDED_TEXT_HEIGHT)),
                    );
                } else {
                    body = body.push(
                        widget::scrollable(widget::text::body(entry.text.clone()))
                            .height(Length::Fixed(EXPANDED_TEXT_HEIGHT)),
                    );
                }
                if entry.html.is_some() {
                    let label = if self.expanded_html {
                        fl!("show-text")
                    } else {
                        fl!("show-html")
                    };
                    body = body.push(
                        widget::button::text(label)
                            .on_press(Message::ToggleHtmlView)
                            .class(theme::Button::Text),
                    );
                }
            }
        }
        let time = localized_time(entry);
        let mut actions = widget::row::with_capacity(4).spacing(4);
        actions = actions.push(
            widget::button::text(fl!("copy"))
                .on_press(Message::Copy(entry.id.clone()))
                .class(theme::Button::Suggested),
        );
        if !time.is_empty() {
            actions = actions.push(widget::text::caption(time));
        }
        actions = actions.push(widget::space::horizontal().width(Length::Fill));
        actions = actions.push(
            widget::button::custom(icon::from_name("pan-up-symbolic").size(16).symbolic(true))
                .on_press(Message::ToggleExpand(entry.id.clone()))
                .class(theme::Button::Text),
        );
        actions = actions.push(
            widget::button::custom(pin_icon(entry.pinned))
                .on_press(Message::TogglePin(entry.id.clone()))
                .class(theme::Button::Text),
        );
        actions = actions.push(
            widget::button::custom(
                icon::from_name("edit-delete-symbolic").size(16).symbolic(true),
            )
            .on_press(Message::Delete(entry.id.clone()))
            .class(theme::Button::Text),
        );

        widget::column::with_capacity(2)
            .push(body)
            .push(actions.align_y(Alignment::Center))
            .push(widget::divider::horizontal::default())
            .spacing(4)
            .into()
    }

    fn settings_view(&self) -> Element<'_, Message> {
        let poll_label = fl!("poll-label", ms = self.config.poll_ms);
        let expire_label = match self.config.expire_days {
            0 => fl!("expire-never"),
            n => fl!("expire-days", days = n),
        };

        let mut patterns = widget::column::with_capacity(self.config.ignore_patterns.len() + 2);
        for pattern in &self.config.ignore_patterns {
            patterns = patterns.push(
                widget::row::with_capacity(2)
                    .push(widget::text::caption(pattern.clone()).width(Length::Fill))
                    .push(
                        widget::button::custom(
                            icon::from_name("window-close-symbolic")
                                .size(14)
                                .symbolic(true),
                        )
                        .on_press(Message::IgnoreRemove(pattern.clone()))
                        .class(theme::Button::Text),
                    )
                    .align_y(Alignment::Center),
            );
        }
        patterns = patterns.push(
            widget::row::with_capacity(2)
                .push(
                    widget::text_input(fl!("ignore-placeholder"), &self.ignore_input)
                        .on_input(Message::IgnoreInput)
                        .on_submit(|_| Message::IgnoreAdd)
                        .width(Length::Fill),
                )
                .push(step_button(
                    "list-add-symbolic",
                    Message::IgnoreAdd,
                ))
                .align_y(Alignment::Center),
        );
        patterns = patterns.push(
            widget::text::caption(fl!("ignore-hint"))
                .width(Length::Fill),
        );

        let mut data = widget::column::with_capacity(3).spacing(4);
        data = data.push(
            widget::button::custom(
                widget::row::with_capacity(2)
                    .push(icon::from_name("document-save-symbolic").size(14).symbolic(true))
                    .push(widget::text::body(fl!("export-history")))
                    .spacing(8),
            )
            .on_press(Message::Export)
            .class(theme::Button::Text),
        );
        if let Some(msg) = &self.export_msg {
            data = data.push(widget::text::caption(msg.clone()));
        }
        data = data.push(widget::text::caption(fl!(
            "stats",
            items = self.history.total(),
            pinned = self.history.pinned_count(),
        )
        ));

        widget::column::with_capacity(6)
            .push(
                widget::row::with_capacity(2)
                    .push(
                        widget::button::custom(
                            icon::from_name("go-previous-symbolic")
                                .size(16)
                                .symbolic(true),
                        )
                        .on_press(Message::ShowSettings(false))
                        .class(theme::Button::Text),
                    )
                    .push(widget::text::body(fl!("settings")))
                    .align_y(Alignment::Center)
                    .spacing(4),
            )
            .push(
                widget::scrollable(
                    widget::column::with_capacity(4)
                        .push(
                            widget::settings::section()
                                .title(fl!("history-section"))
                                .add(widget::settings::item(
                                    fl!("max-entries"),
                                    widget::row::with_capacity(3)
                                        .push(step_button(
                                            "list-remove-symbolic",
                                            Message::HistorySize(-1),
                                        ))
                                        .push(widget::text::caption(format!(
                                            "{}",
                                            self.config.history_size
                                        )))
                                        .push(step_button(
                                            "list-add-symbolic",
                                            Message::HistorySize(1),
                                        ))
                                        .align_y(Alignment::Center)
                                        .spacing(4),
                                ))
                                .add(widget::settings::item(
                                    fl!("expire-after"),
                                    widget::row::with_capacity(3)
                                        .push(step_button(
                                            "list-remove-symbolic",
                                            Message::ExpireDays(-1),
                                        ))
                                        .push(widget::text::caption(expire_label))
                                        .push(step_button(
                                            "list-add-symbolic",
                                            Message::ExpireDays(1),
                                        ))
                                        .align_y(Alignment::Center)
                                        .spacing(4),
                                )),
                        )
                        .push(
                            widget::settings::section()
                                .title(fl!("capture-section"))
                                .add(widget::settings::item(
                                    fl!("poll-rate"),
                                    widget::row::with_capacity(3)
                                        .push(step_button(
                                            "list-remove-symbolic",
                                            Message::PollRate(-1),
                                        ))
                                        .push(widget::text::caption(poll_label))
                                        .push(step_button(
                                            "list-add-symbolic",
                                            Message::PollRate(1),
                                        ))
                                        .align_y(Alignment::Center)
                                        .spacing(4),
                                ))
                                .add(widget::settings::item(
                                    fl!("save-images"),
                                    widget::toggler(self.config.capture_images)
                                        .on_toggle(Message::ToggleImages),
                                ))
                                .add(widget::settings::item(
                                    fl!("capture-primary"),
                                    widget::toggler(self.config.capture_primary)
                                        .on_toggle(Message::TogglePrimary),
                                )),
                        )
                        .push(
                            widget::settings::section()
                                .title(fl!("privacy-section"))
                                .add(patterns),
                        )
                        .push(
                            widget::settings::section()
                                .title(fl!("data-section"))
                                .add(data),
                        )
                        .spacing(8),
                )
                .height(Length::Fixed(430.0)),
            )
            .spacing(8)
            .padding(8)
            .width(Length::Fixed(356.0))
            .into()
    }

    fn copy_entry(&mut self, id: &str) -> Task<cosmic::Action<Message>> {
        let Some(entry) = self.history.get(id).cloned() else {
            return Task::none();
        };
        let task = match entry.kind {
            Kind::Text => {
                let hash = clipboard::hash_text(&entry.text);
                let clip = Clip::Text {
                    text: entry.text.clone(),
                    html: entry.html.clone(),
                };
                tokio::task::spawn_blocking(move || {
                    clipboard::set_clipboard(&clip);
                    Some(hash)
                })
            }
            Kind::Image => {
                let Some(path) = crate::history::image_path(
                    &crate::history::data_dir(),
                    entry.image.as_deref(),
                ) else {
                    return Task::none();
                };
                tokio::task::spawn_blocking(move || match std::fs::read(path) {
                    Ok(bytes) => {
                        let hash = clipboard::hash_bytes(&bytes);
                        let format =
                            clipboard::sniff_format(&bytes).unwrap_or(clipboard::ImageFormat::Png);
                        clipboard::set_clipboard(&Clip::Image { bytes, format });
                        Some(hash)
                    }
                    Err(why) => {
                        eprintln!("clipit: cannot read image: {why}");
                        None
                    }
                })
            }
        };
        self.history.restore(entry);
        self.history.trim(self.config.history_size);
        self.history.save();

        let mut tasks = vec![Task::perform(
            async move {
                let hash = task.await.ok().flatten();
                Message::Copied(hash)
            },
            cosmic::Action::from,
        )];
        if let Some(popup) = self.popup.take() {
            tasks.push(destroy_popup(popup));
        }
        Task::batch(tasks)
    }
}

fn pin_icon(pinned: bool) -> widget::icon::Named {
    if pinned {
        icon::from_name("starred-symbolic")
    } else {
        icon::from_name("non-starred-symbolic")
    }
}

/// Registers `Super+V -> clipit --toggle` in the COSMIC shortcuts config so
/// the popup can be opened globally. Idempotent; the binding shows up in
/// Settings → Keyboard where it can be changed or removed.
fn register_shortcut() {
    use cosmic_config::{ConfigGet, ConfigSet};
    use cosmic_settings_config::shortcuts::{self, Action, Binding, Shortcuts};
    use std::str::FromStr;

    const COMMAND: &str = "clipit --toggle";
    let Ok(context) = shortcuts::context() else {
        eprintln!("clipit: cannot open shortcuts config context");
        return;
    };
    let mut custom: Shortcuts = context
        .get("custom")
        .unwrap_or_else(|_| Shortcuts::default());
    if custom
        .0
        .values()
        .any(|action| matches!(action, Action::Spawn(command) if command == COMMAND))
    {
        return;
    }
    match Binding::from_str("Super+V") {
        Ok(binding) => {
            custom.0.insert(binding, Action::Spawn(COMMAND.to_string()));
            if let Err(why) = context.set("custom", custom) {
                eprintln!("clipit: cannot write shortcut: {why}");
            }
        }
        Err(why) => eprintln!("clipit: invalid shortcut binding: {why}"),
    }
}

/// D-Bus service: `dev.clipit.Clipit` / `/dev/clipit/Clipit`, method
/// `Toggle()` flips the popup. Used by `clipit --toggle`.
struct DbusToggle {
    sender: cosmic::iced::futures::channel::mpsc::Sender<Message>,
}

#[zbus::interface(name = "dev.clipit.Clipit")]
impl DbusToggle {
            async fn toggle(&self) {
                let mut sender = self.sender.clone();
                let _ = sender.send(Message::TogglePopup).await;
            }
}

fn dbus_service_stream() -> impl cosmic::iced::futures::Stream<Item = Message> {
    cosmic::iced::stream::channel(
        4,
        move |sender: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
            let result: Result<(), zbus::Error> = async move {
                let builder = zbus::connection::Builder::session()?;
                let builder = builder.name("dev.clipit.Clipit")?;
                let builder = builder.serve_at(
                    "/dev/clipit/Clipit",
                    DbusToggle {
                        sender: sender.clone(),
                    },
                )?;
                let connection = builder.build().await?;
                let _ = connection;
                cosmic::iced::futures::future::pending::<()>().await;
                Ok(())
            }
            .await;
            if let Err(why) = result {
                eprintln!("clipit: dbus service error: {why}");
            }
        },
    )
}

impl Default for App {
    fn default() -> Self {
        Self {
            core: cosmic::Core::default(),
            popup: None,
            config: Config::default(),
            history: History::default(),
            search: String::new(),
            search_id: TextInputId::unique(),
            last_set: None,
            expanded: None,
            expanded_html: false,
            selected: None,
            last_deleted: Vec::new(),
            show_settings: false,
            ignore_input: String::new(),
            export_msg: None,
        }
    }
}

impl cosmic::Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(
        core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        let config = cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
            .map(|context| match Config::get_entry(&context) {
                Ok(config) => config,
                Err((_errors, config)) => config,
            })
            .unwrap_or_default();

        let mut history = History::load();
        history.prune(config.expire_days);
        history.trim(config.history_size);

        register_shortcut();

        (
            App {
                core,
                config,
                history,
                ..Default::default()
            },
            Task::none(),
        )
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn view(&self) -> Element<'_, Self::Message> {
        self.core
            .applet
            .icon_button("edit-paste-symbolic")
            .on_press(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        if self.show_settings {
            self.settings_view()
        } else {
            self.search_view()
        }
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let mut subs = vec![
            clipboard::watch(
                self.config.poll_ms,
                self.config.capture_images,
                self.config.capture_primary,
            )
            .map(Message::ClipboardContent),
            self.core
                .watch_config::<Config>(Self::APP_ID)
                .map(|update| Message::Config(update.config)),
            Subscription::run(dbus_service_stream),
        ];

        // Keyboard navigation while the popup is open. Unlike
        // keyboard::listen(), listen_with() also sees keys the focused
        // search field consumed (arrow keys). Key events only reach this
        // process while one of its windows has focus, which is the popup.
        if self.popup.is_some() {
            subs.push(listen_with(|event, _status, _window| match event {
                IcedEvent::Keyboard(KeyEvent::KeyPressed {
                    key: Key::Named(name),
                    ..
                }) => match name {
                    Named::ArrowDown => Some(Message::NavDown),
                    Named::ArrowUp => Some(Message::NavUp),
                    Named::Enter => Some(Message::Activate),
                    Named::Escape => Some(Message::Escape),
                    _ => None,
                },
                _ => None,
            }));
        }

        Subscription::batch(subs)
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::TogglePopup => {
                return if let Some(popup) = self.popup.take() {
                    destroy_popup(popup)
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    self.selected = None;
                    self.expanded = None;
                    self.expanded_html = false;
                    self.search.clear();
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(420.0)
                        .min_width(300.0)
                        .min_height(200.0)
                        .max_height(640.0);
                    Task::batch(vec![
                        get_popup(popup_settings),
                        text_input::focus::<Message>(self.search_id.clone())
                            .map(cosmic::Action::from),
                    ])
                };
            }
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
            Message::Escape => {
                if let Some(popup) = self.popup.take() {
                    return destroy_popup(popup);
                }
            }
            Message::Config(config) => {
                self.config = config;
                self.history.prune(self.config.expire_days);
                self.history.trim(self.config.history_size);
            }
            Message::ClipboardContent(content) => match content {
                Clip::Text { text, html } => {
                    let hash = clipboard::hash_text(&text);
                    if self.last_set == Some(hash) {
                        return Task::none();
                    }
                    if text.trim().is_empty() {
                        return Task::none();
                    }
                    if self
                        .history
                        .add_text(text, html, &self.config.ignore_patterns)
                    {
                        self.last_set = None;
                        self.history.prune(self.config.expire_days);
                        self.history.trim(self.config.history_size);
                        self.history.save();
                    }
                }
                Clip::Image { bytes, format } => {
                    if self.history.add_image(&bytes, format.ext()) {
                        self.history.prune(self.config.expire_days);
                        self.history.trim(self.config.history_size);
                        self.history.save();
                    }
                }
            },
            Message::Copy(id) => return self.copy_entry(&id),
            Message::Copied(hash) => {
                if let Some(hash) = hash {
                    self.last_set = Some(hash);
                }
            }
            Message::Delete(id) => {
                if let Some(removed) = self.history.delete(&id) {
                    self.last_deleted.push(removed);
                    if self.last_deleted.len() > 20 {
                        self.last_deleted.remove(0);
                    }
                    self.history.save();
                }
                if self.expanded.as_deref() == Some(&id) {
                    self.expanded = None;
                }
            }
            Message::UndoDelete => {
                if let Some(entry) = self.last_deleted.pop() {
                    self.history.restore(entry);
                    self.history.save();
                }
            }
            Message::TogglePin(id) => {
                self.history.toggle_pin(&id);
                self.history.save();
            }
            Message::ToggleExpand(id) => {
                self.expanded = if self.expanded.as_deref() == Some(id.as_str()) {
                    None
                } else {
                    Some(id)
                };
                self.expanded_html = false;
            }
            Message::ToggleHtmlView => {
                self.expanded_html = !self.expanded_html;
            }
            Message::TogglePrimary(enabled) => {
                self.config.capture_primary = enabled;
                self.write_config();
            }
            Message::ClearHistory => {
                self.history.clear();
                self.history.save();
            }
            Message::Search(search) => {
                self.search = search;
                self.selected = None;
            }
            Message::NavUp | Message::NavDown => {
                let len = self.visible().len();
                if len == 0 {
                    return Task::none();
                }
                let delta = if matches!(message, Message::NavDown) {
                    1
                } else {
                    -1
                };
                let current = self.selected.unwrap_or(0);
                self.selected = Some(
                    (current as i32 + delta).clamp(0, len as i32 - 1) as usize,
                );
            }
            Message::Activate => {
                if let Some(idx) = self.selected {
                    let visible = self.visible();
                    if let Some(entry) = visible.get(idx) {
                        let id = entry.id.clone();
                        return self.copy_entry(&id);
                    }
                }
            }
            Message::ShowSettings(show) => {
                self.show_settings = show;
                self.export_msg = None;
            }
            Message::HistorySize(direction) => {
                let next = cycle(&SIZE_PRESETS, self.config.history_size, direction);
                if next != self.config.history_size {
                    self.config.history_size = next;
                    self.write_config();
                    self.history.trim(next);
                    self.history.save();
                }
            }
            Message::PollRate(direction) => {
                let next = cycle(&POLL_PRESETS, self.config.poll_ms, direction);
                if next != self.config.poll_ms {
                    self.config.poll_ms = next;
                    self.write_config();
                }
            }
            Message::ExpireDays(direction) => {
                let next = cycle(&EXPIRE_PRESETS, self.config.expire_days, direction);
                if next != self.config.expire_days {
                    self.config.expire_days = next;
                    self.write_config();
                    self.history.prune(next);
                    self.history.save();
                }
            }
            Message::ToggleImages(enabled) => {
                self.config.capture_images = enabled;
                self.write_config();
            }
            Message::IgnoreInput(input) => self.ignore_input = input,
            Message::IgnoreAdd => {
                let pattern = self.ignore_input.trim().to_string();
                if !pattern.is_empty() && !self.config.ignore_patterns.contains(&pattern) {
                    self.config.ignore_patterns.push(pattern);
                    self.write_config();
                }
                self.ignore_input.clear();
            }
            Message::IgnoreRemove(pattern) => {
                self.config.ignore_patterns.retain(|p| p != &pattern);
                self.write_config();
            }
            Message::Export => {
                self.export_msg = Some(export_history(&self.history));
            }
        }
        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

/// Writes a timestamped export folder with history.json and all image files.
fn export_history(history: &History) -> String {
    let base = match std::env::var_os("XDG_DOCUMENTS_DIR") {
        Some(dir) => std::path::PathBuf::from(dir),
        None => {
            let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
            let docs = home.join("Documents");
            if docs.is_dir() {
                docs
            } else {
                home
            }
        }
    };
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dir = base.join(format!("clipit-export-{ts}"));
    if let Err(why) = ensure_private_dir(&dir.join("images")) {
        return fl!("export-failed", error = why.to_string());
    }
    let entries = history.display();
    for entry in &entries {
        if let Some(file) = &entry.image
            && let Some(src) = crate::history::image_path(&crate::history::data_dir(), Some(file))
            && let Ok(bytes) = std::fs::read(src)
        {
            let _ = write_private(&dir.join("images").join(file), &bytes);
        }
    }
    match serde_json::to_string_pretty(&entries) {
        Ok(json) => match write_private(&dir.join("history.json"), json.as_bytes()) {
            Ok(_) => fl!("exported-to", path = dir.display().to_string()),
            Err(why) => fl!("export-failed", error = why.to_string()),
        },
        Err(why) => fl!("export-failed", error = why.to_string()),
    }
}
