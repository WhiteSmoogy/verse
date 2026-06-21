use crate::error::VerseError;
use crate::token::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Effect {
    NoRollback,
    Converges,
    Computes,
    Varies,
    Transacts,
    Reads,
    Writes,
    Allocates,
    Suspends,
    Decides,
}

impl Effect {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "no_rollback" => Some(Self::NoRollback),
            "converges" => Some(Self::Converges),
            "computes" => Some(Self::Computes),
            "varies" => Some(Self::Varies),
            "transacts" => Some(Self::Transacts),
            "reads" => Some(Self::Reads),
            "writes" => Some(Self::Writes),
            "allocates" => Some(Self::Allocates),
            "suspends" => Some(Self::Suspends),
            "decides" => Some(Self::Decides),
            _ => None,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::NoRollback => "no_rollback",
            Self::Converges => "converges",
            Self::Computes => "computes",
            Self::Varies => "varies",
            Self::Transacts => "transacts",
            Self::Reads => "reads",
            Self::Writes => "writes",
            Self::Allocates => "allocates",
            Self::Suspends => "suspends",
            Self::Decides => "decides",
        }
    }

    const fn is_explicit_call_effect(self) -> bool {
        matches!(
            self,
            Self::Transacts
                | Self::Varies
                | Self::Computes
                | Self::Converges
                | Self::Reads
                | Self::Writes
                | Self::Allocates
        )
    }

    const fn is_declared_function_effect(self) -> bool {
        !matches!(self, Self::NoRollback)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectSet {
    effects: Vec<Effect>,
}

impl EffectSet {
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        let mut effects = Vec::new();
        for name in names {
            if let Some(effect) = Effect::from_name(name) {
                push_effect(&mut effects, effect);
            }
        }
        Self { effects }
    }

    pub fn from_effect_names(effects: &[String]) -> Self {
        Self::from_names(effects.iter().map(String::as_str))
    }

    pub fn contains(&self, effect: Effect) -> bool {
        self.effects.iter().any(|existing| *existing == effect)
    }

    pub fn has_explicit_call_effect(&self) -> bool {
        self.effects
            .iter()
            .any(|effect| effect.is_explicit_call_effect())
    }

    pub fn has_no_rollback(&self) -> bool {
        self.contains(Effect::NoRollback) || !self.has_explicit_call_effect()
    }

    pub fn call_allowed_effects(&self) -> Vec<Effect> {
        let mut capabilities = Vec::new();

        if self.contains(Effect::Transacts) {
            push_effect(&mut capabilities, Effect::Transacts);
            push_effect(&mut capabilities, Effect::Varies);
            push_effect(&mut capabilities, Effect::Reads);
            push_effect(&mut capabilities, Effect::Writes);
            push_effect(&mut capabilities, Effect::Allocates);
            push_effect(&mut capabilities, Effect::Computes);
            push_effect(&mut capabilities, Effect::Converges);
        }
        if self.contains(Effect::Varies) {
            push_effect(&mut capabilities, Effect::Varies);
            push_effect(&mut capabilities, Effect::Computes);
            push_effect(&mut capabilities, Effect::Converges);
        }
        if self.contains(Effect::Computes) {
            push_effect(&mut capabilities, Effect::Computes);
            push_effect(&mut capabilities, Effect::Converges);
        }
        if self.contains(Effect::Converges) {
            push_effect(&mut capabilities, Effect::Converges);
        }
        if self.contains(Effect::Reads) {
            push_effect(&mut capabilities, Effect::Reads);
            push_effect(&mut capabilities, Effect::Computes);
            push_effect(&mut capabilities, Effect::Converges);
        }
        if self.contains(Effect::Writes) {
            push_effect(&mut capabilities, Effect::Writes);
            push_effect(&mut capabilities, Effect::Computes);
            push_effect(&mut capabilities, Effect::Converges);
        }
        if self.contains(Effect::Allocates) {
            push_effect(&mut capabilities, Effect::Allocates);
            push_effect(&mut capabilities, Effect::Computes);
            push_effect(&mut capabilities, Effect::Converges);
        }

        capabilities
    }

    pub fn call_required_effects(&self) -> Vec<Effect> {
        let mut capabilities = Vec::new();

        if self.contains(Effect::Transacts) {
            push_effect(&mut capabilities, Effect::Transacts);
        } else if self.contains(Effect::Varies) {
            push_effect(&mut capabilities, Effect::Varies);
        } else if self.contains(Effect::Computes) {
            push_effect(&mut capabilities, Effect::Computes);
        } else if self.contains(Effect::Converges) {
            push_effect(&mut capabilities, Effect::Converges);
        }
        if self.contains(Effect::Reads) {
            push_effect(&mut capabilities, Effect::Reads);
        }
        if self.contains(Effect::Writes) {
            push_effect(&mut capabilities, Effect::Writes);
        }
        if self.contains(Effect::Allocates) {
            push_effect(&mut capabilities, Effect::Allocates);
        }

        capabilities
    }

    pub fn assignable_from(&self, actual: &Self) -> bool {
        if self.contains(Effect::Decides) != actual.contains(Effect::Decides) {
            return false;
        }

        let expected = self.assignment_capabilities();
        let actual = actual.assignment_capabilities();
        actual
            .iter()
            .all(|capability| expected.iter().any(|expected| expected == capability))
    }

    pub fn render_declared(&self) -> String {
        let rendered = self
            .effects
            .iter()
            .copied()
            .filter(|effect| effect.is_declared_function_effect())
            .map(|effect| format!("<{}>", effect.name()))
            .collect::<Vec<_>>();
        if rendered.is_empty() {
            "<no_rollback>".to_string()
        } else {
            rendered.join("")
        }
    }

    fn assignment_capabilities(&self) -> Vec<Effect> {
        let mut capabilities = Vec::new();

        if self.has_no_rollback() {
            push_effect(&mut capabilities, Effect::NoRollback);
        }
        if self.contains(Effect::Transacts) {
            push_effect(&mut capabilities, Effect::Transacts);
            push_effect(&mut capabilities, Effect::Varies);
            push_effect(&mut capabilities, Effect::Computes);
            push_effect(&mut capabilities, Effect::Converges);
            push_effect(&mut capabilities, Effect::Allocates);
            push_effect(&mut capabilities, Effect::Reads);
            push_effect(&mut capabilities, Effect::Writes);
        }
        if self.contains(Effect::Varies) {
            push_effect(&mut capabilities, Effect::Varies);
            push_effect(&mut capabilities, Effect::Computes);
            push_effect(&mut capabilities, Effect::Converges);
        }
        if self.contains(Effect::Computes) {
            push_effect(&mut capabilities, Effect::Computes);
            push_effect(&mut capabilities, Effect::Converges);
        }
        if self.contains(Effect::Converges) {
            push_effect(&mut capabilities, Effect::Converges);
        }
        if self.contains(Effect::Reads) {
            push_effect(&mut capabilities, Effect::Reads);
        }
        if self.contains(Effect::Writes) {
            push_effect(&mut capabilities, Effect::Writes);
        }
        if self.contains(Effect::Allocates) {
            push_effect(&mut capabilities, Effect::Allocates);
        }
        if self.contains(Effect::Suspends) {
            push_effect(&mut capabilities, Effect::Suspends);
        }

        capabilities
    }
}

