use crate::command::CommandOutcome;

pub struct SpecCommand {
    format: SpecFormat,
    sections: Option<Vec<String>>,
    list: bool,
    core_only: bool,
    runner_only: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum SpecFormat {
    #[default]
    Prose,
    Compact,
    Schema,
}

impl SpecFormat {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "prose" => Ok(SpecFormat::Prose),
            "compact" => Ok(SpecFormat::Compact),
            "schema" => Ok(SpecFormat::Schema),
            other => Err(format!(
                "unknown spec format: '{other}' (expected prose, compact, or schema)"
            )),
        }
    }
}

impl SpecCommand {
    pub fn new(
        format: SpecFormat,
        sections: Option<Vec<String>>,
        list: bool,
        core_only: bool,
        runner_only: bool,
    ) -> Self {
        Self {
            format,
            sections,
            list,
            core_only,
            runner_only,
        }
    }

    pub fn execute(&self) -> CommandOutcome {
        if self.list {
            return self.run_list();
        }

        if let Some(ref sections) = self.sections {
            return self.run_sections(sections);
        }

        self.run_full()
    }

    fn run_list(&self) -> CommandOutcome {
        let sections = ail_spec::list_sections();
        let mut last_group = "";

        for s in sections {
            if self.core_only && s.category != "core" {
                continue;
            }
            if self.runner_only && s.category != "runner" {
                continue;
            }

            // Print group header when group changes
            if s.group != last_group {
                if !last_group.is_empty() {
                    println!();
                }
                println!("## {}", s.group);
                last_group = s.group;
            }

            println!("{:<6} {:>6} words  {}", s.id, s.word_count, s.title);
        }
        CommandOutcome::Success
    }

    fn run_sections(&self, ids: &[String]) -> CommandOutcome {
        let mut first = true;
        for id in ids {
            match ail_spec::section(id) {
                Some(content) => {
                    if !first {
                        println!();
                    }
                    print!("{content}");
                    first = false;
                }
                None => {
                    eprintln!("Unknown spec section: '{id}'");
                    eprintln!("Run `ail spec --list` to see available sections.");
                    return CommandOutcome::ExitCode(1);
                }
            }
        }
        CommandOutcome::Success
    }

    fn run_full(&self) -> CommandOutcome {
        match self.format {
            SpecFormat::Schema => {
                print!("{}", ail_spec::schema());
            }
            SpecFormat::Compact => {
                print!("{}", ail_spec::compact());
            }
            SpecFormat::Prose => {
                if self.core_only {
                    print!("{}", ail_spec::core_prose());
                } else if self.runner_only {
                    print!("{}", ail_spec::runner_prose());
                } else {
                    print!("{}", ail_spec::full_prose());
                }
            }
        }
        CommandOutcome::Success
    }
}
