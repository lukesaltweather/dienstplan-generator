pub mod mdbuilder;
pub mod htmlbuilder;

pub trait Builder {
    fn new() -> Self;
    fn add_headings(self, items: &[&str]) -> Self;
    fn add_row(self, items: &[&str]) -> Self;
    fn build(self) -> String;
}

pub use mdbuilder::MdBuilder;
pub use htmlbuilder::HtmlBuilder;