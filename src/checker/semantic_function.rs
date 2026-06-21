use std::collections::HashMap;

use crate::ast::{CallArg, Expr, Param, ParamPattern, TypeParam};
use crate::error::VerseError;

use super::*;

pub(super) fn method_binding_types(methods: &[ClassMethodInfo]) -> Vec<(String, Type)> {
    let mut grouped: Vec<(String, Vec<Type>)> = Vec::new();
    for method in methods {
        if let Some((_, overloads)) = grouped.iter_mut().find(|(name, _)| name == &method.name) {
            overloads.push(method.value_type.clone());
        } else {
            grouped.push((method.name.clone(), vec![method.value_type.clone()]));
        }
    }

    grouped
        .into_iter()
        .map(|(name, overloads)| {
            let value_type = match overloads.as_slice() {
                [single] => single.clone(),
                _ => Type::Overload(overloads),
            };
            (name, value_type)
        })
        .collect()
}

pub(super) fn method_group_type<'a>(
    methods: impl IntoIterator<Item = &'a ClassMethodInfo>,
) -> Option<Type> {
    let overloads = methods
        .into_iter()
        .map(|method| method.value_type.clone())
        .collect::<Vec<_>>();
    match overloads.as_slice() {
        [] => None,
        [single] => Some(single.clone()),
        _ => Some(Type::Overload(overloads)),
    }
}

pub(super) fn qualifier_matches(stored: &str, requested: &str) -> bool {
    stored == requested
        || stored.rsplit('.').next() == Some(requested)
        || requested.rsplit('.').next() == Some(stored)
}

pub(super) fn method_has_qualifier(method: &ClassMethodInfo, qualifier: &str) -> bool {
    method
        .qualifier
        .as_deref()
        .is_some_and(|stored| qualifier_matches(stored, qualifier))
}

pub(super) fn extension_method_has_qualifier(
    method: &ExtensionMethodInfo,
    qualifier: &str,
) -> bool {
    method
        .module_name
        .as_deref()
        .is_some_and(|stored| qualifier_matches(stored, qualifier))
}

pub(super) fn method_qualifiers_conflict(left: &ClassMethodInfo, right: &ClassMethodInfo) -> bool {
    match (left.qualifier.as_deref(), right.qualifier.as_deref()) {
        (Some(left), Some(right)) => qualifier_matches(left, right),
        (None, None) => true,
        _ => false,
    }
}

pub(super) fn method_signatures_conflict(
    left: &ClassMethodInfo,
    right: &ClassMethodInfo,
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    left.name == right.name
        && function_signatures_conflict(&left.value_type, &right.value_type, struct_types)
}

pub(super) fn inherited_method_override_index(
    inherited_methods: &[ClassMethodInfo],
    method: &ClassMethodInfo,
    struct_types: &HashMap<String, StructInfo>,
) -> Result<Option<usize>, VerseError> {
    let candidates = inherited_methods
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            method_signatures_conflict(candidate, method, struct_types).then_some(index)
        })
        .collect::<Vec<_>>();

    if method.qualifier.is_some() {
        return Ok(candidates
            .into_iter()
            .find(|index| method_qualifiers_conflict(&inherited_methods[*index], method)));
    }

    if let Some(index) = candidates
        .iter()
        .copied()
        .find(|index| method_qualifiers_conflict(&inherited_methods[*index], method))
    {
        return Ok(Some(index));
    }

    match candidates.as_slice() {
        [] => Ok(None),
        [index] => Ok(Some(*index)),
        _ => Err(VerseError::check_at(
            format!(
                "method `{}` override is ambiguous; use a qualified method name",
                method.name
            ),
            method.span,
        )),
    }
}

pub(super) fn inherited_method_duplicate_index(
    inherited_methods: &[ClassMethodInfo],
    method: &ClassMethodInfo,
    struct_types: &HashMap<String, StructInfo>,
) -> Option<usize> {
    inherited_methods.iter().position(|candidate| {
        method_signatures_conflict(candidate, method, struct_types)
            && (method.qualifier.is_none() || method_qualifiers_conflict(candidate, method))
    })
}