pub(super) fn has_effect(effects: &[String], name: &str) -> bool {
    effects.iter().any(|effect| effect == name)
}

pub(super) fn ensure_callable_in_failure_context(
    effects: &[String],
    span: Span,
) -> Result<(), VerseError> {
    if has_no_rollback_effect(effects) {
        return Err(VerseError::check_at(
            "function with `<no_rollback>` effect cannot be called in a failure context",
            span,
        ));
    }

    Ok(())
}

pub(super) fn has_no_rollback_effect(effects: &[String]) -> bool {
    EffectSet::from_effect_names(effects).has_no_rollback()
}

pub(super) fn has_explicit_call_effect_specifier(effects: &[String]) -> bool {
    EffectSet::from_effect_names(effects).has_explicit_call_effect()
}

pub(super) fn call_allowed_capabilities(effects: &[String]) -> Vec<&'static str> {
    EffectSet::from_effect_names(effects)
        .call_allowed_effects()
        .into_iter()
        .map(Effect::name)
        .collect()
}

pub(super) fn call_required_capabilities(effects: &[String]) -> Vec<&'static str> {
    EffectSet::from_effect_names(effects)
        .call_required_effects()
        .into_iter()
        .map(Effect::name)
        .collect()
}

pub(super) fn effect_call_error(
    caller_effects: &[String],
    required: &str,
    span: Span,
) -> VerseError {
    VerseError::check_at(
        format!(
            "function with {} effect cannot call function requiring <{}> effect",
            render_effect_set(caller_effects),
            required
        ),
        span,
    )
}

pub(super) fn render_effect_set(effects: &[String]) -> String {
    EffectSet::from_effect_names(effects).render_declared()
}

pub(super) fn function_effects_are_assignable(expected: &[String], actual: &[String]) -> bool {
    EffectSet::from_effect_names(expected).assignable_from(&EffectSet::from_effect_names(actual))
}

fn push_effect(effects: &mut Vec<Effect>, effect: Effect) {
    if !effects.iter().any(|existing| *existing == effect) {
        effects.push(effect);
    }
}

pub(super) fn validate_function_effect_combination(
    effects: &[String],
    span: Span,
) -> Result<(), VerseError> {
    let mut seen = Vec::new();
    for effect in effects
        .iter()
        .filter_map(|effect| Effect::from_name(effect))
    {
        if !effect.is_declared_function_effect() {
            continue;
        }
        if seen.iter().any(|seen_effect| *seen_effect == effect) {
            return Err(VerseError::check_at(
                format!("duplicate function effect `<{}>`", effect.name()),
                span,
            ));
        }
        seen.push(effect);
    }

    let effect_set = EffectSet::from_effect_names(effects);
    if effect_set.contains(Effect::Decides) && !effect_set.contains(Effect::Transacts) {
        return Err(VerseError::check_at(
            "function with `<decides>` must also have `<transacts>`",
            span,
        ));
    }

    if has_effect(effects, "constructor") && effect_set.contains(Effect::Suspends) {
        return Err(VerseError::check_at(
            "constructor functions cannot use `<suspends>`",
            span,
        ));
    }

    let exclusive = [
        Effect::Transacts,
        Effect::Varies,
        Effect::Computes,
        Effect::Converges,
    ]
    .into_iter()
    .filter(|effect| effect_set.contains(*effect))
    .collect::<Vec<_>>();
    if exclusive.len() > 1 {
        return Err(VerseError::check_at(
            format!(
                "function exclusive effects cannot be combined: {}",
                exclusive
                    .into_iter()
                    .map(|effect| format!("<{}>", effect.name()))
                    .collect::<Vec<_>>()
                    .join("")
            ),
            span,
        ));
    }

    Ok(())
}
