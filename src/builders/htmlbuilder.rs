use crate::builders::Builder;
use tera::Tera;

pub struct HtmlBuilder {
    header: Vec<String>,
    lines: Vec<Vec<String>>,
    columns: i16,
}

impl Builder for HtmlBuilder {
    fn new() -> Self {
        Self {
            header: vec![],
            lines: vec![],
            columns: 0,
        }
    }
    fn add_headings(mut self, items: &[&str]) -> Self {
        self.columns = items.len() as i16;
        eprintln!("{}", self.columns);

        for &item in items {
            self.header.push(item.to_owned());
        }

        self
    }
    fn add_row(mut self, items: &[&str]) -> Self {
        self.lines
            .push(items.iter().map(|i| (*i).to_owned()).collect());

        self
    }
    fn build(self) -> String {
        let mut tera = Tera::default();

        tera.add_raw_templates(vec![("table.html", include_str!("templates/table.html"))])
            .unwrap();

        let mut context = tera::Context::new();
        context.insert("headers", &self.header);
        context.insert("rows", &self.lines);

        tera.render("table.html", &context).unwrap()
    }
}