pub(super) fn push_distinct_local_method_info(
    infos: &mut Vec<ClassMethodInfo>,
    info: ClassMethodInfo,
    aggregate_kind: &str,
    struct_types: &HashMap<String, StructInfo>,
) -> Result<(), VerseError> {
    if infos.iter().any(|existing| {
        existing.name == info.name
            && method_qualifiers_conflict(existing, &info)
            && function_signatures_conflict(&existing.value_type, &info.value_type, struct_types)
    }) {
        return Err(VerseError::check_at(
            format!("duplicate {aggregate_kind} method overload `{}`", info.name),
            info.span,
        ));
    }
    infos.push(info);
    Ok(())
}

pub(super) fn function_signatures_match_exactly(left: &Type, right: &Type) -> bool {
    let (
        Type::Function {
            arity: left_arity,
            arity_range: left_arity_range,
            param_types: left_param_types,
            param_specs: left_param_specs,
            ..
        },
        Type::Function {
            arity: right_arity,
            arity_range: right_arity_range,
            param_types: right_param_types,
            param_specs: right_param_specs,
            ..
        },
    ) = (left, right)
    else {
        return false;
    };

    left_arity == right_arity
        && left_arity_range == right_arity_range
        && left_param_types == right_param_types
        && exact_param_specs_key(left_param_specs.as_deref())
            == exact_param_specs_key(right_param_specs.as_deref())
}

pub(super) fn exact_param_specs_key(
    specs: Option<&[ParamSpec]>,
) -> Option<Vec<(bool, String, Type)>> {
    let specs = specs?;
    let mut key = specs
        .iter()
        .map(|spec| {
            (
                spec.named,
                if spec.named {
                    spec.name.clone()
                } else {
                    String::new()
                },
                spec.value_type.clone(),
            )
        })
        .collect::<Vec<_>>();
    if key.iter().all(|(named, _, _)| *named) {
        key.sort_by(|left, right| left.1.cmp(&right.1));
    }
    Some(key)
}

pub(super) fn function_signatures_conflict(
    left: &Type,
    right: &Type,
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    let (
        Type::Function {
            arity: left_arity,
            arity_range: left_arity_range,
            param_types: left_param_types,
            param_specs: left_param_specs,
            ..
        },
        Type::Function {
            arity: right_arity,
            arity_range: right_arity_range,
            param_types: right_param_types,
            param_specs: right_param_specs,
            ..
        },
    ) = (left, right)
    else {
        return false;
    };

    if left_arity_range != right_arity_range {
        return false;
    }

    if let (Some(left_specs), Some(right_specs)) =
        (left_param_specs.as_deref(), right_param_specs.as_deref())
    {
        return param_specs_overlap(left_specs, right_specs, struct_types);
    }

    left_arity == right_arity
        && param_type_lists_overlap(
            left_param_types.as_deref(),
            right_param_types.as_deref(),
            struct_types,
        )
}

pub(super) fn param_specs_overlap(
    left: &[ParamSpec],
    right: &[ParamSpec],
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    if param_specs_overlap_direct(left, right, struct_types) {
        return true;
    }

    let left_variants = expanded_single_tuple_param_spec_variants(left);
    let right_variants = expanded_single_tuple_param_spec_variants(right);

    for left_variant in &left_variants {
        if param_specs_overlap_direct(left_variant, right, struct_types) {
            return true;
        }
        for right_variant in &right_variants {
            if param_specs_overlap_direct(left_variant, right_variant, struct_types) {
                return true;
            }
        }
    }

    right_variants
        .iter()
        .any(|right_variant| param_specs_overlap_direct(left, right_variant, struct_types))
}

pub(super) fn expanded_single_tuple_param_spec_variants(
    specs: &[ParamSpec],
) -> Vec<Vec<ParamSpec>> {
    let [single] = specs else {
        return Vec::new();
    };
    let Some(items) = &single.tuple_items else {
        return Vec::new();
    };

    let mut variants = vec![items.clone()];
    variants.extend(expanded_single_tuple_param_spec_variants(items));
    variants
}

pub(super) fn param_specs_overlap_direct(
    left: &[ParamSpec],
    right: &[ParamSpec],
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    let left_positional = left
        .iter()
        .filter(|spec| !spec.named)
        .map(|spec| &spec.value_type)
        .collect::<Vec<_>>();
    let right_positional = right
        .iter()
        .filter(|spec| !spec.named)
        .map(|spec| &spec.value_type)
        .collect::<Vec<_>>();

    if !param_type_slices_overlap(&left_positional, &right_positional, struct_types) {
        return false;
    }

    let left_named = left.iter().filter(|spec| spec.named).collect::<Vec<_>>();
    let right_named = right.iter().filter(|spec| spec.named).collect::<Vec<_>>();

    required_named_params_are_accepted_by(&left_named, &right_named, struct_types)
        && required_named_params_are_accepted_by(&right_named, &left_named, struct_types)
}

