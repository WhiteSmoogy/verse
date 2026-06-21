use std::rc::Rc;

#[derive(Clone)]
pub struct RuntimeSuspension;

pub struct RuntimeTask;

impl RuntimeTask {
    pub(super) fn new_running() -> Rc<Self> {
        Rc::new(Self)
    }
}
