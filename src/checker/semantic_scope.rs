use std::collections::{HashMap, HashSet};

use super::*;

#[derive(Clone)]
pub(super) struct Symbol {
    pub(super) value_type: Type,
    pub(super) mutable: bool,
}

impl Symbol {
    pub(super) fn immutable(value_type: Type) -> Self {
        Self {
            value_type,
            mutable: false,
        }
    }
}

impl Checker {
    pub(super) fn define(
        &mut self,
        name: &str,
        value_type: Type,
        mutable: bool,
        span: Span,
    ) -> Result<(), VerseError> {
        self.ensure_not_shadowing_class_member(name, span)?;
        let current = self
            .scopes
            .last_mut()
            .expect("checker should always have a scope");
        if current.contains_key(name) {
            return Err(VerseError::check_at(
                format!("duplicate definition `{name}`"),
                span,
            ));
        }
        current.insert(
            name.to_string(),
            Symbol {
                value_type,
                mutable,
            },
        );
        Ok(())
    }

    pub(super) fn define_predeclared_aggregate_value(
        &mut self,
        name: &str,
        qualified: &str,
        value_type: Type,
        span: Span,
    ) -> Result<(), VerseError> {
        self.ensure_not_shadowing_class_member(name, span)?;
        let current = self
            .scopes
            .last_mut()
            .expect("checker should always have a scope");
        if current.contains_key(name) {
            return Err(VerseError::check_at(
                format!("duplicate definition `{name}`"),
                span,
            ));
        }
        current.insert(name.to_string(), Symbol::immutable(value_type));
        self.predeclared_aggregate_values
            .insert(qualified.to_string());
        Ok(())
    }

    pub(super) fn define_aggregate_value(
        &mut self,
        name: &str,
        qualified: &str,
        value_type: Type,
        span: Span,
    ) -> Result<(), VerseError> {
        self.ensure_not_shadowing_class_member(name, span)?;
        if self.predeclared_aggregate_values.remove(qualified) {
            let current = self
                .scopes
                .last_mut()
                .expect("checker should always have a scope");
            match current.get_mut(name) {
                Some(symbol) if !symbol.mutable && symbol.value_type == value_type => {
                    symbol.value_type = value_type;
                    return Ok(());
                }
                _ => {
                    return Err(VerseError::check_at(
                        format!("duplicate definition `{name}`"),
                        span,
                    ));
                }
            }
        }

        self.define(name, value_type, false, span)
    }

    pub(super) fn push_class_member_shadow_names(
        &mut self,
        class_name: &str,
        fields: &[StructFieldInfo],
    ) {
        let mut names = fields
            .iter()
            .map(|field| field.name.clone())
            .collect::<HashSet<_>>();
        if let Some(info) = self.struct_types.get(class_name) {
            names.extend(info.methods.iter().map(|method| method.name.clone()));
        }
        self.class_member_shadow_names.push(names);
    }

    pub(super) fn pop_class_member_shadow_names(&mut self) {
        self.class_member_shadow_names
            .pop()
            .expect("class member shadow stack should not underflow");
    }

    pub(super) fn ensure_not_shadowing_class_member(
        &self,
        name: &str,
        span: Span,
    ) -> Result<(), VerseError> {
        if self
            .class_member_shadow_names
            .last()
            .is_some_and(|members| members.contains(name))
        {
            Err(VerseError::check_at(
                format!("definition `{name}` cannot shadow class member `{name}`"),
                span,
            ))
        } else {
            Ok(())
        }
    }

    pub(super) fn lookup(&self, name: &str) -> Option<Symbol> {
        for (scope, imports) in self.scopes.iter().zip(&self.scope_imports).rev() {
            if let Some(symbol) = scope.get(name) {
                return Some(symbol.clone());
            }
            for module_name in imports.iter().rev() {
                if let Some(value_type) = self
                    .module_types
                    .get(module_name)
                    .and_then(|module| module.members.get(name))
                {
                    return Some(Symbol {
                        value_type: value_type.clone(),
                        mutable: false,
                    });
                }
            }
        }
        None
    }