pub(super) fn required_named_params_are_accepted_by(
    required_source: &[&ParamSpec],
    target: &[&ParamSpec],
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    required_source
        .iter()
        .filter(|spec| !spec.has_default)
        .all(|required| {
            target
                .iter()
                .find(|candidate| candidate.name == required.name)
                .is_some_and(|candidate| {
                    types_not_distinct(&required.value_type, &candidate.value_type, struct_types)
                })
        })
}

pub(super) fn param_type_lists_overlap(
    left: Option<&[Type]>,
    right: Option<&[Type]>,
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => {
            let left_refs = left.iter().collect::<Vec<_>>();
            let right_refs = right.iter().collect::<Vec<_>>();
            param_type_slices_overlap(&left_refs, &right_refs, struct_types)
        }
        _ => true,
    }
}

pub(super) fn param_type_slices_overlap(
    left: &[&Type],
    right: &[&Type],
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    if left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| types_not_distinct(left, right, struct_types))
    {
        return true;
    }

    if let [single] = left
        && single_param_overlaps_sequence(single, right, struct_types)
    {
        return true;
    }

    if let [single] = right {
        return single_param_overlaps_sequence(single, left, struct_types);
    }

    false
}

pub(super) fn single_param_overlaps_sequence(
    single: &Type,
    sequence: &[&Type],
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    match single {
        Type::Tuple(items) if items.len() == sequence.len() => items
            .iter()
            .zip(sequence)
            .all(|(item, sequence_type)| types_not_distinct(item, sequence_type, struct_types)),
        _ => false,
    }
}

pub(super) fn types_not_distinct(
    left: &Type,
    right: &Type,
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    if left == right {
        return true;
    }

    match (left, right) {
        (Type::Any | Type::Unknown, _) | (_, Type::Any | Type::Unknown) => true,
        (Type::None, _) | (_, Type::None) => true,
        (Type::Option(_), Type::Bool) | (Type::Bool, Type::Option(_)) => true,
        (Type::Array(_), Type::Map(_, _) | Type::WeakMap(_, _))
        | (Type::Map(_, _) | Type::WeakMap(_, _), Type::Array(_)) => true,
        (Type::Function { .. }, Type::Array(_) | Type::Map(_, _) | Type::WeakMap(_, _))
        | (Type::Array(_) | Type::Map(_, _) | Type::WeakMap(_, _), Type::Function { .. }) => true,
        (Type::Function { .. }, Type::Function { .. }) => true,
        (Type::Interface(_), Type::Class(_)) | (Type::Class(_), Type::Interface(_)) => true,
        (Type::Class(left), Type::Class(right)) => {
            class_types_not_distinct(left, right, struct_types)
        }
        (Type::Tuple(items), Type::Array(item)) | (Type::Array(item), Type::Tuple(items)) => items
            .iter()
            .any(|tuple_item| types_not_distinct(tuple_item, item, struct_types)),
        (Type::Tuple(items), Type::Map(key, value))
        | (Type::Map(key, value), Type::Tuple(items)) => {
            matches!(key.as_ref(), Type::Int)
                && items
                    .iter()
                    .any(|tuple_item| types_not_distinct(tuple_item, value, struct_types))
        }
        (Type::Tuple(items), Type::Option(item)) | (Type::Option(item), Type::Tuple(items)) => {
            matches!(items.as_slice(), [single] if types_not_distinct(single, item, struct_types))
        }
        _ => false,
    }
}

pub(super) fn class_types_not_distinct(
    left: &str,
    right: &str,
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    left == right
        || class_is_subtype_of(left, right, struct_types)
        || class_is_subtype_of(right, left, struct_types)
}

pub(super) fn class_is_subtype_of(
    child: &str,
    parent: &str,
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    let mut current = Some(child);
    while let Some(name) = current {
        if name == parent {
            return true;
        }
        current = struct_types.get(name).and_then(|info| info.base.as_deref());
    }
    false
}

