use crate::builders::{self, *};
use chrono::{LocalResult, Locale, NaiveDate, NaiveDateTime, NaiveTime, prelude::*};
use egui_extras::{Column, TableBuilder};
use icalendar::{CalendarDateTime, Component, DatePerhapsTime, Event};
use regex::Regex;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::LazyLock;
use std::sync::mpsc::{self, Receiver, TryRecvError};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use web_sys::{Blob, BlobPropertyBag, Url};

static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([a-zA-Z]+): (.*)").unwrap());

fn get_date_string(i: &icalendar::Event) -> String {
    match i
        .get_start()
        .unwrap_or(chrono::Local::now().to_utc().into())
    {
        DatePerhapsTime::Date(naive_date) => {
            // 1. Handle all-day events by building a naive midnight
            let naive_dt = naive_date.and_time(NaiveTime::MIN);

            // 2. Map it safely to your local system's timezone
            match naive_dt.and_local_timezone(Local) {
                LocalResult::Single(dt) => dt.format_localized("%d. %b %Y %H:%M", Locale::de_DE),
                LocalResult::Ambiguous(dt, _) => {
                    dt.format_localized("%d. %b %Y %H:%M", Locale::de_DE)
                }
                LocalResult::None => naive_dt
                    .and_utc()
                    .with_timezone(&Local)
                    .format_localized("%d. %b %Y %H:%M", Locale::de_DE),
            }
            .to_string()
        }

        DatePerhapsTime::DateTime(calendar_dt) => {
            // 1. Get the underlying timezone-aware DateTime from the icalendar enum
            // 2. Shift the timezone safely to your local German system time
            calendar_dt
                .try_into_utc() // Decodes ical's internal representation safely to UTC
                .unwrap()
                .with_timezone(&Local) // <-- THIS SHIFTS UTC TO UTC+1 / UTC+2 DEPENDING ON THE DATE
                .format_localized("%d. %b %Y %H:%M", Locale::de_DE)
                .to_string()
        }
    }
}

fn get_local_time(i: &icalendar::Event) -> DateTime<Local> {
    match i
        .get_start()
        .unwrap_or(chrono::Local::now().to_utc().into())
    {
        DatePerhapsTime::Date(naive_date) => {
            // 1. Handle all-day events by building a naive midnight
            let naive_dt = naive_date.and_time(NaiveTime::MIN);

            // 2. Map it safely to your local system's timezone
            match naive_dt.and_local_timezone(Local) {
                LocalResult::Single(dt) => dt,
                LocalResult::Ambiguous(dt, _) => dt,
                LocalResult::None => naive_dt.and_utc().with_timezone(&Local),
            }
        }

        DatePerhapsTime::DateTime(calendar_dt) => {
            // 1. Get the underlying timezone-aware DateTime from the icalendar enum
            // 2. Shift the timezone safely to your local German system time
            calendar_dt
                .try_into_utc() // Decodes ical's internal representation safely to UTC
                .unwrap()
                .with_timezone(&Local)
        }
    }
}

struct CalendarEntry {
    date_time: DateTime<Local>,
    summary: String,
    calendar: String,
    props: HashMap<String, String>,
}

pub struct TemplateApp {
    calendar_links: String,
    events: Vec<CalendarEntry>,
    unique_properties: BTreeSet<String>,
    checked: Vec<bool>,
    pending_calendar_results: Option<Receiver<(usize, Result<String, String>)>>,
    pending_calendars: usize,
    fetched_calendars: Vec<Option<String>>,
    fetch_errors: Vec<String>,
    status: Option<String>,
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            events: vec![],
            calendar_links: "".to_owned(),
            unique_properties: BTreeSet::new(),
            checked: vec![false; 0],
            pending_calendar_results: None,
            pending_calendars: 0,
            fetched_calendars: vec![],
            fetch_errors: vec![],
            status: None,
        }
    }
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Default::default()
    }
}

impl eframe::App for TemplateApp {
    /// Called by the framework to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {}

