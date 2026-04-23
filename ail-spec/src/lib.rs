mod generated {
    include!(concat!(env!("OUT_DIR"), "/embedded_specs.rs"));
}

pub use generated::SpecSection;

pub fn section(id: &str) -> Option<&'static str> {
    generated::section_content(id)
}

pub fn list_sections() -> &'static [SpecSection] {
    generated::SECTIONS
}

pub fn full_prose() -> String {
    generated::full_prose_fn()
}

pub fn core_prose() -> String {
    generated::core_prose_fn()
}

pub fn runner_prose() -> String {
    generated::runner_prose_fn()
}

pub fn compact() -> &'static str {
    generated::COMPACT
}

pub fn schema() -> &'static str {
    generated::SCHEMA
}
