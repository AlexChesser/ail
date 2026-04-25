use crate::template::{Template, TemplateMeta};

pub mod bundled;
pub mod url;

/// A source of templates — bundled, local directory, or (future) remote registry.
///
/// Sources are polled in order by the resolver; the first source that returns
/// `Some` from `fetch()` wins. Implementations must be cheap to `list()` and
/// may do heavier work in `fetch()`.
pub trait TemplateSource {
    fn list(&self) -> Vec<TemplateMeta>;
    fn fetch(&self, name_or_alias: &str) -> Option<Template>;
}
