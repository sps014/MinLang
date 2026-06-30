pub mod expression;
pub mod function;
pub mod pattern;
pub mod program;
pub mod statement;
pub mod struct_node;
pub mod types;

pub use expression::{ExpressionNode, MatchArm, MatchArmBody};
pub use function::{FunctionNode, ParameterNode};
pub use pattern::PatternNode;
pub use program::{
    EnumDeclarationNode, EnumVariantNode, ExtendNode, GlobalVariableNode, ImportNode, ProgramNode,
};
pub use statement::StatementNode;
pub use struct_node::{StructDeclarationNode, StructFieldNode};
pub use types::Type;

use crate::syntax::token::syntax_token::SyntaxToken;

#[derive(Debug, Clone)]
pub struct AttributeNode {
    pub name: SyntaxToken,
    pub args: Vec<SyntaxToken>,
}
