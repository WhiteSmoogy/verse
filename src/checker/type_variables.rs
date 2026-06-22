use crate::ast::{TypeName, TypeParam, TypeParamConstraint};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeVariableBounds {
    pub negative: Option<TypeName>,
    pub positive: Option<TypeName>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeVariable {
    pub name: String,
    pub bounds: TypeVariableBounds,
    pub explicit: bool,
}

impl TypeVariable {
    pub fn from_type_param(param: &TypeParam) -> Self {
        let (negative, positive) = match &param.constraint {
            TypeParamConstraint::Type => (None, Some(TypeName::Any)),
            TypeParamConstraint::Subtype(supertype) => (None, Some(supertype.clone())),
            TypeParamConstraint::TypeBounds { lower, upper } => {
                (Some(lower.clone()), Some(upper.clone()))
            }
        };
        Self {
            name: param.name.clone(),
            bounds: TypeVariableBounds { negative, positive },
            explicit: true,
        }
    }

    pub fn from_type_params(params: &[TypeParam]) -> Vec<Self> {
        params.iter().map(Self::from_type_param).collect()
    }
}
