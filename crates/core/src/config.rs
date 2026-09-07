//! The public configuration schema. Unknown keys and unimplemented modes are errors.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::model::{Rule, RuleMask};

macro_rules! option_enum {
    ($(#[$meta:meta])* $name:ident, $default:ident, $($(#[$variant_meta:meta])* $variant:ident => $text:literal),+ $(,)?) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
        pub enum $name {
            $($(#[$variant_meta])* #[serde(rename = $text)] $variant),+
        }
        impl Default for $name {
            fn default() -> Self { return Self::$default; }
        }
    };
}

option_enum!(
    /// Policy for adjacent bindings and assignments.
    Bindings, Consecutive,
    /// Keep consecutive bindings together.
    Consecutive => "consecutive",
    /// Join only bindings of the same syntactic kind.
    SameKind => "same-kind",
    /// Join bindings with a known semantic relationship.
    Related => "related",
    /// Retain existing binding spacing.
    Preserve => "preserve");
option_enum!(
    /// Policy for adjacent ordinary expressions.
    Expressions, Related,
    /// Group semantically related expressions.
    Related => "related",
    /// Separate ordinary expressions.
    Strict => "strict",
    /// Retain existing expression spacing.
    Preserve => "preserve");
option_enum!(
    /// How `self` field projections participate in relationships.
    SelfFields, Root,
    /// Treat sibling fields as distinct places.
    Distinct => "distinct",
    /// Relate all projections of the same `self` binding.
    Root => "root");
option_enum!(
    /// How excess setup is separated from control flow.
    Overflow, WholeGroup,
    /// Retain only a bounded related suffix.
    RelatedSuffix => "related-suffix",
    /// Separate the complete setup group when it exceeds the limit.
    WholeGroup => "whole-group");
option_enum!(
    /// Scope of reads used to associate setup with control flow.
    UseIn, HeaderOrFirstBodyStatement,
    /// Consider only header reads.
    Header => "header",
    /// Include reads from the first direct body statement.
    HeaderOrFirstBodyStatement => "header-or-first-body-statement",
    /// Include all non-deferred body reads.
    WholeBody => "whole-body");
option_enum!(
    /// Whether a category requires a blank-line boundary.
    Separation, Separate,
    /// Require a separating blank line.
    Separate => "separate",
    /// Retain existing spacing for this category.
    Preserve => "preserve");
option_enum!(
    /// Treatment of consecutive short exiting guards.
    GuardChain, Allow,
    /// Permit guards to remain adjacent.
    Allow => "allow",
    /// Apply normal separation between guards.
    Separate => "separate");
option_enum!(
    /// Spacing policy for a block's final value.
    Tail, Smart,
    /// Consider block size and immediate producer relationships.
    Smart => "smart",
    /// Require separation before the tail value.
    AlwaysSeparate => "always-separate",
    /// Retain existing tail spacing.
    Preserve => "preserve");
option_enum!(
    /// Treatment of a compiler-confirmed immediate Result/Option check.
    ImmediateCheck, Join,
    /// Keep the producer and check adjacent.
    Join => "join",
    /// Apply ordinary grouping and separation policies.
    Ordinary => "ordinary");

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
/// Complete spacing configuration; omitted fields use their documented defaults.
pub struct Config {
    /// Configuration schema version; must be 1.
    pub schema_version: u32,
    /// Additional checks to enable beyond default rules.
    pub enable: Vec<Rule>,
    /// Checks to disable from default rules.
    pub disable: Vec<Rule>,
    /// Relationships and limits for statement groups.
    pub grouping: Grouping,
    /// Spacing after blocks and between guards.
    pub control_flow: ControlFlow,
    /// Block-size and tail-value policy.
    pub exits: Exits,
    /// Policy for immediate Result/Option checks.
    pub error_handling: ErrorHandling,
    /// Spacing between declaration categories.
    pub items: Items,
}

impl Default for Config {
    fn default() -> Self {
        return Self {
            schema_version: 1,
            enable: Vec::new(),
            disable: Vec::new(),
            grouping: Grouping::default(),
            control_flow: ControlFlow::default(),
            exits: Exits::default(),
            error_handling: ErrorHandling::default(),
            items: Items::default(),
        };
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
/// Statement relationship policy and control-flow setup limits.
#[expect(
    clippy::struct_excessive_bools,
    reason = "Grouping heuristics are independently configurable boolean keys in the public configuration schema."
)]
pub struct Grouping {
    /// Binding policy; defaults to consecutive.
    pub bindings: Bindings,
    /// Expression policy; defaults to related.
    pub expressions: Expressions,
    /// Whether a shared method receiver establishes relatedness; defaults to true.
    pub same_receiver: bool,
    /// Whether shared reads establish relatedness; defaults to true.
    pub shared_inputs: bool,
    /// Whether accesses to the same aggregate establish relatedness; defaults to true.
    pub same_object: bool,
    /// Whether direct calls to the same resolved function establish relatedness; defaults to true.
    pub same_callee: bool,
    /// Whether consecutive assignments or mutable receiver calls relate; defaults to true.
    pub consecutive_mutations: bool,
    /// `self` projection matching; defaults to the same receiver root.
    pub self_fields: SelfFields,
    /// Maximum ordinary setup units attached to control flow; defaults to 1.
    pub max_before_control: usize,
    /// How excess setup is split; defaults to preserving the whole group.
    pub overflow: Overflow,
    /// Reads considered for setup; defaults to header and first body statement.
    pub use_in: UseIn,
    /// Whether related units may lose existing blank lines; defaults to false.
    pub join_related: bool,
}

impl Default for Grouping {
    fn default() -> Self {
        return Self {
            bindings: Bindings::Consecutive,
            expressions: Expressions::Related,
            same_receiver: true,
            shared_inputs: true,
            same_object: true,
            same_callee: true,
            consecutive_mutations: true,
            self_fields: SelfFields::Root,
            max_before_control: 1,
            overflow: Overflow::WholeGroup,
            use_in: UseIn::HeaderOrFirstBodyStatement,
            join_related: false,
        };
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
/// Rules for standalone blocks and adjacent exiting guards.
pub struct ControlFlow {
    /// Whether to separate after standalone blocks; defaults to separate.
    pub after_block: Separation,
    /// Whether adjacent short guards may stay compact; defaults to allow.
    pub guard_chain: GuardChain,
    /// Keep a block with a following operation on the same receiver; defaults to true.
    pub related_continuation: bool,
    /// Keep known mutations and empty for-loop drains in one cleanup phase; defaults to true.
    pub compact_cleanup: bool,
}

impl Default for ControlFlow {
    fn default() -> Self {
        return Self {
            after_block: Separation::Separate,
            guard_chain: GuardChain::Allow,
            related_continuation: true,
            compact_cleanup: true,
        };
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
/// Spacing of explicit exits and final block values.
pub struct Exits {
    /// Direct executable-unit threshold for a short block; defaults to 2.
    pub short_block_max_statements: usize,
    /// Tail-value spacing policy; defaults to smart.
    pub tail: Tail,
    /// Keep a state update with its immediate valueless break/continue; defaults to true.
    pub attached_loop_exit: bool,
}

impl Default for Exits {
    fn default() -> Self {
        return Self {
            short_block_max_statements: 2,
            tail: Tail::Smart,
            attached_loop_exit: true,
        };
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
/// Spacing for compiler-confirmed result inspections.
pub struct ErrorHandling {
    /// Producer/check adjacency; defaults to join.
    pub immediate_result_option_check: ImmediateCheck,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
/// Spacing between functions, major items, and compact declarations.
pub struct Items {
    /// Spacing between functions with bodies; defaults to separate.
    pub functions: Separation,
    /// Spacing around major declarations; defaults to separate.
    pub major_items: Separation,
    /// Whether compact declarations can remain adjacent; defaults to true.
    pub compact_declarations: bool,
}

impl Default for Items {
    fn default() -> Self {
        return Self {
            functions: Separation::Separate,
            major_items: Separation::Separate,
            compact_declarations: true,
        };
    }
}

impl Config {
    /// Checks schema, numeric limits, and rule overrides.
    ///
    /// # Errors
    /// Rejects unsupported schema versions, limits above 1024, and duplicate or
    /// contradictory overrides.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("statement_spacing.schema_version must be 1".into());
        }
        for (name, value) in [
            ("max_before_control", self.grouping.max_before_control),
            (
                "short_block_max_statements",
                self.exits.short_block_max_statements,
            ),
        ] {
            if value > 1024 {
                return Err(format!(
                    "statement_spacing {name} must be between 0 and 1024"
                ));
            }
        }
        for (name, rules) in [("enable", &self.enable), ("disable", &self.disable)] {
            if rules.iter().copied().collect::<BTreeSet<_>>().len() != rules.len() {
                return Err(format!(
                    "statement_spacing.{name} contains a duplicate check"
                ));
            }
        }
        for rule in &self.enable {
            if self.disable.contains(rule) {
                return Err(format!("{} is both enabled and disabled", rule.name()));
            }
        }
        return Ok(());
    }

    /// Computes the effective rule set with enable and disable overrides applied.
    pub fn enabled(&self) -> RuleMask {
        let mut mask = RuleMask::all().without(Rule::Layout);
        for rule in &self.enable {
            mask = mask.with(*rule);
        }
        for rule in &self.disable {
            mask = mask.without(*rule);
        }
        return mask;
    }

    /// Creates a configuration with only the specified rules enabled.
    pub fn only(rules: &[Rule]) -> Self {
        let default_rules = RuleMask::all().without(Rule::Layout);
        let mut enable = Vec::new();
        let mut disable = Vec::new();
        for rule in Rule::ALL {
            if rules.contains(&rule) {
                if !default_rules.has(rule) {
                    enable.push(rule);
                }
            } else if default_rules.has(rule) {
                disable.push(rule);
            }
        }
        return Self {
            enable,
            disable,
            ..Self::default()
        };
    }
}
