use crate::builders::{self, *};
use chrono::{LocalResult, Locale, NaiveDate, NaiveDateTime, NaiveTime, prelude::*};

use egui_extras::{Column, TableBuilder};
use icalendar::{CalendarDateTime, Component, DatePerhapsTime, Event};
use regex::Regex;
use std::collections::{BTreeSet, HashMap, HashSet};

use std::sync::LazyLock;

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

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct TemplateApp {
    calendar_links: String,

    #[serde(skip)] // This how you opt-out of serialization of a field
    events: Vec<CalendarEntry>,
    unique_properties: BTreeSet<String>,
    checked: Vec<bool>,
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            events: vec![],
            calendar_links: "".to_owned(),
            unique_properties: BTreeSet::new(),
            checked: vec![false; 0],
        }
    }
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        }
    }
}

impl eframe::App for TemplateApp {
    /// Called by the framework to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

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

                let mut builder = builders::HtmlBuilder::new().add_headings(&vec![
                    "Datum",
                    "Summary",
                    "Description",
                ]);

                for line in self.calendar_links.split("\n") {
                    let response = reqwest::blocking::get(line).expect("Failed to make request");
                    let text: String =
                        String::from_utf8(response.bytes().expect("No bytes").to_vec())
                            .expect("msg");

                    let unfolded = icalendar::parser::unfold(&text);

                    let calendar: icalendar::Calendar = unfolded.parse().unwrap();

                    for i in calendar.events() {
                        builder = builder.add_row(&Box::<Vec<&str>>::new(vec![
                            &format!("{}", get_date_string(i)),
                            &i.get_summary().unwrap_or("No summary").to_string(),
                            &i.get_description().unwrap_or("No description").to_string(),
                        ]));

                        let prop_str = i.get_description().unwrap_or("");

                        self.events.push(CalendarEntry {
                            date_time: get_local_time(i),
                            summary: i.get_summary().unwrap_or("No summary").to_owned(),
                            calendar: calendar.get_name().unwrap_or("No Name").to_owned(),
                            props: RE
                                .captures_iter(prop_str)
                                .map(|v| (v[1].to_owned(), v[2].to_owned()))
                                .collect(),
                        })
                    }
                }

                let s = builder.build();

                std::fs::write("output.html", &s).expect("Failed to write PDF");

                let pdf = ironpress::html_to_pdf(&s).unwrap();
                std::fs::write("output.pdf", pdf).unwrap();

                self.unique_properties = self
                    .events
                    .iter()
                    .flat_map(|ev| ev.props.keys())
                    .map(|x| x.to_owned())
                    .collect();
                self.checked = vec![false; self.unique_properties.len()];
            }

            for (i, v) in self.unique_properties.iter().enumerate() {
                ui.checkbox(&mut self.checked[i], v);
            }

            let props_to_display = self
                .unique_properties
                .iter()
                .enumerate()
                .filter(|(i, _)| {
                    self.checked[*i]
                }).map(|(i,v)| v)
                .collect::<Vec<&String>>();

            let mut builder = TableBuilder::new(ui)
                .column(Column::auto_with_initial_suggestion(200.0).resizable(true))
                .column(Column::auto_with_initial_suggestion(200.0).resizable(true));
            for v in props_to_display.iter() {
                builder = builder.column(Column::auto_with_initial_suggestion(200.0).resizable(true));
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

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                powered_by_egui_and_eframe(ui);
                egui::warn_if_debug_build(ui);
            });
        });
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