pub(super) fn positional_call_args(args: &[Expr]) -> Vec<CallArg> {
    args.iter().cloned().map(CallArg::Positional).collect()
}

pub(super) fn infer_function_type_params(
    param_types: Option<&[Type]>,
    arg_types: &[Type],
) -> Option<HashMap<String, Type>> {
    let param_types = param_types?;
    if param_types.len() != arg_types.len() {
        return Some(HashMap::new());
    }
    let mut inferred = HashMap::new();
    for (param_type, arg_type) in param_types.iter().zip(arg_types) {
        infer_type_params_from_type(param_type, arg_type, &mut inferred)?;
    }
    Some(inferred)
}

pub(super) fn infer_type_params_from_type(
    pattern: &Type,
    actual: &Type,
    inferred: &mut HashMap<String, Type>,
) -> Option<()> {
    match (pattern, actual) {
        (Type::Param(name, _), actual) => {
            if inferred.contains_key(name) {
                Some(())
            } else {
                inferred.insert(name.clone(), actual.clone());
                Some(())
            }
        }
        (Type::Array(pattern), Type::Array(actual)) => {
            infer_type_params_from_type(pattern, actual, inferred)
        }
        (Type::Map(pattern_key, pattern_value), Type::Map(actual_key, actual_value))
        | (Type::WeakMap(pattern_key, pattern_value), Type::WeakMap(actual_key, actual_value)) => {
            infer_type_params_from_type(pattern_key, actual_key, inferred)?;
            infer_type_params_from_type(pattern_value, actual_value, inferred)
        }
        (Type::Tuple(pattern_items), Type::Tuple(actual_items))
            if pattern_items.len() == actual_items.len() =>
        {
            for (pattern, actual) in pattern_items.iter().zip(actual_items) {
                infer_type_params_from_type(pattern, actual, inferred)?;
            }
            Some(())
        }
        (Type::Option(pattern), Type::Option(actual))
        | (Type::Task(pattern), Type::Task(actual))
        | (Type::CastableSubtype(pattern), Type::CastableSubtype(actual))
        | (Type::ConcreteSubtype(pattern), Type::ConcreteSubtype(actual))
        | (Type::ClassifiableSubset(pattern), Type::ClassifiableSubset(actual))
        | (Type::Modifier(pattern), Type::Modifier(actual))
        | (Type::ModifierStack(pattern), Type::ModifierStack(actual))
        | (Type::Signalable(pattern), Type::Signalable(actual)) => {
            infer_type_params_from_type(pattern, actual, inferred)
        }
        (
            Type::Result(pattern_success, pattern_error),
            Type::Result(actual_success, actual_error),
        ) => {
            infer_type_params_from_type(pattern_success, actual_success, inferred)?;
            infer_type_params_from_type(pattern_error, actual_error, inferred)
        }
        (Type::Event(pattern), Type::Event(actual))
        | (Type::Generator(pattern), Type::Generator(actual))
        | (Type::Awaitable(pattern), Type::Awaitable(actual))
        | (Type::Subscribable(pattern), Type::Subscribable(actual))
        | (Type::Listenable(pattern), Type::Listenable(actual)) => match (pattern, actual) {
            (Some(pattern), Some(actual)) => infer_type_params_from_type(pattern, actual, inferred),
            _ => Some(()),
        },
        (
            Type::Function {
                param_types: pattern_params,
                return_type: pattern_return,
                ..
            },
            Type::Function {
                param_types: actual_params,
                return_type: actual_return,
                ..
            },
        ) => {
            if let (Some(pattern_params), Some(actual_params)) = (pattern_params, actual_params) {
                if pattern_params.len() != actual_params.len() {
                    return None;
                }
                for (pattern, actual) in pattern_params.iter().zip(actual_params) {
                    infer_type_params_from_type(pattern, actual, inferred)?;
                }
            }
            infer_type_params_from_type(pattern_return, actual_return, inferred)
        }
        _ => Some(()),
    }
}

