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
            return_type: left_return_type,
            ..
        },
        Type::Function {
            arity: right_arity,
            arity_range: right_arity_range,
            param_types: right_param_types,
            param_specs: right_param_specs,
            return_type: right_return_type,
            ..
        },
    ) = (left, right)
    else {
        return false;
    };

    if left_arity_range != right_arity_range {
        return false;
    }

    let include_type_value_family_overlap = !type_can_be_used_as_type_value(left_return_type)
        && !type_can_be_used_as_type_value(right_return_type);

    if let (Some(left_specs), Some(right_specs)) =
        (left_param_specs.as_deref(), right_param_specs.as_deref())
    {
        return param_specs_overlap(
            left_specs,
            right_specs,
            struct_types,
            include_type_value_family_overlap,
        );
    }

    left_arity == right_arity
        && param_type_lists_overlap(
            left_param_types.as_deref(),
            right_param_types.as_deref(),
            struct_types,
            include_type_value_family_overlap,
        )
}

pub(super) fn param_specs_overlap(
    left: &[ParamSpec],
    right: &[ParamSpec],
    struct_types: &HashMap<String, StructInfo>,
    include_type_value_family_overlap: bool,
) -> bool {
    if param_specs_overlap_direct(
        left,
        right,
        struct_types,
        include_type_value_family_overlap,
    ) {
        return true;
    }

    let left_variants = expanded_single_tuple_param_spec_variants(left);
    let right_variants = expanded_single_tuple_param_spec_variants(right);

    for left_variant in &left_variants {
        if param_specs_overlap_direct(
            left_variant,
            right,
            struct_types,
            include_type_value_family_overlap,
        ) {
            return true;
        }
        for right_variant in &right_variants {
            if param_specs_overlap_direct(
                left_variant,
                right_variant,
                struct_types,
                include_type_value_family_overlap,
            ) {
                return true;
            }
        }
    }

    right_variants.iter().any(|right_variant| {
        param_specs_overlap_direct(
            left,
            right_variant,
            struct_types,
            include_type_value_family_overlap,
        )
    })
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
    include_type_value_family_overlap: bool,
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

    if !param_type_slices_overlap(
        &left_positional,
        &right_positional,
        struct_types,
        include_type_value_family_overlap,
    ) {
        return false;
    }

    let left_named = left.iter().filter(|spec| spec.named).collect::<Vec<_>>();
    let right_named = right.iter().filter(|spec| spec.named).collect::<Vec<_>>();

    required_named_params_are_accepted_by(
        &left_named,
        &right_named,
        struct_types,
        include_type_value_family_overlap,
    ) && required_named_params_are_accepted_by(
        &right_named,
        &left_named,
        struct_types,
        include_type_value_family_overlap,
    )
}

pub(super) fn required_named_params_are_accepted_by(
    required_source: &[&ParamSpec],
    target: &[&ParamSpec],
    struct_types: &HashMap<String, StructInfo>,
    include_type_value_family_overlap: bool,
) -> bool {
    required_source
        .iter()
        .filter(|spec| !spec.has_default)
        .all(|required| {
            target
                .iter()
                .find(|candidate| candidate.name == required.name)
                .is_some_and(|candidate| {
                    overload_param_types_overlap(
                        &required.value_type,
                        &candidate.value_type,
                        struct_types,
                        include_type_value_family_overlap,
                    )
                })
        })
}

pub(super) fn param_type_lists_overlap(
    left: Option<&[Type]>,
    right: Option<&[Type]>,
    struct_types: &HashMap<String, StructInfo>,
    include_type_value_family_overlap: bool,
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => {
            let left_refs = left.iter().collect::<Vec<_>>();
            let right_refs = right.iter().collect::<Vec<_>>();
            param_type_slices_overlap(
                &left_refs,
                &right_refs,
                struct_types,
                include_type_value_family_overlap,
            )
        }
        _ => true,
    }
}

pub(super) fn param_type_slices_overlap(
    left: &[&Type],
    right: &[&Type],
    struct_types: &HashMap<String, StructInfo>,
    include_type_value_family_overlap: bool,
) -> bool {
    if left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| {
                overload_param_types_overlap(
                    left,
                    right,
                    struct_types,
                    include_type_value_family_overlap,
                )
            })
    {
        return true;
    }

    if let [single] = left
        && single_param_overlaps_sequence(
            single,
            right,
            struct_types,
            include_type_value_family_overlap,
        )
    {
        return true;
    }

    if let [single] = right {
        return single_param_overlaps_sequence(
            single,
            left,
            struct_types,
            include_type_value_family_overlap,
        );
    }

    false
}

pub(super) fn single_param_overlaps_sequence(
    single: &Type,
    sequence: &[&Type],
    struct_types: &HashMap<String, StructInfo>,
    include_type_value_family_overlap: bool,
) -> bool {
    match single {
        Type::Tuple(items) if items.len() == sequence.len() => items
            .iter()
            .zip(sequence)
            .all(|(item, sequence_type)| {
                overload_param_types_overlap(
                    item,
                    sequence_type,
                    struct_types,
                    include_type_value_family_overlap,
                )
            }),
        _ => false,
    }
}

fn overload_param_types_overlap(
    left: &Type,
    right: &Type,
    struct_types: &HashMap<String, StructInfo>,
    include_type_value_family_overlap: bool,
) -> bool {
    types_not_distinct(left, right, struct_types)
        || (include_type_value_family_overlap
            && type_value_families_overlap(left, right, struct_types))
}

fn type_value_families_overlap(
    left: &Type,
    right: &Type,
    struct_types: &HashMap<String, StructInfo>,
) -> bool {
    match (left, right) {
        (Type::TypeValue, other) | (other, Type::TypeValue) => is_type_value_family(other),
        (
            Type::TypeValueBounds {
                lower: left_lower,
                upper: left_upper,
            },
            Type::TypeValueBounds {
                lower: right_lower,
                upper: right_upper,
            },
        ) => {
            types_not_distinct(left_upper, right_upper, struct_types)
                && types_not_distinct(left_lower, right_lower, struct_types)
        }
        (Type::TypeValueBounds { upper, .. }, other)
        | (other, Type::TypeValueBounds { upper, .. }) => subtype_family_base(other)
            .is_some_and(|base| types_not_distinct(upper, base, struct_types)),
        _ => match (subtype_family_base(left), subtype_family_base(right)) {
            (Some(left_base), Some(right_base)) => {
                types_not_distinct(left_base, right_base, struct_types)
            }
            _ => false,
        },
    }
}

