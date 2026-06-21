use std::rc::Rc;

use crate::ast::TypeName;

use super::Env;

#[derive(Clone)]
pub struct RuntimeSuspension;

pub struct RuntimeTask;

impl RuntimeTask {
    pub(super) fn new_running() -> Rc<Self> {
        Rc::new(Self)
    }

    pub(super) fn matches_payload_type(&self, _payload: &TypeName, _env: &Env) -> bool {
        true
    }
}