pub(super) fn substitute_type_params(value_type: &Type, inferred: &HashMap<String, Type>) -> Type {
    match value_type {
        Type::Param(name, _) => inferred
            .get(name)
            .cloned()
            .unwrap_or_else(|| value_type.clone()),
        Type::Array(item) => Type::Array(Box::new(substitute_type_params(item, inferred))),
        Type::Map(key, value) => Type::Map(
            Box::new(substitute_type_params(key, inferred)),
            Box::new(substitute_type_params(value, inferred)),
        ),
        Type::WeakMap(key, value) => Type::WeakMap(
            Box::new(substitute_type_params(key, inferred)),
            Box::new(substitute_type_params(value, inferred)),
        ),
        Type::Tuple(items) => Type::Tuple(
            items
                .iter()
                .map(|item| substitute_type_params(item, inferred))
                .collect(),
        ),
        Type::Option(item) => Type::Option(Box::new(substitute_type_params(item, inferred))),
        Type::Result(success, error) => Type::Result(
            Box::new(substitute_type_params(success, inferred)),
            Box::new(substitute_type_params(error, inferred)),
        ),
        Type::Event(payload) => Type::Event(
            payload
                .as_deref()
                .map(|payload| Box::new(substitute_type_params(payload, inferred))),
        ),
        Type::Task(payload) => Type::Task(Box::new(substitute_type_params(payload, inferred))),
        Type::Generator(payload) => Type::Generator(
            payload
                .as_deref()
                .map(|payload| Box::new(substitute_type_params(payload, inferred))),
        ),
        Type::CastableSubtype(item) => {
            Type::CastableSubtype(Box::new(substitute_type_params(item, inferred)))
        }
        Type::ConcreteSubtype(item) => {
            Type::ConcreteSubtype(Box::new(substitute_type_params(item, inferred)))
        }
        Type::ClassifiableSubset(item) => {
            Type::ClassifiableSubset(Box::new(substitute_type_params(item, inferred)))
        }
        Type::Modifier(item) => Type::Modifier(Box::new(substitute_type_params(item, inferred))),
        Type::ModifierStack(item) => {
            Type::ModifierStack(Box::new(substitute_type_params(item, inferred)))
        }
        Type::Awaitable(payload) => Type::Awaitable(
            payload
                .as_deref()
                .map(|payload| Box::new(substitute_type_params(payload, inferred))),
        ),
        Type::Signalable(payload) => {
            Type::Signalable(Box::new(substitute_type_params(payload, inferred)))
        }
        Type::Subscribable(payload) => Type::Subscribable(
            payload
                .as_deref()
                .map(|payload| Box::new(substitute_type_params(payload, inferred))),
        ),
        Type::Listenable(payload) => Type::Listenable(
            payload
                .as_deref()
                .map(|payload| Box::new(substitute_type_params(payload, inferred))),
        ),
        Type::Function {
            arity,
            arity_range,
            effects,
            param_types,
            param_specs,
            return_type,
        } => Type::Function {
            arity: *arity,
            arity_range: *arity_range,
            effects: effects.clone(),
            param_types: param_types.as_ref().map(|params| {
                params
                    .iter()
                    .map(|param| substitute_type_params(param, inferred))
                    .collect()
            }),
            param_specs: param_specs.as_ref().map(|specs| {
                specs
                    .iter()
                    .map(|spec| substitute_param_spec(spec, inferred))
                    .collect()
            }),
            return_type: Box::new(substitute_type_params(return_type, inferred)),
        },
        Type::Overload(overloads) => Type::Overload(
            overloads
                .iter()
                .map(|overload| substitute_type_params(overload, inferred))
                .collect(),
        ),
        _ => value_type.clone(),
    }
}

