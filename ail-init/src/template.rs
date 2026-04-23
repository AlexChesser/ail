use std::path::PathBuf;

/// Metadata for a template — the lightweight view returned by `TemplateSource::list()`.
#[derive(Debug, Clone)]
pub struct TemplateMeta {
    pub name: String,
    pub aliases: Vec<String>,
    pub short_description: String,
    pub tags: Vec<String>,
}

impl TemplateMeta {
    pub fn matches(&self, name_or_alias: &str) -> bool {
        self.name == name_or_alias || self.aliases.iter().any(|a| a == name_or_alias)
    }
}

/// A single file in a template, keyed by its install-relative path.
///
/// The `relative_path` is relative to the install root (`$CWD/.ail/`), so a
/// file from `demo/oh-my-ail/agents/atlas.ail.yaml` has a `relative_path` of
/// `agents/atlas.ail.yaml` and lands at `$CWD/.ail/agents/atlas.ail.yaml`.
#[derive(Debug, Clone)]
pub struct TemplateFile {
    pub relative_path: PathBuf,
    pub contents: Vec<u8>,
}

/// A fully-fetched template — metadata plus every file the user will receive.
#[derive(Debug, Clone)]
pub struct Template {
    pub meta: TemplateMeta,
    pub files: Vec<TemplateFile>,
}