fn is_type_value_family(value_type: &Type) -> bool {
    matches!(
        value_type,
        Type::TypeValue
            | Type::TypeValueOf(_)
            | Type::TypeValueBounds { .. }
            | Type::Subtype(_)
            | Type::CastableSubtype(_)
            | Type::ConcreteSubtype(_)
            | Type::ClassType(_)
            | Type::InterfaceType(_)
            | Type::StructType(_)
            | Type::EnumType(_)
    )
}

fn subtype_family_base(value_type: &Type) -> Option<&Type> {
    match value_type {
        Type::Subtype(base) | Type::CastableSubtype(base) => Some(base),
        Type::ConcreteSubtype(inner) => subtype_family_base(inner),
        _ => None,
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
        _ if numeric_types_overlap(left, right) => true,
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

fn numeric_types_overlap(left: &Type, right: &Type) -> bool {
    match (left, right) {
        (Type::Number, other) | (other, Type::Number) => numeric_domain(other).is_some(),
        (Type::Rational, other) | (other, Type::Rational) => {
            matches!(numeric_domain(other), Some(NumericDomain::Int | NumericDomain::Rational))
        }
        (Type::Int, other) | (other, Type::Int) => {
            matches!(
                numeric_domain(other),
                Some(NumericDomain::Int | NumericDomain::Rational)
            )
        }
        (Type::IntRange(left), Type::IntRange(right)) => int_ranges_overlap(*left, *right),
        (Type::IntRange(_), other) | (other, Type::IntRange(_)) => {
            matches!(
                numeric_domain(other),
                Some(NumericDomain::Int | NumericDomain::Rational)
            )
        }
        (Type::Float, other) | (other, Type::Float) => {
            matches!(numeric_domain(other), Some(NumericDomain::Float))
        }
        (Type::FloatRange(left), Type::FloatRange(right)) => float_ranges_overlap(*left, *right),
        (Type::FloatRange(_), other) | (other, Type::FloatRange(_)) => {
            matches!(numeric_domain(other), Some(NumericDomain::Float))
        }
        _ => false,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NumericDomain {
    Int,
    Rational,
    Float,
}

fn numeric_domain(value_type: &Type) -> Option<NumericDomain> {
    match value_type {
        Type::Int | Type::IntRange(_) => Some(NumericDomain::Int),
        Type::Rational => Some(NumericDomain::Rational),
        Type::Float | Type::FloatRange(_) => Some(NumericDomain::Float),
        Type::Number => None,
        _ => None,
    }
}

fn int_ranges_overlap(left: IntRange, right: IntRange) -> bool {
    left.min <= right.max && right.min <= left.max
}

fn float_ranges_overlap(left: FloatRange, right: FloatRange) -> bool {
    left.min.get() <= right.max.get() && right.min.get() <= left.max.get()
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

impl Checker {
    pub(super) fn infer_function_type_params(
        &mut self,
        param_types: Option<&[Type]>,
        return_type: Option<&Type>,
        arg_types: &[Type],
    ) -> Option<HashMap<String, Type>> {
        let mut inferred = infer_function_type_params(param_types, arg_types)?;
        let param_types = param_types?;
        if param_types.len() != arg_types.len() {
            return Some(inferred);
        }
        for (param_type, arg_type) in param_types.iter().zip(arg_types) {
            self.infer_parametric_instance_type_params(param_type, arg_type, &mut inferred)?;
        }
        for (param_type, arg_type) in param_types.iter().zip(arg_types) {
            self.infer_type_params_from_constraints(param_type, arg_type, &mut inferred)?;
        }
        let constraints = collect_type_param_constraints(param_types, return_type);
        self.infer_type_params_from_known_constraints(&constraints, &mut inferred)?;
        Some(inferred)
    }

    fn infer_type_params_from_known_constraints(
        &mut self,
        constraints: &HashMap<String, TypeParamConstraint>,
        inferred: &mut HashMap<String, Type>,
    ) -> Option<()> {
        loop {
            let previous_len = inferred.len();
            for name in inferred.keys().cloned().collect::<Vec<_>>() {
                let Some(constraint) = constraints.get(&name) else {
                    continue;
                };
                let actual = inferred.get(&name).cloned()?;
                self.infer_type_params_from_constraint(constraint, &actual, inferred)?;
            }
            if inferred.len() == previous_len {
                return Some(());
            }
        }
    }

    fn infer_type_params_from_constraints(
        &mut self,
        pattern: &Type,
        actual: &Type,
        inferred: &mut HashMap<String, Type>,
    ) -> Option<()> {
        match (pattern, actual) {
            (Type::Param(_, constraint), actual) => {
                self.infer_type_params_from_constraint(constraint, actual, inferred)
            }
            (Type::Array(pattern), Type::Array(actual))
            | (Type::Option(pattern), Type::Option(actual))
            | (Type::Task(pattern), Type::Task(actual))
            | (Type::Subtype(pattern), Type::Subtype(actual))
            | (Type::CastableSubtype(pattern), Type::CastableSubtype(actual))
            | (Type::ConcreteSubtype(pattern), Type::ConcreteSubtype(actual))
            | (Type::ClassifiableSubset(pattern), Type::ClassifiableSubset(actual))
            | (Type::ClassifiableSubsetKey(pattern), Type::ClassifiableSubsetKey(actual))
            | (Type::ClassifiableSubsetVar(pattern), Type::ClassifiableSubsetVar(actual))
            | (Type::Modifier(pattern), Type::Modifier(actual))
            | (Type::ModifierStack(pattern), Type::ModifierStack(actual))
            | (Type::Signalable(pattern), Type::Signalable(actual)) => {
                self.infer_type_params_from_constraints(pattern, actual, inferred)
            }
            (Type::Map(pattern_key, pattern_value), Type::Map(actual_key, actual_value))
            | (
                Type::WeakMap(pattern_key, pattern_value),
                Type::WeakMap(actual_key, actual_value),
            )
            | (Type::Result(pattern_key, pattern_value), Type::Result(actual_key, actual_value)) => {
                self.infer_type_params_from_constraints(pattern_key, actual_key, inferred)?;
                self.infer_type_params_from_constraints(pattern_value, actual_value, inferred)
            }
            (Type::SuccessResult(pattern), Type::SuccessResult(actual))
            | (Type::ErrorResult(pattern), Type::ErrorResult(actual)) => {
                self.infer_type_params_from_constraints(pattern, actual, inferred)
            }
            (Type::SuccessResult(pattern), Type::Result(actual_success, actual_error))
                if matches!(actual_error.as_ref(), Type::Never) =>
            {
                self.infer_type_params_from_constraints(pattern, actual_success, inferred)
            }
            (Type::ErrorResult(pattern), Type::Result(actual_success, actual_error))
                if matches!(actual_success.as_ref(), Type::Never) =>
            {
                self.infer_type_params_from_constraints(pattern, actual_error, inferred)
            }
            (Type::Tuple(pattern_items), Type::Tuple(actual_items))
                if pattern_items.len() == actual_items.len() =>
            {
                for (pattern, actual) in pattern_items.iter().zip(actual_items) {
                    self.infer_type_params_from_constraints(pattern, actual, inferred)?;
                }
                Some(())
            }
            (Type::Event(pattern), Type::Event(actual))
            | (Type::Generator(pattern), Type::Generator(actual))
            | (Type::Awaitable(pattern), Type::Awaitable(actual))
            | (Type::Subscribable(pattern), Type::Subscribable(actual))
            | (Type::Listenable(pattern), Type::Listenable(actual)) => match (pattern, actual) {
                (Some(pattern), Some(actual)) => {
                    self.infer_type_params_from_constraints(pattern, actual, inferred)
                }
                _ => Some(()),
            },
            (Type::SubscribableEvent(pattern), Type::SubscribableEvent(actual)) => {
                self.infer_type_params_from_constraints(pattern, actual, inferred)
            }
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
                if let (Some(pattern_params), Some(actual_params)) = (pattern_params, actual_params)
                {
                    if pattern_params.len() != actual_params.len() {
                        return None;
                    }
                    for (pattern, actual) in pattern_params.iter().zip(actual_params) {
                        self.infer_type_params_from_constraints(pattern, actual, inferred)?;
                    }
                }
                self.infer_type_params_from_constraints(pattern_return, actual_return, inferred)
            }
            _ => Some(()),
        }
    }

    pub(super) fn infer_type_params_from_constraint(
        &mut self,
        constraint: &TypeParamConstraint,
        actual: &Type,
        inferred: &mut HashMap<String, Type>,
    ) -> Option<()> {
        let parent = match constraint {
            TypeParamConstraint::Subtype(parent) => parent,
            TypeParamConstraint::TypeBounds { upper, .. } => upper,
            TypeParamConstraint::Type => return Some(()),
        };
        let pattern = self.type_name_to_inference_pattern(parent, inferred)?;
        infer_type_params_from_type(&pattern, actual, inferred)?;
        self.infer_parametric_instance_type_params(&pattern, actual, inferred)
    }

    fn type_name_to_inference_pattern(
        &mut self,
        name: &TypeName,
        inferred: &HashMap<String, Type>,
    ) -> Option<Type> {
        match name {
            TypeName::Int => Some(Type::Int),
            TypeName::Float => Some(Type::Float),
            TypeName::FloatRange(range) => Some(Type::FloatRange(*range)),
            TypeName::Rational => Some(Type::Rational),
            TypeName::Number => Some(Type::Number),
            TypeName::Bool => Some(Type::Bool),
            TypeName::String => Some(Type::String),
            TypeName::Message => Some(Type::Message),
            TypeName::Char => Some(Type::Char),
            TypeName::Char8 => Some(Type::Char8),
            TypeName::Char32 => Some(Type::Char32),
            TypeName::None => Some(Type::None),
            TypeName::Any => Some(Type::Any),
            TypeName::Comparable => Some(Type::Comparable),
            TypeName::Type => Some(Type::TypeValue),
            TypeName::TypeBounds { lower, upper } => Some(Type::TypeValueBounds {
                lower: Box::new(self.type_name_to_inference_pattern(lower, inferred)?),
                upper: Box::new(self.type_name_to_inference_pattern(upper, inferred)?),
            }),
            TypeName::IntRange { min, max } => Some(Type::IntRange(IntRange::new(*min, *max))),
            TypeName::Array(item) => Some(Type::Array(Box::new(match item.as_deref() {
                Some(item) => self.type_name_to_inference_pattern(item, inferred)?,
                None => Type::Unknown,
            }))),
            TypeName::Map(key, value) => Some(Type::Map(
                Box::new(self.type_name_to_inference_pattern(key, inferred)?),
                Box::new(self.type_name_to_inference_pattern(value, inferred)?),
            )),
            TypeName::WeakMap(key, value) => Some(Type::WeakMap(
                Box::new(self.type_name_to_inference_pattern(key, inferred)?),
                Box::new(self.type_name_to_inference_pattern(value, inferred)?),
            )),
            TypeName::Tuple(items) => Some(Type::Tuple(
                items
                    .iter()
                    .map(|item| self.type_name_to_inference_pattern(item, inferred))
                    .collect::<Option<Vec<_>>>()?,
            )),
            TypeName::Option(item) => Some(Type::Option(Box::new(
                self.type_name_to_inference_pattern(item, inferred)?,
            ))),
            TypeName::Function => Some(Type::Function {
                arity: None,
                arity_range: None,
                effects: Vec::new(),
                param_types: None,
                param_specs: None,
                return_type: Box::new(Type::Unknown),
            }),
            TypeName::FunctionSignature {
                params,
                effects,
                return_type,
            } => Some(Type::Function {
                arity: Some(params.len()),
                arity_range: None,
                effects: effects.clone(),
                param_types: Some(
                    params
                        .iter()
                        .map(|param| self.type_name_to_inference_pattern(param, inferred))
                        .collect::<Option<Vec<_>>>()?,
                ),
                param_specs: None,
                return_type: Box::new(self.type_name_to_inference_pattern(return_type, inferred)?),
            }),
            TypeName::Applied { name, args } => {
                let args = args
                    .iter()
                    .map(|arg| self.type_name_to_inference_pattern(arg, inferred))
                    .collect::<Option<Vec<_>>>()?;
                if is_official_parametric_type_name(name) {
                    official_parametric_type(name, &args, Span::new(0, 0, 0, 0)).ok()
                } else {
                    let qualified = self.resolve_parametric_type_reference(name)?;
                    let info = self.parametric_types.get(&qualified)?;
                    if info.params.len() != args.len() {
                        return None;
                    }
                    let instance_name = render_parametric_instance_type_name(&qualified, &args);
                    self.parametric_type_instances
                        .entry(instance_name.clone())
                        .or_insert_with(|| args.clone());
                    Some(match info.kind {
                        ParametricTypeKind::Struct => Type::Struct(instance_name),
                        ParametricTypeKind::Class => Type::Class(instance_name),
                        ParametricTypeKind::Interface => Type::Interface(instance_name),
                        ParametricTypeKind::Alias => return None,
                    })
                }
            }
            TypeName::Named(name) => {
                if let Some(value_type) = inferred.get(name) {
                    Some(value_type.clone())
                } else if let Some(value_type) = self.resolve_type_param(name) {
                    Some(value_type)
                } else if let Some(value_type) = self.type_aliases.get(name) {
                    Some(value_type.clone())
                } else if !name.contains('.')
                    && let Some(qualified) = self.resolve_contextual_type_name(name)
                    && let Some(value_type) = self.type_aliases.get(&qualified)
                {
                    Some(value_type.clone())
                } else {
                    self.named_type_to_type(name, Span::new(0, 0, 0, 0))
                        .ok()
                        .or_else(|| Some(Type::Param(name.clone(), TypeParamConstraint::Type)))
                }
            }
        }
    }

    fn infer_parametric_instance_type_params(
        &self,
        pattern: &Type,
        actual: &Type,
        inferred: &mut HashMap<String, Type>,
    ) -> Option<()> {
        match (pattern, actual) {
            (Type::Struct(pattern_name), Type::Struct(actual_name)) => {
                self.infer_parametric_instance_name_params(pattern_name, actual_name, inferred)
            }
            (Type::Class(pattern_name), Type::Class(actual_name)) => {
                self.infer_parametric_class_params_from_class(pattern_name, actual_name, inferred)
            }
            (Type::Interface(pattern_name), Type::Interface(actual_name)) => self
                .infer_parametric_interface_params_from_interface(
                    pattern_name,
                    actual_name,
                    inferred,
                ),
            (Type::Interface(pattern_name), Type::Class(actual_name)) => self
                .infer_parametric_interface_params_from_class(pattern_name, actual_name, inferred),
            (Type::Subtype(pattern), Type::Class(actual_name) | Type::ClassType(actual_name))
            | (
                Type::CastableSubtype(pattern),
                Type::Class(actual_name) | Type::ClassType(actual_name),
            )
            | (
                Type::ConcreteSubtype(pattern),
                Type::Class(actual_name) | Type::ClassType(actual_name),
            ) => self.infer_parametric_class_type_value_params(pattern, actual_name, inferred),
            (Type::TypeValueOf(pattern), actual) => {
                let actual = type_value_instance_type(actual)?;
                self.infer_parametric_instance_type_params(pattern, &actual, inferred)
            }
            (Type::Array(pattern), Type::Array(actual))
            | (Type::Option(pattern), Type::Option(actual))
            | (Type::Task(pattern), Type::Task(actual))
            | (Type::Subtype(pattern), Type::Subtype(actual))
            | (Type::CastableSubtype(pattern), Type::CastableSubtype(actual))
            | (Type::ConcreteSubtype(pattern), Type::ConcreteSubtype(actual))
            | (Type::ClassifiableSubset(pattern), Type::ClassifiableSubset(actual))
            | (Type::ClassifiableSubsetKey(pattern), Type::ClassifiableSubsetKey(actual))
            | (Type::ClassifiableSubsetVar(pattern), Type::ClassifiableSubsetVar(actual))
            | (Type::Modifier(pattern), Type::Modifier(actual))
            | (Type::ModifierStack(pattern), Type::ModifierStack(actual))
            | (Type::Signalable(pattern), Type::Signalable(actual)) => {
                self.infer_parametric_instance_type_params(pattern, actual, inferred)
            }
            (Type::Map(pattern_key, pattern_value), Type::Map(actual_key, actual_value))
            | (
                Type::WeakMap(pattern_key, pattern_value),
                Type::WeakMap(actual_key, actual_value),
            )
            | (Type::Result(pattern_key, pattern_value), Type::Result(actual_key, actual_value)) => {
                self.infer_parametric_instance_type_params(pattern_key, actual_key, inferred)?;
                self.infer_parametric_instance_type_params(pattern_value, actual_value, inferred)
            }
            (Type::SuccessResult(pattern), Type::SuccessResult(actual))
            | (Type::ErrorResult(pattern), Type::ErrorResult(actual)) => {
                self.infer_parametric_instance_type_params(pattern, actual, inferred)
            }
            (Type::SuccessResult(pattern), Type::Result(actual_success, actual_error))
                if matches!(actual_error.as_ref(), Type::Never) =>
            {
                self.infer_parametric_instance_type_params(pattern, actual_success, inferred)
            }
            (Type::ErrorResult(pattern), Type::Result(actual_success, actual_error))
                if matches!(actual_success.as_ref(), Type::Never) =>
            {
                self.infer_parametric_instance_type_params(pattern, actual_error, inferred)
            }
            (Type::Tuple(pattern_items), Type::Tuple(actual_items))
                if pattern_items.len() == actual_items.len() =>
            {
                for (pattern, actual) in pattern_items.iter().zip(actual_items) {
                    self.infer_parametric_instance_type_params(pattern, actual, inferred)?;
                }
                Some(())
            }
            (Type::Event(pattern), Type::Event(actual))
            | (Type::Generator(pattern), Type::Generator(actual))
            | (Type::Awaitable(pattern), Type::Awaitable(actual))
            | (Type::Subscribable(pattern), Type::Subscribable(actual))
            | (Type::Listenable(pattern), Type::Listenable(actual)) => match (pattern, actual) {
                (Some(pattern), Some(actual)) => {
                    self.infer_parametric_instance_type_params(pattern, actual, inferred)
                }
                _ => Some(()),
            },
            (Type::SubscribableEvent(pattern), Type::SubscribableEvent(actual)) => {
                self.infer_parametric_instance_type_params(pattern, actual, inferred)
            }
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
                if let (Some(pattern_params), Some(actual_params)) = (pattern_params, actual_params)
                {
                    if pattern_params.len() != actual_params.len() {
                        return None;
                    }
                    for (pattern, actual) in pattern_params.iter().zip(actual_params) {
                        self.infer_parametric_instance_type_params(pattern, actual, inferred)?;
                    }
                }
                self.infer_parametric_instance_type_params(pattern_return, actual_return, inferred)
            }
            _ => Some(()),
        }
    }

    fn infer_parametric_class_type_value_params(
        &self,
        pattern: &Type,
        actual_name: &str,
        inferred: &mut HashMap<String, Type>,
    ) -> Option<()> {
        match pattern {
            Type::Subtype(pattern)
            | Type::CastableSubtype(pattern)
            | Type::ConcreteSubtype(pattern) => {
                self.infer_parametric_class_type_value_params(pattern, actual_name, inferred)
            }
            pattern => {
                let actual = Type::Class(actual_name.to_string());
                infer_type_params_from_type(pattern, &actual, inferred)?;
                self.infer_parametric_instance_type_params(pattern, &actual, inferred)
            }
        }
    }

    fn infer_parametric_class_params_from_class(
        &self,
        pattern_name: &str,
        actual_name: &str,
        inferred: &mut HashMap<String, Type>,
    ) -> Option<()> {
        let mut current = Some(actual_name);
        while let Some(name) = current {
            self.infer_parametric_instance_name_params(pattern_name, name, inferred)?;
            current = self
                .struct_types
                .get(name)
                .and_then(|info| info.base.as_deref());
        }
        Some(())
    }

    fn infer_parametric_interface_params_from_class(
        &self,
        pattern_name: &str,
        actual_name: &str,
        inferred: &mut HashMap<String, Type>,
    ) -> Option<()> {
        let mut current = Some(actual_name);
        while let Some(name) = current {
            let Some(info) = self.struct_types.get(name) else {
                return Some(());
            };
            for interface in &info.interfaces {
                self.infer_parametric_interface_params_from_interface(
                    pattern_name,
                    interface,
                    inferred,
                )?;
            }
            current = info.base.as_deref();
        }
        Some(())
    }

    fn infer_parametric_interface_params_from_interface(
        &self,
        pattern_name: &str,
        actual_name: &str,
        inferred: &mut HashMap<String, Type>,
    ) -> Option<()> {
        self.infer_parametric_instance_name_params(pattern_name, actual_name, inferred)?;
        let Some(info) = self.interface_types.get(actual_name) else {
            return Some(());
        };
        for parent in &info.parents {
            self.infer_parametric_interface_params_from_interface(pattern_name, parent, inferred)?;
        }
        Some(())
    }

    fn infer_parametric_instance_name_params(
        &self,
        pattern_name: &str,
        actual_name: &str,
        inferred: &mut HashMap<String, Type>,
    ) -> Option<()> {
        let Some(pattern_head) = parametric_instance_head(pattern_name) else {
            return Some(());
        };
        let Some(actual_head) = parametric_instance_head(actual_name) else {
            return Some(());
        };
        if pattern_head != actual_head {
            return Some(());
        }
        let Some(pattern_args) = self.parametric_type_instances.get(pattern_name) else {
            return Some(());
        };
        let Some(actual_args) = self.parametric_type_instances.get(actual_name) else {
            return Some(());
        };
        if pattern_args.len() != actual_args.len() {
            return Some(());
        }
        for (pattern, actual) in pattern_args.iter().zip(actual_args) {
            infer_type_params_from_type(pattern, actual, inferred)?;
            self.infer_parametric_instance_type_params(pattern, actual, inferred)?;
        }
        Some(())
    }
}

fn parametric_instance_head(name: &str) -> Option<&str> {
    let open = name.find('(')?;
    name.ends_with(')').then_some(&name[..open])
}

fn collect_type_param_constraints(
    param_types: &[Type],
    return_type: Option<&Type>,
) -> HashMap<String, TypeParamConstraint> {
    let mut constraints = HashMap::new();
    for param_type in param_types {
        collect_type_param_constraints_inner(param_type, &mut constraints);
    }
    if let Some(return_type) = return_type {
        collect_type_param_constraints_inner(return_type, &mut constraints);
    }
    constraints
}

fn collect_type_param_constraints_inner(
    value_type: &Type,
    constraints: &mut HashMap<String, TypeParamConstraint>,
) {
    match value_type {
        Type::Param(name, constraint) => {
            constraints
                .entry(name.clone())
                .or_insert_with(|| constraint.clone());
        }
        Type::Array(item)
        | Type::Option(item)
        | Type::Task(item)
        | Type::TypeValueOf(item)
        | Type::Subtype(item)
        | Type::CastableSubtype(item)
        | Type::ConcreteSubtype(item)
        | Type::ClassifiableSubset(item)
        | Type::ClassifiableSubsetKey(item)
        | Type::ClassifiableSubsetVar(item)
        | Type::Modifier(item)
        | Type::ModifierStack(item)
        | Type::Signalable(item) => {
            collect_type_param_constraints_inner(item, constraints);
        }
        Type::Map(key, value) | Type::WeakMap(key, value) | Type::Result(key, value) => {
            collect_type_param_constraints_inner(key, constraints);
            collect_type_param_constraints_inner(value, constraints);
        }
        Type::SuccessResult(item) | Type::ErrorResult(item) => {
            collect_type_param_constraints_inner(item, constraints);
        }
        Type::TypeValueBounds { lower, upper } => {
            collect_type_param_constraints_inner(lower, constraints);
            collect_type_param_constraints_inner(upper, constraints);
        }
        Type::Tuple(items) | Type::Overload(items) => {
            for item in items {
                collect_type_param_constraints_inner(item, constraints);
            }
        }
        Type::Event(payload)
        | Type::SubscribableEventIntrnl(payload)
        | Type::StickyEvent(payload)
        | Type::Generator(payload)
        | Type::Awaitable(payload)
        | Type::Subscribable(payload)
        | Type::Listenable(payload) => {
            if let Some(payload) = payload {
                collect_type_param_constraints_inner(payload, constraints);
            }
        }
        Type::SubscribableEvent(payload) => {
            collect_type_param_constraints_inner(payload, constraints);
        }
        Type::Function {
            param_types,
            param_specs,
            return_type,
            ..
        } => {
            if let Some(param_types) = param_types {
                for param_type in param_types {
                    collect_type_param_constraints_inner(param_type, constraints);
                }
            }
            if let Some(param_specs) = param_specs {
                for spec in param_specs {
                    collect_param_spec_type_param_constraints(spec, constraints);
                }
            }
            collect_type_param_constraints_inner(return_type, constraints);
        }
        Type::Int
        | Type::IntRange(_)
        | Type::Float
        | Type::FloatRange(_)
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
        | Type::TypeValue
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
        | Type::ParametricType { .. } => {}
    }
}

fn collect_param_spec_type_param_constraints(
    spec: &ParamSpec,
    constraints: &mut HashMap<String, TypeParamConstraint>,
) {
    collect_type_param_constraints_inner(&spec.value_type, constraints);
    if let Some(items) = &spec.tuple_items {
        for item in items {
            collect_param_spec_type_param_constraints(item, constraints);
        }
    }
}

pub(super) fn infer_type_params_from_type(
    pattern: &Type,
    actual: &Type,
    inferred: &mut HashMap<String, Type>,
) -> Option<()> {
    match (pattern, actual) {
        (Type::Param(name, _), actual) => {
            if inferred
                .get(name)
                .is_none_or(|existing| unresolved_type_function_inferred_param(existing, name))
            {
                inferred.insert(name.clone(), actual.clone());
            }
            Some(())
        }
        (Type::TypeValueOf(pattern), actual) => {
            let actual = type_value_instance_type(actual)?;
            infer_type_params_from_type(pattern, &actual, inferred)
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
        | (Type::Subtype(pattern), Type::Subtype(actual))
        | (Type::CastableSubtype(pattern), Type::CastableSubtype(actual))
        | (Type::ConcreteSubtype(pattern), Type::ConcreteSubtype(actual))
        | (Type::ClassifiableSubset(pattern), Type::ClassifiableSubset(actual))
        | (Type::ClassifiableSubsetKey(pattern), Type::ClassifiableSubsetKey(actual))
        | (Type::ClassifiableSubsetVar(pattern), Type::ClassifiableSubsetVar(actual))
        | (Type::Modifier(pattern), Type::Modifier(actual))
        | (Type::ModifierStack(pattern), Type::ModifierStack(actual))
        | (Type::Signalable(pattern), Type::Signalable(actual)) => {
            infer_type_params_from_type(pattern, actual, inferred)
        }
        (Type::Subtype(pattern), Type::ClassType(actual_name))
        | (Type::CastableSubtype(pattern), Type::ClassType(actual_name))
        | (Type::ConcreteSubtype(pattern), Type::ClassType(actual_name)) => {
            infer_type_params_from_type(pattern, &Type::Class(actual_name.clone()), inferred)
        }
        (
            Type::Result(pattern_success, pattern_error),
            Type::Result(actual_success, actual_error),
        ) => {
            infer_type_params_from_type(pattern_success, actual_success, inferred)?;
            infer_type_params_from_type(pattern_error, actual_error, inferred)
        }
        (Type::SuccessResult(pattern), Type::SuccessResult(actual))
        | (Type::ErrorResult(pattern), Type::ErrorResult(actual)) => {
            infer_type_params_from_type(pattern, actual, inferred)
        }
        (Type::SuccessResult(pattern), Type::Result(actual_success, actual_error))
            if matches!(actual_error.as_ref(), Type::Never) =>
        {
            infer_type_params_from_type(pattern, actual_success, inferred)
        }
        (Type::ErrorResult(pattern), Type::Result(actual_success, actual_error))
            if matches!(actual_success.as_ref(), Type::Never) =>
        {
            infer_type_params_from_type(pattern, actual_error, inferred)
        }
        (Type::Event(pattern), Type::Event(actual))
        | (Type::Generator(pattern), Type::Generator(actual))
        | (Type::Awaitable(pattern), Type::Awaitable(actual))
        | (Type::Subscribable(pattern), Type::Subscribable(actual))
        | (Type::Listenable(pattern), Type::Listenable(actual)) => match (pattern, actual) {
            (Some(pattern), Some(actual)) => infer_type_params_from_type(pattern, actual, inferred),
            _ => Some(()),
        },
        (Type::SubscribableEvent(pattern), Type::SubscribableEvent(actual)) => {
            infer_type_params_from_type(pattern, actual, inferred)
        }
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

pub(super) fn unresolved_type_function_inferred_param(value_type: &Type, name: &str) -> bool {
    matches!(value_type, Type::Param(param_name, _) if param_name == name)
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
        Type::SuccessResult(item) => {
            Type::SuccessResult(Box::new(substitute_type_params(item, inferred)))
        }
        Type::ErrorResult(item) => {
            Type::ErrorResult(Box::new(substitute_type_params(item, inferred)))
        }
        Type::Event(payload) => Type::Event(
            payload
                .as_deref()
                .map(|payload| Box::new(substitute_type_params(payload, inferred))),
        ),
        Type::SubscribableEvent(payload) => {
            Type::SubscribableEvent(Box::new(substitute_type_params(payload, inferred)))
        }
        Type::SubscribableEventIntrnl(payload) => Type::SubscribableEventIntrnl(
            payload
                .as_deref()
                .map(|payload| Box::new(substitute_type_params(payload, inferred))),
        ),
        Type::StickyEvent(payload) => Type::StickyEvent(
            payload
                .as_deref()
                .map(|payload| Box::new(substitute_type_params(payload, inferred))),
        ),
        Type::Task(payload) => Type::Task(Box::new(substitute_type_params(payload, inferred))),
        Type::TypeValueOf(item) => {
            Type::TypeValueOf(Box::new(substitute_type_params(item, inferred)))
        }
        Type::TypeValueBounds { lower, upper } => Type::TypeValueBounds {
            lower: Box::new(substitute_type_params(lower, inferred)),
            upper: Box::new(substitute_type_params(upper, inferred)),
        },
        Type::Generator(payload) => Type::Generator(
            payload
                .as_deref()
                .map(|payload| Box::new(substitute_type_params(payload, inferred))),
        ),
        Type::Subtype(item) => Type::Subtype(Box::new(substitute_type_params(item, inferred))),
        Type::CastableSubtype(item) => {
            Type::CastableSubtype(Box::new(substitute_type_params(item, inferred)))
        }
        Type::ConcreteSubtype(item) => {
            Type::ConcreteSubtype(Box::new(substitute_type_params(item, inferred)))
        }
        Type::ClassifiableSubset(item) => {
            Type::ClassifiableSubset(Box::new(substitute_type_params(item, inferred)))
        }
        Type::ClassifiableSubsetKey(item) => {
            Type::ClassifiableSubsetKey(Box::new(substitute_type_params(item, inferred)))
        }
        Type::ClassifiableSubsetVar(item) => {
            Type::ClassifiableSubsetVar(Box::new(substitute_type_params(item, inferred)))
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
        | Type::TypeValueOf(item)
        | Type::Subtype(item)
        | Type::CastableSubtype(item)
        | Type::ConcreteSubtype(item)
        | Type::ClassifiableSubset(item)
        | Type::ClassifiableSubsetKey(item)
        | Type::ClassifiableSubsetVar(item)
        | Type::Modifier(item)
        | Type::ModifierStack(item)
        | Type::Signalable(item) => type_contains_type_param(item),
        Type::TypeValueBounds { lower, upper } => {
            type_contains_type_param(lower) || type_contains_type_param(upper)
        }
        Type::Map(key, value) | Type::WeakMap(key, value) | Type::Result(key, value) => {
            type_contains_type_param(key) || type_contains_type_param(value)
        }
        Type::SuccessResult(item) | Type::ErrorResult(item) => type_contains_type_param(item),
        Type::Tuple(items) | Type::Overload(items) => items.iter().any(type_contains_type_param),
        Type::Event(payload)
        | Type::SubscribableEventIntrnl(payload)
        | Type::StickyEvent(payload)
        | Type::Generator(payload)
        | Type::Awaitable(payload)
        | Type::Subscribable(payload)
        | Type::Listenable(payload) => payload.as_deref().is_some_and(type_contains_type_param),
        Type::SubscribableEvent(payload) => type_contains_type_param(payload),
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
        | Type::FloatRange(_)
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
        | Type::TypeValue
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

pub(super) fn merge_type_param_lists(
    first: &[TypeParam],
    second: &[TypeParam],
) -> Result<Vec<TypeParam>, VerseError> {
    let mut merged = first.to_vec();
    for param in second {
        if let Some(existing) = merged.iter().find(|existing| existing.name == param.name) {
            if existing.constraint == param.constraint {
                continue;
            }
            return Err(VerseError::check_at(
                format!("duplicate type parameter `{}`", param.name),
                param.span,
            ));
        }
        merged.push(param.clone());
    }
    Ok(merged)
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

fn type_value_type_to_type_param_constraint(value_type: &Type) -> Option<TypeParamConstraint> {
    match value_type {
        Type::TypeValue => Some(TypeParamConstraint::Type),
        Type::TypeValueBounds { lower, upper } => Some(TypeParamConstraint::TypeBounds {
            lower: type_to_constraint_type_name(lower)?,
            upper: type_to_constraint_type_name(upper)?,
        }),
        Type::Subtype(parent) => Some(TypeParamConstraint::Subtype(type_to_constraint_type_name(
            parent,
        )?)),
        Type::CastableSubtype(parent) => Some(TypeParamConstraint::Subtype(TypeName::Applied {
            name: "castable_subtype".to_string(),
            args: vec![type_to_constraint_type_name(parent)?],
        })),
        Type::ConcreteSubtype(parent) => Some(TypeParamConstraint::Subtype(TypeName::Applied {
            name: "concrete_subtype".to_string(),
            args: vec![type_to_constraint_type_name(parent)?],
        })),
        _ => None,
    }
}

fn type_function_target_to_type_param_constraint(target: &TypeName) -> Option<TypeParamConstraint> {
    match target {
        TypeName::Type => Some(TypeParamConstraint::Type),
        TypeName::TypeBounds { lower, upper } => Some(TypeParamConstraint::TypeBounds {
            lower: lower.as_ref().clone(),
            upper: upper.as_ref().clone(),
        }),
        TypeName::Applied { name, args }
            if matches!(
                name.as_str(),
                "subtype" | "castable_subtype" | "concrete_subtype" | "castable_concrete_subtype"
            ) && args.len() == 1 =>
        {
            let parent = if name == "subtype" {
                args[0].clone()
            } else {
                TypeName::Applied {
                    name: name.clone(),
                    args: args.clone(),
                }
            };
            Some(TypeParamConstraint::Subtype(parent))
        }
        _ => None,
    }
}

enum TypeFunctionParamMatch {
    Match(Vec<TypeParam>, Vec<TypeParam>),
    NotTypeFunction,
    Pending,
}

enum TypeFunctionParamConstraintMatch {
    Match(TypeParamConstraint),
    NotTypeFunction,
    Pending,
}

impl Checker {
    pub(super) fn type_function_params(
        &mut self,
        params: &[Param],
    ) -> Result<Option<(Vec<TypeParam>, Vec<TypeParam>)>, VerseError> {
        match self.match_type_function_params(params)? {
            TypeFunctionParamMatch::Match(type_params, inferred_type_params) => {
                Ok(Some((type_params, inferred_type_params)))
            }
            TypeFunctionParamMatch::NotTypeFunction | TypeFunctionParamMatch::Pending => Ok(None),
        }
    }

    fn match_type_function_params(
        &mut self,
        params: &[Param],
    ) -> Result<TypeFunctionParamMatch, VerseError> {
        let mut type_params = Vec::new();
        let mut inferred_type_params = Vec::new();
        for param in params {
            if param.named
                || param.default.is_some()
                || param.name.is_empty()
                || !matches!(param.pattern, ParamPattern::Binding)
            {
                return Ok(TypeFunctionParamMatch::NotTypeFunction);
            }
            inferred_type_params.extend(param.type_params.iter().cloned());
            let Some(annotation) = param.annotation.as_ref() else {
                return Ok(TypeFunctionParamMatch::NotTypeFunction);
            };
            let param_scope = inferred_type_params
                .iter()
                .chain(&type_params)
                .map(|param| {
                    (
                        param.name.clone(),
                        Type::Param(param.name.clone(), param.constraint.clone()),
                    )
                })
                .collect::<Vec<_>>();
            self.push_type_param_scope(param_scope);
            let constraint_match =
                self.type_function_param_constraint(&annotation.name, annotation.span);
            self.pop_type_param_scope();
            let constraint = match constraint_match? {
                TypeFunctionParamConstraintMatch::Match(constraint) => constraint,
                TypeFunctionParamConstraintMatch::NotTypeFunction => {
                    return Ok(TypeFunctionParamMatch::NotTypeFunction);
                }
                TypeFunctionParamConstraintMatch::Pending => {
                    return Ok(TypeFunctionParamMatch::Pending);
                }
            };
            if type_params
                .iter()
                .any(|existing: &TypeParam| existing.name == param.name)
            {
                return Ok(TypeFunctionParamMatch::NotTypeFunction);
            }
            type_params.push(TypeParam {
                name: param.name.clone(),
                constraint,
                span: param.span,
            });
        }
        Ok(TypeFunctionParamMatch::Match(
            type_params,
            inferred_type_params,
        ))
    }

    fn type_function_param_constraint(
        &mut self,
        annotation: &TypeName,
        span: Span,
    ) -> Result<TypeFunctionParamConstraintMatch, VerseError> {
        match annotation {
            TypeName::Type => Ok(TypeFunctionParamConstraintMatch::Match(
                TypeParamConstraint::Type,
            )),
            TypeName::TypeBounds { lower, upper } => Ok(TypeFunctionParamConstraintMatch::Match(
                TypeParamConstraint::TypeBounds {
                    lower: lower.as_ref().clone(),
                    upper: upper.as_ref().clone(),
                },
            )),
            TypeName::Applied { name, args }
                if matches!(
                    name.as_str(),
                    "subtype"
                        | "castable_subtype"
                        | "concrete_subtype"
                        | "castable_concrete_subtype"
                ) && args.len() == 1 =>
            {
                let parent = if name == "subtype" {
                    args[0].clone()
                } else {
                    TypeName::Applied {
                        name: name.clone(),
                        args: args.clone(),
                    }
                };
                Ok(TypeFunctionParamConstraintMatch::Match(
                    TypeParamConstraint::Subtype(parent),
                ))
            }
            TypeName::Applied { name, .. } if is_official_parametric_type_name(name) => {
                Ok(TypeFunctionParamConstraintMatch::NotTypeFunction)
            }
            TypeName::Applied { name, args } => {
                let Some(qualified) = self.resolve_type_function_reference(name) else {
                    return Ok(if name.contains('.') {
                        TypeFunctionParamConstraintMatch::NotTypeFunction
                    } else {
                        TypeFunctionParamConstraintMatch::Pending
                    });
                };
                let args = match args
                    .iter()
                    .map(|arg| self.type_name_to_type_name(arg, span))
                    .collect::<Result<Vec<_>, _>>()
                {
                    Ok(args) => args,
                    Err(_) => return Ok(TypeFunctionParamConstraintMatch::Pending),
                };
                let target = match self
                    .instantiate_type_function_target_name(name, &qualified, &args, span)
                {
                    Ok(target) => target,
                    Err(_) => return Ok(TypeFunctionParamConstraintMatch::Pending),
                };
                if let Some(constraint) = target
                    .as_ref()
                    .and_then(type_function_target_to_type_param_constraint)
                {
                    return Ok(TypeFunctionParamConstraintMatch::Match(constraint));
                }
                let value_type = match self.instantiate_type_function(name, &qualified, &args, span)
                {
                    Ok(value_type) => value_type,
                    Err(_) => return Ok(TypeFunctionParamConstraintMatch::Pending),
                };
                Ok(type_value_type_to_type_param_constraint(&value_type)
                    .map(TypeFunctionParamConstraintMatch::Match)
                    .unwrap_or(TypeFunctionParamConstraintMatch::NotTypeFunction))
            }
            _ => Ok(TypeFunctionParamConstraintMatch::NotTypeFunction),
        }
    }

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
                let (param_types, param_specs, _) =
                    self.dependent_type_value_param_signature(params, return_type.as_ref())?;
                Ok(Type::Function {
                    arity: Some(params.len()),
                    arity_range: None,
                    effects: effects.clone(),
                    param_types: Some(param_types),
                    param_specs: Some(param_specs),
                    return_type: Box::new(self.annotation_to_type(return_type.as_ref())?),
                })
            })();
            self.pop_type_param_scope();
            let function_type = function_type?;
            self.define_predeclared_function(name, function_type, statement.span)?;
        }

        Ok(())
    }

    pub(super) fn predeclare_top_level_type_functions(
        &mut self,
        program: &Program,
    ) -> Result<(), VerseError> {
        self.predeclare_type_functions_recursive(&program.statements)
    }

    fn predeclare_type_functions_recursive(
        &mut self,
        statements: &[Stmt],
    ) -> Result<(), VerseError> {
        self.predeclare_type_functions_in_current_scope(statements)?;
        for statement in statements {
            let StmtKind::Let { name, expr, .. } = &statement.kind else {
                continue;
            };
            let ExprKind::ModuleDefinition {
                statements: module_statements,
                ..
            } = &expr.kind
            else {
                continue;
            };
            self.module_path.push(name.clone());
            self.predeclare_type_functions_recursive(module_statements)?;
            self.module_path.pop();
        }
        Ok(())
    }

    pub(super) fn predeclare_type_functions_in_current_scope(
        &mut self,
        statements: &[Stmt],
    ) -> Result<(), VerseError> {
        let mut pending = vec![true; statements.len()];

        loop {
            let mut progressed = false;

            for (index, statement) in statements.iter().enumerate() {
                if !pending[index] {
                    continue;
                }

                let StmtKind::Let {
                    name,
                    specifiers,
                    expr,
                    ..
                } = &statement.kind
                else {
                    pending[index] = false;
                    continue;
                };
                let ExprKind::Function {
                    params,
                    effects,
                    return_type,
                    body,
                } = &expr.kind
                else {
                    pending[index] = false;
                    continue;
                };

                let (type_params, inferred_type_params) =
                    match self.match_type_function_params(params)? {
                        TypeFunctionParamMatch::Match(type_params, inferred_type_params) => {
                            (type_params, inferred_type_params)
                        }
                        TypeFunctionParamMatch::NotTypeFunction => {
                            pending[index] = false;
                            continue;
                        }
                        TypeFunctionParamMatch::Pending => continue,
                    };
                let all_type_params = merge_type_param_lists(&inferred_type_params, &type_params)?;
                self.validate_type_parameter_constraints(&all_type_params, statement.span)?;
                let Some(return_type) = return_type.as_ref() else {
                    pending[index] = false;
                    continue;
                };
                self.push_type_param_scope(all_type_params.iter().map(|param| {
                    (
                        param.name.clone(),
                        Type::Param(param.name.clone(), param.constraint.clone()),
                    )
                }));
                let signature_parts = (|| {
                    let param_types = params
                        .iter()
                        .map(|param| self.annotation_to_type(param.annotation.as_ref()))
                        .collect::<Result<Vec<_>, _>>()?;
                    let param_specs = params
                        .iter()
                        .zip(&param_types)
                        .map(|(param, value_type)| ParamSpec {
                            name: param.name.clone(),
                            value_type: value_type.clone(),
                            named: param.named,
                            has_default: param.default.is_some(),
                            tuple_items: None,
                        })
                        .collect::<Vec<_>>();
                    // This pass is speculative; ordinary zero-arg runtime functions can look like
                    // type functions until their return annotation resolves.
                    let Ok(return_value_type) = self.annotation_to_type(Some(return_type)) else {
                        return Ok(None);
                    };
                    Ok(Some((param_types, param_specs, return_value_type)))
                })();
                self.pop_type_param_scope();
                let Some((param_types, param_specs, return_value_type)) = signature_parts? else {
                    pending[index] = false;
                    continue;
                };
                if !type_can_be_used_as_type_value(&return_value_type) {
                    pending[index] = false;
                    continue;
                }
                let Ok(type_name) = self.expr_to_type_name(body) else {
                    continue;
                };
                let qualified = self.current_qualified_name(name);
                self.type_functions
                    .entry(qualified)
                    .or_default()
                    .push(TypeFunctionInfo {
                        params: type_params,
                        inferred_params: inferred_type_params,
                        target: type_name,
                        module_path: self.module_path.clone(),
                        signature: Type::Function {
                            arity: Some(params.len()),
                            arity_range: None,
                            effects: effects.clone(),
                            param_types: Some(param_types),
                            param_specs: Some(param_specs),
                            return_type: Box::new(return_value_type),
                        },
                    });
                self.record_current_module_member_access(
                    name,
                    module_member_specifiers(specifiers, expr),
                    statement.span,
                )?;
                pending[index] = false;
                progressed = true;
            }

            if !progressed {
                break;
            }
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
        let method_type = self.extension_declared_method_type(extension)?;
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
            let method_type = self.extension_declared_method_type(extension)?;
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