pub(super) fn type_contains_type_param(value_type: &Type) -> bool {
    match value_type {
        Type::Param(_, _) => true,
        Type::Array(item)
        | Type::Option(item)
        | Type::Task(item)
        | Type::CastableSubtype(item)
        | Type::ConcreteSubtype(item)
        | Type::ClassifiableSubset(item)
        | Type::Modifier(item)
        | Type::ModifierStack(item)
        | Type::Signalable(item) => type_contains_type_param(item),
        Type::Map(key, value) | Type::WeakMap(key, value) | Type::Result(key, value) => {
            type_contains_type_param(key) || type_contains_type_param(value)
        }
        Type::Tuple(items) | Type::Overload(items) => items.iter().any(type_contains_type_param),
        Type::Event(payload)
        | Type::Generator(payload)
        | Type::Awaitable(payload)
        | Type::Subscribable(payload)
        | Type::Listenable(payload) => payload.as_deref().is_some_and(type_contains_type_param),
        Type::Function {
            param_types,
            param_specs,
            return_type,
            ..
        } => {
            param_types
                .as_ref()
                .is_some_and(|params| params.iter().any(type_contains_type_param))
                || param_specs
                    .as_ref()
                    .is_some_and(|specs| specs.iter().any(param_spec_contains_type_param))
                || type_contains_type_param(return_type)
        }
        Type::Int
        | Type::IntRange(_)
        | Type::Float
        | Type::Rational
        | Type::Number
        | Type::Bool
        | Type::String
        | Type::Message
        | Type::Char
        | Type::Char8
        | Type::Char32
        | Type::None
        | Type::Any
        | Type::Comparable
        | Type::Unknown
        | Type::Never
        | Type::Range
        | Type::Enum(_)
        | Type::EnumType(_)
        | Type::Struct(_)
        | Type::StructType(_)
        | Type::Class(_)
        | Type::ClassType(_)
        | Type::Interface(_)
        | Type::InterfaceType(_)
        | Type::Module(_)
        | Type::ParametricType { .. } => false,
    }
}

pub(super) fn param_spec_contains_type_param(spec: &ParamSpec) -> bool {
    type_contains_type_param(&spec.value_type)
        || spec
            .tuple_items
            .as_ref()
            .is_some_and(|items| items.iter().any(param_spec_contains_type_param))
}

pub(super) fn substitute_param_spec(
    spec: &ParamSpec,
    inferred: &HashMap<String, Type>,
) -> ParamSpec {
    ParamSpec {
        name: spec.name.clone(),
        value_type: substitute_type_params(&spec.value_type, inferred),
        named: spec.named,
        has_default: spec.has_default,
        tuple_items: spec.tuple_items.as_ref().map(|items| {
            items
                .iter()
                .map(|item| substitute_param_spec(item, inferred))
                .collect()
        }),
    }
}

pub(super) fn collect_function_type_params(params: &[Param]) -> Result<Vec<TypeParam>, VerseError> {
    let mut collected = Vec::new();
    collect_function_type_params_inner(params, &mut collected)?;
    Ok(collected)
}

pub(super) fn collect_function_type_params_inner(
    params: &[Param],
    collected: &mut Vec<TypeParam>,
) -> Result<(), VerseError> {
    for param in params {
        for type_param in &param.type_params {
            if collected
                .iter()
                .any(|existing: &TypeParam| existing.name == type_param.name)
            {
                return Err(VerseError::check_at(
                    format!("duplicate type parameter `{}`", type_param.name),
                    type_param.span,
                ));
            }
            collected.push(type_param.clone());
        }
        if let ParamPattern::Tuple(items) = &param.pattern {
            collect_function_type_params_inner(items, collected)?;
        }
    }
    Ok(())
}

impl Checker {
    pub(super) fn predeclare_top_level_functions(
        &mut self,
        program: &Program,
    ) -> Result<(), VerseError> {
        self.predeclare_functions_in_current_scope(&program.statements)
    }

    pub(super) fn predeclare_functions_in_current_scope(
        &mut self,
        statements: &[Stmt],
    ) -> Result<(), VerseError> {
        for statement in statements {
            let StmtKind::Let { name, expr, .. } = &statement.kind else {
                continue;
            };
            let ExprKind::Function {
                params,
                effects,
                return_type,
                ..
            } = &expr.kind
            else {
                continue;
            };

            if self.type_aliases.contains_key(name) {
                return Err(VerseError::check_at(
                    format!("function `{name}` conflicts with type alias `{name}`"),
                    statement.span,
                ));
            }
            let type_params = collect_function_type_params(params)?;
            self.validate_type_parameter_constraints(&type_params, statement.span)?;
            self.push_type_param_scope(type_params.iter().map(|param| {
                (
                    param.name.clone(),
                    Type::Param(param.name.clone(), param.constraint.clone()),
                )
            }));
            let function_type = (|| {
                Ok(Type::Function {
                    arity: Some(params.len()),
                    arity_range: None,
                    effects: effects.clone(),
                    param_types: Some(self.param_types(params)?),
                    param_specs: Some(self.param_specs(params)?),
                    return_type: Box::new(self.annotation_to_type(return_type.as_ref())?),
                })
            })();
            self.pop_type_param_scope();
            let function_type = function_type?;
            self.define_predeclared_function(name, function_type, statement.span)?;
        }

        Ok(())
    }