    pub(super) fn lookup_accessible(
        &self,
        name: &str,
        span: Span,
    ) -> Result<Option<Symbol>, VerseError> {
        for (scope, imports) in self.scopes.iter().zip(&self.scope_imports).rev() {
            if let Some(symbol) = scope.get(name) {
                return Ok(Some(symbol.clone()));
            }
            for module_name in imports.iter().rev() {
                let Some(module) = self.module_types.get(module_name) else {
                    continue;
                };
                let Some(value_type) = module.members.get(name) else {
                    continue;
                };
                let access = module
                    .member_access
                    .get(name)
                    .copied()
                    .unwrap_or(AccessLevel::Internal);
                self.ensure_module_member_accessible(module_name, access, name, span)?;
                return Ok(Some(Symbol {
                    value_type: value_type.clone(),
                    mutable: false,
                }));
            }
        }
        Ok(None)
    }

    pub(super) fn is_current_predeclared_function(&self, name: &str) -> bool {
        matches!(
            self.scopes
                .last()
                .and_then(|scope| scope.get(name))
                .map(|symbol| &symbol.value_type),
            Some(Type::Function { .. } | Type::Overload(_))
        )
    }

    pub(super) fn define_or_overload_function(
        &mut self,
        name: &str,
        function_type: Type,
        span: Span,
    ) -> Result<Type, VerseError> {
        let current = self
            .scopes
            .last_mut()
            .expect("checker should always have a scope");
        let Some(existing) = current.get_mut(name) else {
            current.insert(name.to_string(), Symbol::immutable(function_type.clone()));
            return Ok(function_type);
        };

        if existing.mutable {
            return Err(VerseError::check_at(
                format!("duplicate definition `{name}`"),
                span,
            ));
        }

        match &mut existing.value_type {
            Type::Function { .. } => {
                if function_signatures_conflict(
                    &existing.value_type,
                    &function_type,
                    &self.struct_types,
                ) {
                    return Err(VerseError::check_at(
                        format!("duplicate overload `{name}`"),
                        span,
                    ));
                }
                let previous = existing.value_type.clone();
                existing.value_type = Type::Overload(vec![previous, function_type]);
                Ok(existing.value_type.clone())
            }
            Type::Overload(overloads) => {
                if overloads.iter().any(|overload| {
                    function_signatures_conflict(overload, &function_type, &self.struct_types)
                }) {
                    return Err(VerseError::check_at(
                        format!("duplicate overload `{name}`"),
                        span,
                    ));
                }
                overloads.push(function_type);
                Ok(existing.value_type.clone())
            }
            _ => Err(VerseError::check_at(
                format!("duplicate definition `{name}`"),
                span,
            )),
        }
    }

    pub(super) fn update_current_function_binding(
        &mut self,
        name: &str,
        function_type: Type,
        span: Span,
    ) -> Result<Type, VerseError> {
        let current = self
            .scopes
            .last_mut()
            .expect("checker should always have a scope");
        let Some(existing) = current.get_mut(name) else {
            current.insert(name.to_string(), Symbol::immutable(function_type.clone()));
            return Ok(function_type);
        };

        match &mut existing.value_type {
            Type::Function { .. } => {
                existing.value_type = function_type.clone();
                Ok(function_type)
            }
            Type::Overload(overloads) => {
                if let Some(index) = overloads.iter().position(|overload| {
                    function_signatures_match_exactly(overload, &function_type)
                }) {
                    overloads[index] = function_type;
                    return Ok(existing.value_type.clone());
                }
                Err(VerseError::check_at(
                    format!("unknown predeclared overload `{name}`"),
                    span,
                ))
            }
            _ => Err(VerseError::check_at(
                format!("duplicate definition `{name}`"),
                span,
            )),
        }
    }

    pub(super) fn validate_function_overloads_in_current_scope(&self) -> Result<(), VerseError> {
        let current = self
            .scopes
            .last()
            .expect("checker should always have a scope");
        for (name, symbol) in current {
            let Type::Overload(overloads) = &symbol.value_type else {
                continue;
            };
            for (index, left) in overloads.iter().enumerate() {
                if overloads
                    .iter()
                    .skip(index + 1)
                    .any(|right| function_signatures_conflict(left, right, &self.struct_types))
                {
                    return Err(VerseError::check(format!("duplicate overload `{name}`")));
                }
            }
        }
        Ok(())
    }

    pub(super) fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.scope_imports.push(Vec::new());
    }

    pub(super) fn pop_scope(&mut self) {
        self.scopes.pop();
        self.scope_imports.pop();
    }
}
