use crate::builders::Builder;

pub struct MdBuilder {
    lines: Vec<String>,
    columns: i16,
}

impl Builder for MdBuilder {
    fn new() -> Self {
        Self {
            lines: vec![],
            columns: 0,
        }
    }
    fn add_headings(mut self, items: &[&str]) -> Self {

        self.columns = items.len() as i16;
        eprintln!("{}", self.columns);

        let mut builder: String = "|".to_owned();

        for item in items {
            builder += item;
            builder += "|";
        }

        self.lines.push(builder);

        builder = "| ".to_owned();

        for _ in 0..self.columns {
            eprintln!("Add column, cs: {}", self.columns);
            builder += ":---: |"
        }
        self.lines.push(builder);

        self
    }
    fn add_row(mut self, items: &[&str]) -> Self {
        let mut builder: String = "|".to_owned();

        for item in items {
            builder += item;
            builder += "|";
        }

        self.lines.push(builder);

        self
    }
    fn build(self) -> String {
        self.lines.join("\n")
    }
}