    pub(super) fn define_predeclared_function(
        &mut self,
        name: &str,
        function_type: Type,
        span: Span,
    ) -> Result<(), VerseError> {
        let current = self
            .scopes
            .last_mut()
            .expect("checker should always have a scope");
        let Some(existing) = current.get_mut(name) else {
            current.insert(name.to_string(), Symbol::immutable(function_type));
            return Ok(());
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
                Ok(())
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
                Ok(())
            }
            _ => Err(VerseError::check_at(
                format!("duplicate definition `{name}`"),
                span,
            )),
        }
    }

    pub(super) fn predeclare_extension_methods_in_current_scope(
        &mut self,
        statements: &[Stmt],
    ) -> Result<(), VerseError> {
        for statement in statements {
            let StmtKind::ExtensionMethod(method) = &statement.kind else {
                continue;
            };
            self.register_extension_method_signature(method)?;
        }

        Ok(())
    }

    pub(super) fn register_extension_method_signature(
        &mut self,
        extension: &ExtensionMethod,
    ) -> Result<(), VerseError> {
        let receiver_type = self.extension_receiver_type(extension)?;
        let method_type = self.extension_method_declared_type(&extension.method)?;
        if self.current_definition_level() {
            ensure_private_protected_access_only_in_classes(
                &extension.method.effects,
                extension.span,
            )?;
        }
        let access = access_level_from_specifiers(
            &extension.method.effects,
            "extension method",
            extension.span,
        )?;
        let module_name = self.current_module_name();
        let methods = self
            .extension_methods
            .entry(extension.method.name.clone())
            .or_default();

        if let Some(existing) = methods.iter().find(|method| {
            method.module_name == module_name && method.receiver_type == receiver_type
        }) {
            return Err(VerseError::check_at(
                format!(
                    "duplicate extension method `{}` for receiver type `{receiver_type}`",
                    extension.method.name
                ),
                existing.span.through(extension.span),
            ));
        }

        methods.push(ExtensionMethodInfo {
            receiver_type,
            method_type,
            module_name,
            access,
            scopes: scoped_access_scopes(&extension.method.effects).unwrap_or_default(),
            span: extension.span,
        });

        Ok(())
    }

    pub(super) fn with_local_extension_methods<T>(
        &mut self,
        extensions: &[ExtensionMethod],
        f: impl FnOnce(&mut Self) -> Result<T, VerseError>,
    ) -> Result<T, VerseError> {
        let previous = self.extension_methods.clone();
        let result = self
            .register_local_extension_method_signatures(extensions)
            .and_then(|_| f(self));
        self.extension_methods = previous;
        result
    }

    pub(super) fn register_local_extension_method_signatures(
        &mut self,
        extensions: &[ExtensionMethod],
    ) -> Result<(), VerseError> {
        let mut local = Vec::with_capacity(extensions.len());
        for extension in extensions {
            let receiver_type = self.extension_receiver_type(extension)?;
            if local.iter().any(
                |(name, existing_receiver, _, _, _, _): &(
                    String,
                    Type,
                    Type,
                    AccessLevel,
                    Vec<String>,
                    Span,
                )| {
                    name == &extension.method.name && existing_receiver == &receiver_type
                },
            ) {
                return Err(VerseError::check_at(
                    format!(
                        "duplicate extension method `{}` for receiver type `{receiver_type}`",
                        extension.method.name
                    ),
                    extension.span,
                ));
            }
            let method_type = self.extension_method_declared_type(&extension.method)?;
            let access = access_level_from_specifiers(
                &extension.method.effects,
                "extension method",
                extension.span,
            )?;
            local.push((
                extension.method.name.clone(),
                receiver_type,
                method_type,
                access,
                scoped_access_scopes(&extension.method.effects).unwrap_or_default(),
                extension.span,
            ));
        }

        for (name, receiver_type, method_type, access, scopes, span) in local {
            let methods = self.extension_methods.entry(name).or_default();
            methods.retain(|method| method.receiver_type != receiver_type);
            methods.push(ExtensionMethodInfo {
                receiver_type,
                method_type,
                module_name: None,
                access,
                scopes,
                span,
            });
        }

        Ok(())
    }
}