    /// Called each time the UI needs repainting, which may be many times per second.
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        egui::Panel::top("top_panel").show(ui, |ui| {
            // The top panel is often a good place for a menu bar:

            egui::MenuBar::new().ui(ui, |ui| {
                // NOTE: no File->Quit on web pages!
                let is_web = cfg!(target_arch = "wasm32");
                if !is_web {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            ui.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(16.0);
                }

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            ui.heading("eframe template");

            ui.text_edit_multiline(&mut self.calendar_links);

            if ui.button("Generate").clicked() {
                self.events.clear();
                self.unique_properties.clear();
                self.checked.clear();
                self.fetched_calendars.clear();
                self.fetch_errors.clear();
                self.status = None;

                let urls = self
                    .calendar_links
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                if urls.is_empty() {
                    self.status = Some("Enter at least one calendar URL.".to_owned());
                } else {
                    let (sender, receiver) = mpsc::channel();
                    self.pending_calendars = urls.len();
                    self.fetched_calendars = (0..urls.len()).map(|_| None).collect();
                    self.pending_calendar_results = Some(receiver);
                    self.status = Some(format!("Loading {} calendar(s)...", urls.len()));

                    for (index, url) in urls.into_iter().enumerate() {
                        let sender = sender.clone();
                        let ctx = ui.ctx().clone();
                        let error_url = url.clone();
                        ehttp::fetch(ehttp::Request::get(&url), move |result| {
                            let result = result
                                .map_err(|error| format!("{error_url}: {error:?}"))
                                .and_then(|response| {
                                    if !response.ok {
                                        return Err(format!(
                                            "{error_url}: HTTP {}",
                                            response.status
                                        ));
                                    }
                                    String::from_utf8(response.bytes).map_err(|error| {
                                        format!("{error_url}: invalid UTF-8: {error}")
                                    })
                                });
                            let _ = sender.send((index, result));
                            ctx.request_repaint();
                        });
                    }
                }
            }

            let mut received = Vec::new();
            let mut disconnected = false;
            if let Some(receiver) = &self.pending_calendar_results {
                loop {
                    match receiver.try_recv() {
                        Ok(result) => received.push(result),
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            disconnected = true;
                            break;
                        }
                    }
                }
            }

            for (index, result) in received {
                self.pending_calendars -= 1;
                match result {
                    Ok(calendar) => self.fetched_calendars[index] = Some(calendar),
                    Err(error) => self.fetch_errors.push(error),
                }
            }

            if disconnected && self.pending_calendars > 0 {
                self.fetch_errors
                    .push("A calendar request ended unexpectedly.".to_owned());
                self.pending_calendars = 0;
            }

            if self.pending_calendar_results.is_some() && self.pending_calendars == 0 {
                self.pending_calendar_results = None;
                for text in self.fetched_calendars.drain(..).flatten() {
                    let unfolded = icalendar::parser::unfold(&text);
                    let calendar: icalendar::Calendar = unfolded.parse().unwrap();

                    for event in calendar.events() {

                        let prop_str = event.get_description().unwrap_or("");
                        self.events.push(CalendarEntry {
                            date_time: get_local_time(event),
                            summary: event.get_summary().unwrap_or("No summary").to_owned(),
                            calendar: calendar.get_name().unwrap_or("No Name").to_owned(),
                            props: RE
                                .captures_iter(prop_str)
                                .map(|capture| (capture[1].to_owned(), capture[2].to_owned()))
                                .collect(),
                        });
                    }
                }
                self.unique_properties = self
                    .events
                    .iter()
                    .flat_map(|event| event.props.keys())
                    .cloned()
                    .collect();
                println!("Unique properties: {:?}", self.unique_properties);
                self.checked = vec![false; self.unique_properties.len()];
                self.status = if self.fetch_errors.is_empty() {
                    Some(if cfg!(target_arch = "wasm32") {
                        "Calendars loaded. File export is available in native builds.".to_owned()
                    } else {
                        "Calendars loaded.".to_owned()
                    })
                } else {
                    Some(self.fetch_errors.join("\n"))
                };
            }

            if let Some(status) = &self.status {
                if status.starts_with("Loading") {
                    ui.spinner();
                }
                ui.label(status);
            }

            for (i, v) in self.unique_properties.iter().enumerate() {
                ui.checkbox(&mut self.checked[i], v);
            }

            let props_to_display = self
                .unique_properties
                .iter()
                .enumerate()
                .filter(|(i, _)| self.checked[*i])
                .map(|(i, v)| v)
                .collect::<Vec<&String>>();

            let mut builder = TableBuilder::new(ui)
                .column(Column::auto_with_initial_suggestion(200.0).resizable(true))
                .column(Column::auto_with_initial_suggestion(200.0).resizable(true));
            for v in props_to_display.iter() {
                builder =
                    builder.column(Column::auto_with_initial_suggestion(200.0).resizable(true));
            }
            builder
                .header(20.0, |mut header| {
                    if self.events.len() > 0 {
                        header.col(|ui| {
                            ui.heading("Datum");
                        });
                        header.col(|ui| {
                            ui.heading("Summary");
                        });
                        for v in props_to_display.iter() {
                            header.col(|ui| {
                                ui.heading(*v);
                            });
                        }
                    }
                })
                .body(|mut body| {
                    if self.events.len() > 0 {
                        for i in self.events.iter() {
                            body.row(30.0, |mut row| {
                                row.col(|ui| {
                                    ui.label(
                                        i.date_time
                                            .format_localized("%d. %b %Y %H:%M", Locale::de_DE)
                                            .to_string(),
                                    );
                                });
                                row.col(|ui| {
                                    ui.label(&i.summary);
                                });
                                for v in props_to_display.iter() {
                                    row.col(|ui| {
                                        ui.label(match i.props.get(*v) {
                                            Some(val) => val.to_owned(),
                                            None => "".to_owned(),
                                        });
                                    });
                                }
                            });
                        }
                    }
                });

            if !self.events.is_empty() {
                let button = ui.button("Generate PDF");
                if button.clicked() {
                    let pdf = self.generate_from_calendars();
                    store_or_offer_for_download("calendar.pdf", &pdf);
                }
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                powered_by_egui_and_eframe(ui);
                egui::warn_if_debug_build(ui);
            });
        });
    }
}

