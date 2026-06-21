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
        let positive = match &param.constraint {
            TypeParamConstraint::Type => Some(TypeName::Any),
            TypeParamConstraint::Subtype(supertype) => Some(supertype.clone()),
        };
        Self {
            name: param.name.clone(),
            bounds: TypeVariableBounds {
                negative: None,
                positive,
            },
            explicit: true,
        }
    }

    pub fn from_type_params(params: &[TypeParam]) -> Vec<Self> {
        params.iter().map(Self::from_type_param).collect()
    }
}
