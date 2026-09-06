use rust_statement_spacing_core::Config;
use rust_statement_spacing_core::Rule;
use rust_statement_spacing_core::config::{ControlFlow, ErrorHandling, Exits, Grouping, Items};
use serde::Deserialize;

use crate::workspace::Workspace;

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ConfigTable {
    schema_version: u32,
    enable: Vec<Rule>,
    disable: Vec<Rule>,
    include_generated: bool,
    exclude: Vec<String>,
    grouping: Grouping,
    control_flow: ControlFlow,
    exits: Exits,
    error_handling: ErrorHandling,
    items: Items,
}

impl Default for ConfigTable {
    fn default() -> Self {
        let policy = Config::default();
        Self {
            schema_version: policy.schema_version,
            enable: policy.enable,
            disable: policy.disable,
            include_generated: false,
            exclude: vec!["target/**".into(), "vendor/**".into()],
            grouping: policy.grouping,
            control_flow: policy.control_flow,
            exits: policy.exits,
            error_handling: policy.error_handling,
            items: policy.items,
        }
    }
}

impl ConfigTable {
    pub(crate) fn resolve(self, root: std::path::PathBuf) -> Result<(Config, Workspace), String> {
        let policy = Config {
            schema_version: self.schema_version,
            enable: self.enable,
            disable: self.disable,
            grouping: self.grouping,
            control_flow: self.control_flow,
            exits: self.exits,
            error_handling: self.error_handling,
            items: self.items,
        };
        policy.validate()?;
        let workspace = Workspace::new(root, self.include_generated, &self.exclude)?;
        Ok((policy, workspace))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<(Config, Workspace), String> {
        let value: toml::Value = toml::from_str(text).map_err(|error| error.to_string())?;
        let table = match value.get("statement_spacing") {
            Some(value) => value
                .clone()
                .try_into::<ConfigTable>()
                .map_err(|error| error.to_string())?,
            None => ConfigTable::default(),
        };
        table.resolve(std::path::PathBuf::from("/workspace"))
    }

    #[test]
    fn partial_nested_configuration_retains_defaults() -> Result<(), Box<dyn std::error::Error>> {
        let (config, _) = parse("[statement_spacing.grouping]\nmax_before_control = 3\n")?;
        assert_eq!(config.grouping.max_before_control, 3);
        assert!(config.grouping.same_receiver);
        assert_eq!(
            config.grouping.bindings,
            rust_statement_spacing_core::config::Bindings::Consecutive
        );
        Ok(())
    }

    #[test]
    fn unknown_library_table_is_allowed() -> Result<(), Box<dyn std::error::Error>> {
        let (config, _) = parse("[other]\ncustom=42\n")?;
        assert!(config.enabled().has(Rule::Bindings));
        Ok(())
    }
    #[test]
    fn selection_overrides_preserve_workspace_safety() -> Result<(), Box<dyn std::error::Error>> {
        let (_, defaults) = parse("")?;
        let source = "// @generated\nfn generated() {}\n";
        let path = std::path::Path::new("/workspace/src/generated.rs");
        assert!(!defaults.includes(path, source));
        assert!(!defaults.includes(std::path::Path::new("/workspace/vendor/file.rs"), ""));

        let (_, configured) =
            parse("[statement_spacing]\ninclude_generated=true\nexclude=['src/ignored/**']")?;
        assert!(configured.includes(path, source));
        assert!(configured.includes(std::path::Path::new("/workspace/vendor/file.rs"), ""));
        assert!(!configured.includes(std::path::Path::new("/workspace/src/ignored/file.rs"), ""));
        assert!(!configured.includes(std::path::Path::new("/workspace/nested/target/file.rs"), ""));
        assert!(!configured.includes(std::path::Path::new("/outside/file.rs"), ""));
        Ok(())
    }

    #[test]
    fn invalid_configuration_is_rejected() {
        for text in [
            "[statement_spacing]\nunknown=true",
            "[statement_spacing]\npreset='recommended'",
            "[statement_spacing.grouping]\nmaximum_before_control=2",
            "[statement_spacing.grouping]\nexpressions='typo'",
            "[statement_spacing]\nschema_version=2",
            "[statement_spacing]\nenable=['exit']\ndisable=['exit']",
            "[statement_spacing]\nenable=['exit','exit']",
            "[statement_spacing.grouping]\nmax_before_control=1025",
            "[statement_spacing]\nexclude=['../outside/**']",
            "[statement_spacing]\nexclude=['[']",
        ] {
            assert!(parse(text).is_err(), "accepted {text}");
        }
    }
}