pub fn store_or_offer_for_download(filename: &str, content: &[u8]) {
    #[cfg(target_arch = "wasm32")]
    {
        let array = js_sys::Uint8Array::from(content);

        let parts = js_sys::Array::new();
        parts.push(&array);

        let options = BlobPropertyBag::new();
        options.set_type("application/pdf");

        let blob = Blob::new_with_u8_array_sequence_and_options(&parts, &options).unwrap();

        let url = Url::create_object_url_with_blob(&blob).unwrap();

        let window = web_sys::window().unwrap();
        let document = window.document().unwrap();

        let a = document
            .create_element("a")
            .unwrap()
            .dyn_into::<web_sys::HtmlAnchorElement>()
            .unwrap();

        a.set_href(&url);
        a.set_download(filename);
        a.click();

        Url::revoke_object_url(&url).unwrap();
    }
    #[cfg(not(target_arch = "wasm32"))]
    {}
}

impl TemplateApp {
    fn generate_from_calendars(&mut self) -> Vec<u8> {
        let mut builder =
            builders::HtmlBuilder::new().add_headings(&["Datum", "Summary"]);

            for event in self.events.iter() {
                                    builder = builder.add_row(&Box::<Vec<&str>>::new(vec![
                            &event.date_time.format_localized("%d. %b %Y %H:%M", Locale::de_DE).to_string(),
                            &event.summary
                        ]));
                                    }

        let html = builder.build();
        ironpress::html_to_pdf(&html).unwrap()
    }
}

fn powered_by_egui_and_eframe(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label("Powered by ");
        ui.hyperlink_to("egui", "https://github.com/emilk/egui");
        ui.label(" and ");
        ui.hyperlink_to(
            "eframe",
            "https://github.com/emilk/egui/tree/master/crates/eframe",
        );
        ui.label(".");
    });
}
