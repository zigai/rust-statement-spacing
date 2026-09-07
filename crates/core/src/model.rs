//! Source units, compiler facts, and rule identifiers.

use std::collections::BTreeSet;
use std::ops::Range;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
/// Independently configurable spacing checks, serialized by their short names.
pub enum Rule {
    /// Grouping of bindings and assignments.
    Bindings,
    /// Grouping of ordinary expressions.
    Expressions,
    /// Bounded setup before a control-flow statement.
    ControlFlow,
    /// Separation after a standalone block or control-flow statement.
    AfterBlock,
    /// Separation of explicit exits and tail values.
    Exit,
    /// Adjacency of a producer and its immediate Result/Option check.
    ResultCheck,
    /// Separation of declarations and functions.
    ItemSpacing,
    /// Removal of surplus vertical whitespace.
    Layout,
}

impl Rule {
    /// All supported checks in declaration order.
    pub const ALL: [Self; 8] = [
        Self::Bindings,
        Self::Expressions,
        Self::ControlFlow,
        Self::AfterBlock,
        Self::Exit,
        Self::ResultCheck,
        Self::ItemSpacing,
        Self::Layout,
    ];

    /// Returns the short configuration key for this check.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bindings => return "bindings",
            Self::Expressions => return "expressions",
            Self::ControlFlow => return "control_flow",
            Self::AfterBlock => return "after_block",
            Self::Exit => return "exit",
            Self::ResultCheck => return "result_check",
            Self::ItemSpacing => return "item_spacing",
            Self::Layout => return "layout",
        }
    }

    /// Returns the fully qualified compiler lint identifier.
    pub fn lint_name(self) -> String {
        return format!("statement_spacing_{}", self.name());
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
/// Set of enabled checks; the default includes all supported rules.
pub struct RuleMask(u16);

impl Default for RuleMask {
    fn default() -> Self {
        return Self::all();
    }
}

impl RuleMask {
    /// Includes every supported check.
    pub const fn all() -> Self {
        return Self(0xff);
    }
    /// Disables every check.
    pub const fn none() -> Self {
        return Self(0);
    }
    /// Tests whether this check is enabled.
    pub const fn has(self, rule: Rule) -> bool {
        return self.0 & (1 << rule as u8) != 0;
    }
    /// Returns a copy with the given check enabled.
    pub const fn with(self, rule: Rule) -> Self {
        return Self(self.0 | (1 << rule as u8));
    }
    /// Returns a copy with the given check disabled.
    pub const fn without(self, rule: Rule) -> Self {
        return Self(self.0 & !(1 << rule as u8));
    }
    /// Keeps only checks enabled in both masks.
    pub const fn intersect(self, other: Self) -> Self {
        return Self(self.0 & other.0);
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
/// Half-open byte offsets into normalized UTF-8 source.
pub struct ByteRange {
    /// Inclusive byte offset.
    pub start: usize,
    /// Exclusive byte offset.
    pub end: usize,
}

impl ByteRange {
    /// Constructs offsets without validating their order or source bounds.
    pub const fn new(start: usize, end: usize) -> Self {
        return Self { start, end };
    }
    /// Tests whether both bounds of the other range lie within this range.
    pub const fn contains(self, other: Self) -> bool {
        return self.start <= other.start && other.end <= self.end;
    }
    /// Tests whether the ranges have intersecting interiors.
    pub const fn overlaps(self, other: Self) -> bool {
        return self.start < other.end && other.start < self.end;
    }
    /// Returns the equivalent standard half-open range.
    pub fn as_range(self) -> Range<usize> {
        return self.start..self.end;
    }
}

/// A compiler-resolved local and syntactic place projections. This is not alias analysis.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Place {
    /// Compiler identity of the root binding, not its written name.
    pub local: String,
    /// Ordered field or place projections from the root binding.
    pub projections: Vec<String>,
    /// Whether the root binding is the method receiver.
    pub is_self: bool,
}

impl Place {
    /// Constructs an unprojected local with the supplied compiler identity.
    pub fn local(id: impl Into<String>) -> Self {
        return Self {
            local: id.into(),
            projections: Vec::new(),
            is_self: false,
        };
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
/// Compiler facts about one source unit; unknown facts do not imply independence.
pub struct Facts {
    /// Whether the adapter has sufficient facts to compare this unit.
    pub known: bool,
    /// Local bindings introduced by this unit.
    pub definitions: BTreeSet<Place>,
    /// Places read by this unit, excluding deferred bodies.
    pub reads: BTreeSet<Place>,
    /// Places assigned or exposed through a mutable reference/raw address by this unit.
    pub writes: BTreeSet<Place>,
    /// Places used as method receivers.
    pub receivers: BTreeSet<Place>,
    /// Method receivers compiler-adjusted to a mutable reference, not proven assignment writes.
    pub mutating_receivers: BTreeSet<Place>,
    /// Compiler identities of direct source callees, excluding nested and deferred calls.
    pub direct_callees: BTreeSet<String>,
    /// Places read in a control-flow header.
    pub header_reads: BTreeSet<Place>,
    /// Places read by the first executable body statement.
    pub first_body_reads: BTreeSet<Place>,
    /// Places read throughout the unit, excluding deferred bodies.
    pub whole_body_reads: BTreeSet<Place>,
    /// Set only for a narrow compiler-confirmed standard Result/Option inspection.
    pub check_of: Option<Place>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
/// Declaration categories with distinct spacing policies.
pub enum ItemKind {
    /// Function or method with a body.
    Function,
    /// Major declaration such as a struct or implementation.
    Major,
    /// Compact declaration such as an import or type alias.
    Compact,
    /// Syntax whose interior is not analyzed.
    Opaque,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
/// Syntactic role of a direct sibling in a statement or item list.
pub enum UnitKind {
    /// Local binding declaration.
    Let,
    /// Assignment to an existing place.
    Assignment,
    /// Ordinary expression statement or tail value.
    Expression,
    /// Conditional, loop, or match expression.
    Control,
    /// Standalone scope block.
    Block,
    /// Standalone unsafe scope, commonly wrapping an individual FFI operation.
    UnsafeBlock,
    /// Explicit control-flow exit.
    Exit,
    /// Item declaration with its spacing category.
    Item(ItemKind),
    /// Syntax whose interior is not analyzed.
    Opaque,
}

impl UnitKind {
    /// Whether this unit introduces or assigns a binding.
    pub fn is_binding(self) -> bool {
        return matches!(self, Self::Let | Self::Assignment);
    }
    /// Whether this unit is an item declaration.
    pub fn is_item(self) -> bool {
        return matches!(self, Self::Item(_));
    }
    /// Whether this is a standalone control-flow or scope block.
    pub fn ends_block(self) -> bool {
        return matches!(self, Self::Control | Self::Block | Self::UnsafeBlock);
    }
    /// Whether ordinary binding/expression grouping applies.
    pub fn ordinary(self) -> bool {
        return matches!(self, Self::Let | Self::Assignment | Self::Expression);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// One direct source sibling and its compiler annotations.
#[expect(
    clippy::struct_excessive_bools,
    reason = "The public serialized source model exposes syntax predicates and compiler annotations as separate boolean fields."
)]
pub struct Unit {
    /// Syntax node including its attached attributes and documentation.
    pub range: ByteRange,
    /// Code span used for compiler matching, excluding attached attributes and docs.
    pub code_range: ByteRange,
    /// Syntactic category used to select spacing policy.
    pub kind: UnitKind,
    /// Whether this expression is the enclosing block value.
    pub is_tail: bool,
    /// Whether this control-flow unit is a short exiting guard.
    pub is_guard: bool,
    /// Whether this is a for loop with no executable body statements.
    pub is_empty_loop: bool,
    /// Whether this is a break or continue without a value.
    pub is_loop_exit: bool,
    /// Whether this is a return without a value.
    pub is_bare_return: bool,
    /// The compiler has mapped this original syntax node to active source.
    pub active: bool,
    /// Whether this unit must preserve its existing spacing.
    pub protected: bool,
    /// Resolved reads, writes, and relationships for this unit.
    pub facts: Facts,
    /// Checks enabled at this compiler node.
    pub enabled: RuleMask,
    /// Opaque key returned to the compiler adapter for correctly scoped diagnostics.
    pub anchor: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Whitespace boundary between two direct siblings.
pub struct Gap {
    /// Editable trivia only; this range never includes an attached comment.
    pub range: ByteRange,
    /// Number of empty lines between the siblings.
    pub blank_lines: usize,
    /// Whether the boundary contains a line break.
    pub vertical: bool,
    /// Whether this gap must preserve its existing spacing.
    pub protected: bool,
    /// Whether adjacency can be established without crossing attached comments.
    pub joinable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Ordered direct siblings in one statement or item container.
pub struct UnitList {
    /// Direct siblings in source order.
    pub units: Vec<Unit>,
    /// `gaps[i]` separates `units[i]` from `units[i + 1]`.
    pub gaps: Vec<Gap>,
    /// Direct executable units, including a tail expression but excluding items.
    pub executable_count: usize,
    /// Whether this container holds declarations rather than statements.
    pub item_list: bool,
}

#[derive(Clone, Debug, Default)]
/// All source containers and eligible block-edge whitespace in a file.
pub struct SourceModel {
    /// Statement and item containers, including nested containers.
    pub lists: Vec<UnitList>,
    /// Edges where excess blank lines can be removed without touching comments.
    pub layout_edges: Vec<(ByteRange, usize)>,
}
