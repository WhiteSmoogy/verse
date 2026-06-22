use std::collections::{HashMap, HashSet};

use crate::ast::Program;
use crate::checker::Type;
use crate::error::Diagnostic;
use crate::token::Span;

#[derive(Debug, Clone)]
pub struct SemanticProgram {
    pub program: Program,
    pub value_type: Type,
    pub warnings: Vec<Diagnostic>,
    pub facts: SemanticFacts,
}

pub type TypedProgram = SemanticProgram;

#[derive(Debug, Clone, Default)]
pub struct SemanticFacts {
    binding_types: HashMap<Span, Type>,
    expression_types: HashMap<Span, Type>,
    static_type_function_calls: HashSet<Span>,
}

impl SemanticFacts {
    pub fn record_binding_type(&mut self, span: Span, value_type: Type) {
        self.binding_types.insert(span, value_type);
    }

    pub fn record_expression_type(&mut self, span: Span, value_type: Type) {
        self.expression_types.insert(span, value_type);
    }

    pub fn record_static_type_function_call(&mut self, span: Span) {
        self.static_type_function_calls.insert(span);
    }

    pub fn binding_type(&self, span: Span) -> Option<&Type> {
        self.binding_types.get(&span)
    }

    pub fn expression_type(&self, span: Span) -> Option<&Type> {
        self.expression_types.get(&span)
    }

    pub fn is_static_type_function_call(&self, span: Span) -> bool {
        self.static_type_function_calls.contains(&span)
    }
}
