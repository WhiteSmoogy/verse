use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::ast::{Expr, ExprKind, Param, TypeName, TypeParam, TypeParamConstraint};
use crate::error::VerseError;
use crate::eval::{
    Env, RationalValue, RuntimeClassInstanceField, RuntimeClassTypeInfo, RuntimeModifierEntry,
    RuntimeSubscriptionEntry, RuntimeTask, Value, bytecode_call_native_array_method,
    bytecode_call_native_cancel_method, bytecode_call_native_event_method,
    bytecode_call_native_function_named, bytecode_call_native_subscribable_method,
    bytecode_call_native_subscription_cancel_method, bytecode_class_instance_value,
    bytecode_class_type_value, bytecode_color_add_values, bytecode_color_divide_values,
    bytecode_color_multiply_or_scale_values, bytecode_color_subtract_values,
    bytecode_event_signal_payload, bytecode_external_return_value, bytecode_external_value,
    bytecode_interface_type_value, bytecode_load_field_value, bytecode_modifier_stack_add,
    bytecode_modifier_stack_ordered_modifiers, bytecode_native_array_method_value,
    bytecode_native_function_value, bytecode_native_member_value, bytecode_new_running_task,
    bytecode_struct_type_value, rational_or_int, register_runtime_class_types,
    replace_string_byte_failable,
};
use crate::ir::bytecode::{
    BytecodeChunk, BytecodeProgram, ClassDescriptor, ClassMethodDescriptor, Constant,
    FieldDescriptor, Instruction, InterfaceDescriptor, ObjectKind, RegisterIndex, ValueOperand,
};
use crate::native::{InjectedNativeFunction, NativeCallResult, NativeRegistry};
use crate::runtime::host::{Host, MockHost, PendingToken};
use crate::token::{CharacterKind, Span};

#[derive(Clone)]
enum VmValue {
    Runtime(Value),
    Function(VmFunction),
    BoundMethod(VmBoundMethod),
    NumberMethod(VmNumberMethod),
    Scope(Rc<Vec<VmValue>>),
    Option(Option<Box<VmValue>>),
    Ref(Rc<RefCell<VmValue>>),
    FieldRef(VmFieldRef),
    Semaphore(Rc<RefCell<VmSemaphore>>),
    Placeholder(usize),
    Uninitialized,
}

enum RuntimeCastTarget {
    Class(String),
    Interface(String),
}

#[derive(Clone)]
struct VmFunction {
    function: usize,
    captures: Option<Rc<Vec<VmValue>>>,
}

#[derive(Clone)]
struct VmBoundMethod {
    name: String,
    candidates: Vec<VmBoundMethodCandidate>,
    self_value: Value,
    fields: Rc<RefCell<Vec<RuntimeClassInstanceField>>>,
}

#[derive(Clone)]
struct VmBoundMethodCandidate {
    owner_type_params: Vec<String>,
    function: usize,
    params: Vec<String>,
    param_types: Vec<Option<TypeName>>,
    external_return_type: Option<TypeName>,
    field_count: usize,
    decides: bool,
}

#[derive(Clone)]
struct VmNumberMethod {
    name: &'static str,
    receiver: Value,
}

#[derive(Clone)]
struct VmFieldRef {
    fields: Rc<RefCell<Vec<RuntimeClassInstanceField>>>,
    index: usize,
}

#[derive(Clone)]
struct VmSemaphore {
    count: i32,
    awaiter: Option<Rc<RuntimeTask>>,
}

struct VmPlaceholder {
    value: Option<VmValue>,
    waiters: Vec<usize>,
}

impl VmPlaceholder {
    fn new() -> Self {
        Self {
            value: None,
            waiters: Vec::new(),
        }
    }
}

impl VmSemaphore {
    fn new() -> Self {
        Self {
            count: 0,
            awaiter: None,
        }
    }

    fn decrement_count(&mut self, count: i32) -> i32 {
        self.count -= count;
        self.count
    }

    fn increment_count(&mut self, count: i32) -> Option<Rc<RuntimeTask>> {
        self.count += count;
        (self.count == 0).then(|| self.awaiter.take()).flatten()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum TaskPhase {
    Active,
    CancelRequested,
    CancelStarted,
    CancelUnwind,
    Canceled,
}

impl TaskPhase {
    fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }
}

#[derive(Clone)]
struct VmTaskState<'program> {
    handle: Rc<RuntimeTask>,
    running: bool,
    phase: TaskPhase,
    result: Option<Value>,
    yield_pc: Option<usize>,
    root_frame: Option<Frame<'program>>,
    parent: Option<usize>,
    children: Vec<usize>,
    awaits: Vec<usize>,
    cancels: Vec<usize>,
    joins: Vec<usize>,
    native_defers: Vec<VmFunction>,
    native_defer_scopes: Vec<usize>,
    task_group: Option<usize>,
    suspension: Option<BytecodeSuspension<'program>>,
    result_placeholder: usize,
    pending_token: Option<PendingToken>,
}

impl<'program> VmTaskState<'program> {
    fn new(handle: Rc<RuntimeTask>, parent: Option<usize>, result_placeholder: usize) -> Self {
        Self {
            handle,
            running: false,
            phase: TaskPhase::Active,
            result: None,
            yield_pc: None,
            root_frame: None,
            parent,
            children: Vec::new(),
            awaits: Vec::new(),
            cancels: Vec::new(),
            joins: Vec::new(),
            native_defers: Vec::new(),
            native_defer_scopes: Vec::new(),
            task_group: None,
            suspension: None,
            result_placeholder,
            pending_token: None,
        }
    }

    fn active(&self) -> bool {
        self.phase.is_active() && self.result.is_none()
    }
}

#[derive(Default)]
struct VmTaskGroup {
    active_tasks: HashSet<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VmJoinKind {
    Sync,
    Race,
    Rush,
}

struct VmJoin {
    kind: VmJoinKind,
    tasks: Vec<(usize, usize)>,
    placeholder: usize,
    completed: bool,
}

#[derive(Clone)]
struct VmSubscriptionCallback {
    id: u64,
    arity: usize,
    callback: VmValue,
}

enum HostWake {
    Task(usize),
    Subscribers(Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>),
}

enum OpResult<'program> {
    Block(VmValue),
    Yield(BytecodeSuspension<'program>),
}

enum VmRef {
    Local(Rc<RefCell<VmValue>>),
    Field(VmFieldRef),
}

impl VmRef {
    fn get(&self, span: Span) -> Result<VmValue, VerseError> {
        match self {
            VmRef::Local(value) => Ok(value.borrow().clone()),
            VmRef::Field(field_ref) => {
                let fields = field_ref.fields.borrow();
                let Some(field) = fields.get(field_ref.index) else {
                    return Err(VerseError::runtime_at(
                        "class field ref index out of range",
                        span,
                    ));
                };
                Ok(VmValue::Runtime(copy_runtime_value(&field.value)))
            }
        }
    }

    fn set(&self, value: VmValue, span: Span) -> Result<(), VerseError> {
        match self {
            VmRef::Local(ref_value) => {
                *ref_value.borrow_mut() = value;
                Ok(())
            }
            VmRef::Field(field_ref) => {
                let value = into_runtime(value, span)?;
                let mut fields = field_ref.fields.borrow_mut();
                let Some(field) = fields.get_mut(field_ref.index) else {
                    return Err(VerseError::runtime_at(
                        "class field ref index out of range",
                        span,
                    ));
                };
                field.value = value;
                Ok(())
            }
        }
    }
}

#[derive(Clone)]
struct Frame<'program> {
    chunk: &'program BytecodeChunk,
    registers: Vec<VmValue>,
    ip: usize,
    failure_stack: Vec<FailureContext>,
}

impl<'program> Frame<'program> {
    fn new(chunk: &'program BytecodeChunk) -> Self {
        Self {
            chunk,
            registers: vec![VmValue::Uninitialized; chunk.register_count()],
            ip: 0,
            failure_stack: Vec::new(),
        }
    }

    fn set_register(&mut self, register: RegisterIndex, value: VmValue) -> Result<(), VerseError> {
        let Some(slot) = self.registers.get_mut(register.index()) else {
            return Err(VerseError::runtime_at(
                format!("bytecode register {} out of range", register.index()),
                Span::new(0, 0, 1, 1),
            ));
        };
        *slot = value;
        Ok(())
    }
}

#[derive(Clone)]
struct FailureContext {
    on_failure: usize,
    transaction: VmTransaction,
}

type RefSnapshot = (Rc<RefCell<VmValue>>, VmValue);
type ArraySnapshot = (Rc<RefCell<Vec<Value>>>, Vec<Value>);
type MapSnapshot = (Rc<RefCell<Vec<(Value, Value)>>>, Vec<(Value, Value)>);
type ModifierEntriesSnapshot = (
    Rc<RefCell<Vec<RuntimeModifierEntry>>>,
    Vec<RuntimeModifierEntry>,
);
type SubscriptionEntriesSnapshot = (
    Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>,
    Vec<RuntimeSubscriptionEntry>,
);
type ClassFieldsSnapshot = (
    Rc<RefCell<Vec<RuntimeClassInstanceField>>>,
    Vec<RuntimeClassInstanceField>,
);

#[derive(Clone)]
struct VmTransaction {
    registers: Vec<VmValue>,
    refs: Vec<RefSnapshot>,
    arrays: Vec<ArraySnapshot>,
    maps: Vec<MapSnapshot>,
    modifier_entries: Vec<ModifierEntriesSnapshot>,
    subscription_entries: Vec<SubscriptionEntriesSnapshot>,
    class_fields: Vec<ClassFieldsSnapshot>,
}

#[derive(Clone)]
enum BytecodeSuspension<'program> {
    AwaitCall {
        frame: Frame<'program>,
        dest: RegisterIndex,
    },
    Yield {
        frame: Frame<'program>,
    },
    Call {
        parent: Frame<'program>,
        dest: RegisterIndex,
        child: Box<BytecodeSuspension<'program>>,
    },
}

impl VmTransaction {
    fn capture(registers: &[VmValue], globals: &HashMap<String, Rc<RefCell<VmValue>>>) -> Self {
        let mut collector = VmTransactionCollector::new();
        for value in registers {
            collector.collect_vm_value(value);
        }
        for value in globals.values() {
            collector.collect_vm_value(&VmValue::Ref(value.clone()));
        }
        Self {
            registers: registers.to_vec(),
            refs: collector.refs,
            arrays: collector.arrays,
            maps: collector.maps,
            modifier_entries: collector.modifier_entries,
            subscription_entries: collector.subscription_entries,
            class_fields: collector.class_fields,
        }
    }

    fn restore(self, frame: &mut Frame<'_>) {
        frame.registers = self.registers;
        for (ref_value, snapshot) in self.refs {
            *ref_value.borrow_mut() = snapshot;
        }
        for (items, snapshot) in self.arrays {
            *items.borrow_mut() = snapshot;
        }
        for (entries, snapshot) in self.maps {
            *entries.borrow_mut() = snapshot;
        }
        for (entries, snapshot) in self.modifier_entries {
            *entries.borrow_mut() = snapshot;
        }
        for (entries, snapshot) in self.subscription_entries {
            *entries.borrow_mut() = snapshot;
        }
        for (fields, snapshot) in self.class_fields {
            *fields.borrow_mut() = snapshot;
        }
    }
}

struct VmTransactionCollector {
    seen_refs: HashSet<usize>,
    seen_arrays: HashSet<usize>,
    seen_maps: HashSet<usize>,
    seen_modifier_entries: HashSet<usize>,
    seen_subscription_entries: HashSet<usize>,
    seen_class_fields: HashSet<usize>,
    refs: Vec<RefSnapshot>,
    arrays: Vec<ArraySnapshot>,
    maps: Vec<MapSnapshot>,
    modifier_entries: Vec<ModifierEntriesSnapshot>,
    subscription_entries: Vec<SubscriptionEntriesSnapshot>,
    class_fields: Vec<ClassFieldsSnapshot>,
}

impl VmTransactionCollector {
    fn new() -> Self {
        Self {
            seen_refs: HashSet::new(),
            seen_arrays: HashSet::new(),
            seen_maps: HashSet::new(),
            seen_modifier_entries: HashSet::new(),
            seen_subscription_entries: HashSet::new(),
            seen_class_fields: HashSet::new(),
            refs: Vec::new(),
            arrays: Vec::new(),
            maps: Vec::new(),
            modifier_entries: Vec::new(),
            subscription_entries: Vec::new(),
            class_fields: Vec::new(),
        }
    }

    fn collect_vm_value(&mut self, value: &VmValue) {
        match value {
            VmValue::Runtime(value) => self.collect_runtime_value(value),
            VmValue::Ref(ref_value) => {
                let id = Rc::as_ptr(ref_value) as usize;
                if self.seen_refs.insert(id) {
                    let snapshot = ref_value.borrow().clone();
                    self.collect_vm_value(&snapshot);
                    self.refs.push((ref_value.clone(), snapshot));
                }
            }
            VmValue::FieldRef(field_ref) => self.collect_class_fields(&field_ref.fields),
            VmValue::Semaphore(_) | VmValue::Placeholder(_) => {}
            VmValue::NumberMethod(method) => self.collect_runtime_value(&method.receiver),
            VmValue::BoundMethod(method) => {
                self.collect_runtime_value(&method.self_value);
                self.collect_class_fields(&method.fields);
            }
            VmValue::Scope(values) => {
                for value in values.iter() {
                    self.collect_vm_value(value);
                }
            }
            VmValue::Option(Some(value)) => self.collect_vm_value(value),
            VmValue::Option(None) => {}
            VmValue::Function(_) | VmValue::Uninitialized => {}
        }
    }

    fn collect_runtime_value(&mut self, value: &Value) {
        match value {
            Value::Array(items) => {
                let id = Rc::as_ptr(items) as usize;
                if self.seen_arrays.insert(id) {
                    let snapshot = items.borrow().clone();
                    for item in &snapshot {
                        self.collect_runtime_value(item);
                    }
                    self.arrays.push((items.clone(), snapshot));
                }
            }
            Value::Map(entries) => {
                let id = Rc::as_ptr(entries) as usize;
                if self.seen_maps.insert(id) {
                    let snapshot = entries.borrow().clone();
                    for (key, value) in &snapshot {
                        self.collect_runtime_value(key);
                        self.collect_runtime_value(value);
                    }
                    self.maps.push((entries.clone(), snapshot));
                }
            }
            Value::Tuple(items) => {
                for item in items {
                    self.collect_runtime_value(item);
                }
            }
            Value::Option(Some(value)) | Value::Result { value, .. } => {
                self.collect_runtime_value(value);
            }
            Value::ClassInstance { fields, .. } => self.collect_class_fields(fields),
            Value::ModifierStack { entries, .. } | Value::ModifierCancelHandle { entries, .. } => {
                self.collect_modifier_entries(entries);
            }
            Value::SubscribableEventIntrnl { subscribers, .. }
            | Value::SubscribableEvent { subscribers, .. }
            | Value::Subscribable { subscribers, .. }
            | Value::Listenable { subscribers, .. }
            | Value::SubscriptionCancelHandle { subscribers, .. } => {
                self.collect_subscription_entries(subscribers);
            }
            _ => {}
        }
    }

    fn collect_modifier_entries(&mut self, entries: &Rc<RefCell<Vec<RuntimeModifierEntry>>>) {
        let id = Rc::as_ptr(entries) as usize;
        if !self.seen_modifier_entries.insert(id) {
            return;
        }
        let snapshot = entries.borrow().clone();
        self.modifier_entries.push((entries.clone(), snapshot));
    }

    fn collect_subscription_entries(
        &mut self,
        entries: &Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>,
    ) {
        let id = Rc::as_ptr(entries) as usize;
        if !self.seen_subscription_entries.insert(id) {
            return;
        }
        let snapshot = entries.borrow().clone();
        self.subscription_entries.push((entries.clone(), snapshot));
    }

    fn collect_class_fields(&mut self, fields: &Rc<RefCell<Vec<RuntimeClassInstanceField>>>) {
        let id = Rc::as_ptr(fields) as usize;
        if !self.seen_class_fields.insert(id) {
            return;
        }
        let snapshot = fields.borrow().clone();
        for field in &snapshot {
            self.collect_runtime_value(&field.value);
        }
        self.class_fields.push((fields.clone(), snapshot));
    }
}

pub(crate) struct BytecodeExecutor<'program, H: Host = MockHost> {
    program: &'program BytecodeProgram,
    host: H,
    native_registry: NativeRegistry,
    globals: RefCell<HashMap<String, Rc<RefCell<VmValue>>>>,
    tasks: HashMap<usize, VmTaskState<'program>>,
    task_groups: Vec<VmTaskGroup>,
    joins: Vec<VmJoin>,
    entry_native_defers: Vec<VmFunction>,
    entry_native_defer_scopes: Vec<usize>,
    placeholders: HashMap<usize, VmPlaceholder>,
    next_placeholder: usize,
    ready_tasks: VecDeque<(usize, Value)>,
    pending_wakes: HashMap<PendingToken, HostWake>,
    subscription_callbacks: HashMap<usize, Vec<VmSubscriptionCallback>>,
    subscription_signal_tokens: HashMap<usize, PendingToken>,
    next_pending_token: u64,
}

impl<'program> BytecodeExecutor<'program, MockHost> {
    #[allow(dead_code)]
    pub(crate) fn new(program: &'program BytecodeProgram) -> Self {
        Self::with_host(program, MockHost::default())
    }

    pub(crate) fn with_native_registry(
        program: &'program BytecodeProgram,
        native_registry: NativeRegistry,
    ) -> Self {
        Self::with_host_and_native_registry(program, MockHost::default(), native_registry)
    }

    #[cfg(test)]
    fn host_poll_count(&self) -> usize {
        self.host.poll_count()
    }

    #[cfg(test)]
    fn host_has_pending(&self) -> bool {
        self.host.has_pending()
    }
}

fn runtime_class_type_info_from_descriptor(class: &ClassDescriptor) -> RuntimeClassTypeInfo {
    RuntimeClassTypeInfo {
        name: class.name().to_string(),
        base: class.base_class().map(str::to_string),
        interfaces: class.interfaces().to_vec(),
        unique: class.unique(),
        abstract_class: class.abstract_class(),
        epic_internal_class: class.epic_internal_class(),
        final_class: class.final_class(),
        final_super: class.final_super(),
        concrete: class.concrete(),
        castable: class.castable(),
    }
}

impl<'program, H: Host> BytecodeExecutor<'program, H> {
    #[allow(dead_code)]
    pub(crate) fn with_host(program: &'program BytecodeProgram, host: H) -> Self {
        Self::with_host_and_native_registry(program, host, NativeRegistry::new())
    }

    pub(crate) fn with_host_and_native_registry(
        program: &'program BytecodeProgram,
        host: H,
        native_registry: NativeRegistry,
    ) -> Self {
        register_runtime_class_types(
            program
                .classes()
                .iter()
                .map(runtime_class_type_info_from_descriptor),
        );
        Self {
            program,
            host,
            native_registry,
            globals: RefCell::new(HashMap::new()),
            tasks: HashMap::new(),
            task_groups: Vec::new(),
            joins: Vec::new(),
            entry_native_defers: Vec::new(),
            entry_native_defer_scopes: Vec::new(),
            placeholders: HashMap::new(),
            next_placeholder: 0,
            ready_tasks: VecDeque::new(),
            pending_wakes: HashMap::new(),
            subscription_callbacks: HashMap::new(),
            subscription_signal_tokens: HashMap::new(),
            next_pending_token: 0,
        }
    }

    pub(crate) fn run(&mut self) -> Result<Value, VerseError> {
        match self.run_chunk(
            self.program.entry(),
            Vec::new(),
            Span::new(0, 0, 1, 1),
            None,
            None,
        )? {
            ChunkOutcome::Value(value) => into_runtime(value, Span::new(0, 0, 1, 1)),
            ChunkOutcome::Failure => Err(VerseError::runtime_at(
                "bytecode failure escaped the entry chunk",
                Span::new(0, 0, 1, 1),
            )),
            ChunkOutcome::Suspended(_) => Err(VerseError::runtime_at(
                "bytecode entry chunk suspended",
                Span::new(0, 0, 1, 1),
            )),
        }
    }

    #[cfg(test)]
    fn host_now(&self) -> f64 {
        self.host.now().as_secs_f64()
    }

    fn class_instance_value(
        &self,
        class_name: String,
        unique: bool,
        fields: Vec<(String, bool, Value)>,
    ) -> Value {
        let descriptors = self
            .program_class_by_runtime_name(&class_name)
            .map(|class| {
                class
                    .fields()
                    .iter()
                    .map(|field| {
                        (
                            field.name().to_string(),
                            (field.predicts(), field.predicts_extern()),
                        )
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        let fields = fields
            .into_iter()
            .map(|(name, mutable, value)| {
                let (predicts, predicts_extern) =
                    descriptors.get(&name).copied().unwrap_or((false, false));
                RuntimeClassInstanceField {
                    owner_class: class_name.clone(),
                    name,
                    mutable,
                    predicts,
                    predicts_extern,
                    value,
                }
            })
            .collect();
        Value::ClassInstance {
            class_name,
            unique,
            fields: Rc::new(RefCell::new(fields)),
            methods: Rc::new(Vec::new()),
        }
    }

    fn run_chunk(
        &mut self,
        chunk_index: usize,
        args: Vec<VmValue>,
        span: Span,
        current_task: Option<Rc<RuntimeTask>>,
        captures: Option<Rc<Vec<VmValue>>>,
    ) -> Result<ChunkOutcome<'program>, VerseError> {
        let chunk = self
            .program
            .chunks()
            .get(chunk_index)
            .ok_or_else(|| VerseError::runtime_at("bytecode chunk index out of range", span))?;
        let mut frame = Frame::new(chunk);
        if let Some(captures) = captures {
            frame.set_register(RegisterIndex::SCOPE, VmValue::Scope(captures))?;
        }
        for (index, arg) in args.into_iter().enumerate() {
            frame.set_register(RegisterIndex(RegisterIndex::PARAMETER_START.0 + index), arg)?;
        }

        self.run_frame(frame, current_task)
    }

    fn run_frame(
        &mut self,
        mut frame: Frame<'program>,
        current_task: Option<Rc<RuntimeTask>>,
    ) -> Result<ChunkOutcome<'program>, VerseError> {
        while let Some(instruction) = frame.chunk.instructions().get(frame.ip).cloned() {
            let mut tick_entry_after_instruction = false;
            match instruction {
                Instruction::Move { dest, source, span }
                | Instruction::MoveTrailed { dest, source, span }
                | Instruction::MoveNonComparable { dest, source, span } => {
                    let value = self.get_operand(&frame, source, span)?;
                    frame.set_register(dest, value)?;
                }
                Instruction::Reset { dest, .. } | Instruction::ResetNonTrailed { dest, .. } => {
                    frame.set_register(dest, VmValue::Uninitialized)?;
                }
                Instruction::Jump { jump_offset } => {
                    frame.ip = jump_offset;
                    continue;
                }
                Instruction::JumpIfInitialized {
                    source,
                    jump_offset,
                    span,
                } => {
                    if !matches!(
                        self.get_operand(&frame, source, span)?,
                        VmValue::Uninitialized
                    ) {
                        frame.ip = jump_offset;
                        continue;
                    }
                }
                Instruction::Switch {
                    which,
                    jump_offsets,
                    span,
                } => {
                    let which = expect_int(
                        self.get_runtime_operand(&frame, which, span)?,
                        "Switch selector",
                        span,
                    )?;
                    let jump_offset = usize::try_from(which)
                        .ok()
                        .and_then(|index| jump_offsets.get(index).copied())
                        .ok_or_else(|| {
                            VerseError::runtime_at("Switch selector out of range", span)
                        })?;
                    frame.ip = jump_offset;
                    continue;
                }
                Instruction::Err { span } => {
                    return Err(VerseError::runtime_at("bytecode Err instruction", span));
                }
                Instruction::Tracepoint { .. } => {}
                Instruction::BeginFailureContext { on_failure, .. } => {
                    let transaction = {
                        let globals = self.globals.borrow();
                        VmTransaction::capture(&frame.registers, &globals)
                    };
                    frame.failure_stack.push(FailureContext {
                        on_failure,
                        transaction,
                    });
                }
                Instruction::EndFailureContext { .. } => {
                    frame.failure_stack.pop();
                }
                Instruction::EndFastFailureContext { .. } => {}
                Instruction::BeginTask {
                    dest,
                    parent,
                    add_to_task_group,
                    on_yield,
                    span,
                } => {
                    let task = if let Some(task) = current_task.clone() {
                        task
                    } else {
                        bytecode_new_running_task()
                    };
                    let id = task_id(&task);
                    let parent_id = self
                        .task_from_operand(&frame, parent, span)?
                        .as_ref()
                        .map(task_id);
                    self.ensure_vm_task(task.clone(), parent_id);
                    if let Some(state) = self.tasks.get_mut(&id) {
                        state.running = true;
                        state.yield_pc = Some(on_yield);
                        state.root_frame.get_or_insert_with(|| frame.clone());
                        if add_to_task_group {
                            self.add_task_to_current_group(id);
                        }
                    }
                    frame.set_register(dest, VmValue::Runtime(Value::Task(task)))?;
                }
                Instruction::BeginAwait { .. }
                | Instruction::AwaitSuccess { .. }
                | Instruction::EndAwait { .. }
                | Instruction::BeginBatch { .. }
                | Instruction::EndBatch { .. } => {}
                Instruction::EndTask {
                    write,
                    switch,
                    value,
                    which,
                    signal,
                    span,
                } => {
                    let result = self.get_operand(&frame, value, span)?;
                    if let Some(write) = write
                        && matches!(
                            frame.registers.get(write.index()),
                            Some(VmValue::Uninitialized)
                        )
                    {
                        frame.set_register(write, result.clone())?;
                        if let Some(switch) = switch
                            && matches!(
                                frame.registers.get(switch.index()),
                                Some(VmValue::Uninitialized)
                            )
                        {
                            let which = self.get_operand(&frame, which, span)?;
                            frame.set_register(switch, which)?;
                        }
                    }
                    if let Some(signal) = signal {
                        self.signal_semaphore(&frame, signal, span)?;
                    }
                    if let Some(task) = current_task.as_ref() {
                        let id = task_id(task);
                        if let Some(state) = self.tasks.get_mut(&id) {
                            state.result = Some(into_runtime(result.clone(), span)?);
                            state.running = false;
                        }
                    }
                    return Ok(ChunkOutcome::Value(result));
                }
                Instruction::Yield { resume_offset, .. } => {
                    frame.ip = resume_offset;
                    return Self::chunk_outcome_from_op_result(OpResult::Yield(
                        BytecodeSuspension::Yield { frame },
                    ));
                }
                Instruction::NewSemaphore { dest, .. } => {
                    frame.set_register(
                        dest,
                        VmValue::Semaphore(Rc::new(RefCell::new(VmSemaphore::new()))),
                    )?;
                }
                Instruction::WaitSemaphore {
                    source,
                    count,
                    span,
                } => {
                    let semaphore = self.get_semaphore_operand(&frame, source, span)?;
                    if semaphore.borrow_mut().decrement_count(count) < 0 {
                        let Some(task) = current_task.clone() else {
                            return Err(VerseError::runtime_at(
                                "WaitSemaphore requires a current task",
                                span,
                            ));
                        };
                        if semaphore.borrow().awaiter.is_some() {
                            return Err(VerseError::runtime_at(
                                "WaitSemaphore already has an awaiter",
                                span,
                            ));
                        }
                        semaphore.borrow_mut().awaiter = Some(task);
                        frame.ip += 1;
                        return Ok(ChunkOutcome::Suspended(BytecodeSuspension::Yield { frame }));
                    }
                }
                Instruction::Add {
                    dest,
                    left_source,
                    right_source,
                    span,
                } => {
                    let left = self.get_runtime_operand(&frame, left_source, span)?;
                    let right = self.get_runtime_operand(&frame, right_source, span)?;
                    frame.set_register(dest, VmValue::Runtime(add_values(left, right, span)?))?;
                }
                Instruction::Sub {
                    dest,
                    left_source,
                    right_source,
                    span,
                } => {
                    let left = self.get_runtime_operand(&frame, left_source, span)?;
                    let right = self.get_runtime_operand(&frame, right_source, span)?;
                    frame.set_register(
                        dest,
                        VmValue::Runtime(subtract_values(left, right, span)?),
                    )?;
                }
                Instruction::Mul {
                    dest,
                    left_source,
                    right_source,
                    span,
                } => {
                    let left = self.get_runtime_operand(&frame, left_source, span)?;
                    let right = self.get_runtime_operand(&frame, right_source, span)?;
                    frame.set_register(
                        dest,
                        VmValue::Runtime(multiply_values(left, right, span)?),
                    )?;
                }
                Instruction::Div {
                    dest,
                    left_source,
                    right_source,
                    span,
                } => {
                    let left = self.get_runtime_operand(&frame, left_source, span)?;
                    let right = self.get_runtime_operand(&frame, right_source, span)?;
                    if numeric_is_zero(&right) {
                        if Self::fail_current_context(&mut frame) {
                            continue;
                        }
                        return Err(VerseError::runtime_at("division by zero", span));
                    }
                    frame
                        .set_register(dest, VmValue::Runtime(divide_values(left, right, span)?))?;
                }
                Instruction::Mod {
                    dest,
                    left_source,
                    right_source,
                    span,
                } => {
                    let left = self.get_runtime_operand(&frame, left_source, span)?;
                    let right = self.get_runtime_operand(&frame, right_source, span)?;
                    if matches!(right, Value::Int(0)) {
                        if Self::fail_current_context(&mut frame) {
                            continue;
                        }
                        return Err(VerseError::runtime_at("remainder by zero", span));
                    }
                    frame.set_register(
                        dest,
                        VmValue::Runtime(remainder_values(left, right, span)?),
                    )?;
                }
                Instruction::CanFastAppendToArrayFastFail {
                    ref_value,
                    maybe_mutable_array,
                    on_failure,
                    span,
                    ..
                } => {
                    let _ = self.get_operand(&frame, ref_value, span)?;
                    let maybe_mutable_array =
                        self.get_runtime_operand(&frame, maybe_mutable_array, span)?;
                    if !matches!(maybe_mutable_array, Value::Array(_)) {
                        Self::jump_to_failure(&mut frame, on_failure);
                        continue;
                    }
                }
                Instruction::FastAppendToArray {
                    left_source,
                    right_source,
                    span,
                } => {
                    let left = self.get_runtime_operand(&frame, left_source, span)?;
                    let right = self.get_runtime_operand(&frame, right_source, span)?;
                    fast_append_to_array(left, right, span)?;
                }
                Instruction::MutableAdd {
                    dest,
                    left_source,
                    right_source,
                    span,
                } => {
                    let left = self.get_runtime_operand(&frame, left_source, span)?;
                    let right = self.get_runtime_operand(&frame, right_source, span)?;
                    frame.set_register(
                        dest,
                        VmValue::Runtime(mutable_add_values(left, right, span)?),
                    )?;
                }
                Instruction::Neg { dest, source, span } => {
                    let value = self.get_runtime_operand(&frame, source, span)?;
                    frame.set_register(dest, VmValue::Runtime(neg_value(value, span)?))?;
                }
                Instruction::Query { dest, source, span } => {
                    let value = self.get_operand(&frame, source, span)?;
                    if let Some(value) = query_vm_value(value) {
                        frame.set_register(dest, value)?;
                    } else if Self::fail_current_context(&mut frame) {
                        continue;
                    } else {
                        return Err(VerseError::runtime_at("query failed", span));
                    }
                }
                Instruction::Neq {
                    dest,
                    left_source,
                    right_source,
                    span,
                }
                | Instruction::Lt {
                    dest,
                    left_source,
                    right_source,
                    span,
                }
                | Instruction::Lte {
                    dest,
                    left_source,
                    right_source,
                    span,
                }
                | Instruction::Gt {
                    dest,
                    left_source,
                    right_source,
                    span,
                }
                | Instruction::Gte {
                    dest,
                    left_source,
                    right_source,
                    span,
                } => {
                    let op = instruction;
                    let left = self.get_runtime_operand(&frame, left_source, span)?;
                    let right = self.get_runtime_operand(&frame, right_source, span)?;
                    if comparison_succeeds(&op, &left, &right, span)? {
                        frame.set_register(dest, VmValue::Runtime(left))?;
                    } else if Self::fail_current_context(&mut frame) {
                        continue;
                    } else {
                        return Err(VerseError::runtime_at("comparison failed", span));
                    }
                }
                Instruction::EqFastFail {
                    dest,
                    lhs,
                    rhs,
                    on_failure,
                    span,
                    ..
                }
                | Instruction::NeqFastFail {
                    dest,
                    lhs,
                    rhs,
                    on_failure,
                    span,
                    ..
                }
                | Instruction::LtFastFail {
                    dest,
                    lhs,
                    rhs,
                    on_failure,
                    span,
                    ..
                }
                | Instruction::LteFastFail {
                    dest,
                    lhs,
                    rhs,
                    on_failure,
                    span,
                    ..
                }
                | Instruction::GtFastFail {
                    dest,
                    lhs,
                    rhs,
                    on_failure,
                    span,
                    ..
                }
                | Instruction::GteFastFail {
                    dest,
                    lhs,
                    rhs,
                    on_failure,
                    span,
                    ..
                } => {
                    let op = instruction;
                    let left = self.get_runtime_operand(&frame, lhs, span)?;
                    let right = self.get_runtime_operand(&frame, rhs, span)?;
                    if fast_comparison_succeeds(&op, &left, &right, span)? {
                        frame.set_register(dest, VmValue::Runtime(left))?;
                    } else {
                        Self::jump_to_failure(&mut frame, on_failure);
                        continue;
                    }
                }
                Instruction::ArrayIndexFastFail {
                    dest,
                    array,
                    index,
                    on_failure,
                    span,
                    ..
                } => {
                    let array = self.get_runtime_operand(&frame, array, span)?;
                    let index = self.get_runtime_operand(&frame, index, span)?;
                    match self.index_value_failable(array, index, span)? {
                        Some(value) => frame.set_register(dest, VmValue::Runtime(value))?,
                        None => {
                            Self::jump_to_failure(&mut frame, on_failure);
                            continue;
                        }
                    }
                }
                Instruction::QueryFastFail {
                    dest,
                    source,
                    on_failure,
                    span,
                    ..
                } => {
                    let source = self.get_operand(&frame, source, span)?;
                    if let Some(value) = query_vm_value(source) {
                        frame.set_register(dest, value)?;
                    } else {
                        Self::jump_to_failure(&mut frame, on_failure);
                        continue;
                    }
                }
                Instruction::Call {
                    dest,
                    callee,
                    arguments,
                    named_arguments,
                    named_argument_values,
                    span,
                    ..
                } => {
                    let callee = self.get_operand(&frame, callee, span)?;
                    let args = arguments
                        .iter()
                        .map(|argument| self.get_operand(&frame, *argument, span))
                        .collect::<Result<Vec<_>, _>>()?;
                    if named_arguments.len() != named_argument_values.len() {
                        return Err(VerseError::runtime_at(
                            "named bytecode call operand count mismatch",
                            span,
                        ));
                    }
                    let named_args = named_arguments
                        .iter()
                        .zip(named_argument_values)
                        .map(|(name, argument)| {
                            self.get_operand(&frame, argument, span)
                                .map(|value| (name.clone(), value))
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    match self.call(callee, args, named_args, span, current_task.clone())? {
                        CallOutcome::Value(value) => frame.set_register(dest, value)?,
                        CallOutcome::Failure => {
                            if Self::fail_current_context(&mut frame) {
                                continue;
                            }
                            return Err(VerseError::runtime_at("bytecode call failed", span));
                        }
                        CallOutcome::Yield => {
                            frame.ip += 1;
                            return Self::chunk_outcome_from_op_result(OpResult::Yield(
                                BytecodeSuspension::AwaitCall { frame, dest },
                            ));
                        }
                        CallOutcome::Block(placeholder) => {
                            let op_result = OpResult::Block(VmValue::Placeholder(placeholder));
                            let OpResult::Block(VmValue::Placeholder(placeholder)) = op_result
                            else {
                                unreachable!("task await block should carry a placeholder");
                            };
                            let Some(task) = current_task.clone() else {
                                return Err(VerseError::runtime_at(
                                    "bytecode Block requires a current task",
                                    span,
                                ));
                            };
                            if let Some(value) = self.block_on_placeholder(placeholder, task) {
                                frame.set_register(dest, value)?;
                            } else {
                                frame.ip += 1;
                                return Self::chunk_outcome_from_op_result(OpResult::Yield(
                                    BytecodeSuspension::AwaitCall { frame, dest },
                                ));
                            }
                        }
                        CallOutcome::Suspended(child) => {
                            frame.ip += 1;
                            return Self::chunk_outcome_from_op_result(OpResult::Yield(
                                BytecodeSuspension::Call {
                                    parent: frame,
                                    dest,
                                    child: Box::new(child),
                                },
                            ));
                        }
                    }
                }
                Instruction::CallTask {
                    dest,
                    parent,
                    callee,
                    arguments,
                    span,
                } => {
                    let callee = self.get_operand(&frame, callee, span)?;
                    let VmValue::Function(function) = callee else {
                        return Err(VerseError::runtime_at(
                            "CallTask expected procedure operand",
                            span,
                        ));
                    };
                    let args = arguments
                        .iter()
                        .map(|argument| self.get_operand(&frame, *argument, span))
                        .collect::<Result<Vec<_>, _>>()?;
                    let parent_task = self
                        .task_from_operand(&frame, parent, span)?
                        .or_else(|| current_task.clone());
                    let task =
                        self.start_bytecode_task(function, args, span, parent_task, false)?;
                    frame.set_register(dest, VmValue::Runtime(Value::Task(task)))?;
                    tick_entry_after_instruction = true;
                }
                Instruction::CallSet {
                    container,
                    index,
                    value_to_set,
                    span,
                } => {
                    let container = self.get_operand(&frame, container, span)?;
                    let index = self.get_runtime_operand(&frame, index, span)?;
                    let value = self.get_runtime_operand(&frame, value_to_set, span)?;
                    if !call_set_vm_value(container, index, value, span)? {
                        if Self::fail_current_context(&mut frame) {
                            continue;
                        }
                        return Err(VerseError::runtime_at("bytecode CallSet failed", span));
                    }
                }
                Instruction::CallSetLive {
                    container,
                    index,
                    value_to_set,
                    task,
                    span,
                } => {
                    let _ = self.get_operand(&frame, task, span)?;
                    let container = self.get_operand(&frame, container, span)?;
                    let index = self.get_runtime_operand(&frame, index, span)?;
                    let value = self.get_runtime_operand(&frame, value_to_set, span)?;
                    if !call_set_vm_value(container, index, value, span)? {
                        if Self::fail_current_context(&mut frame) {
                            continue;
                        }
                        return Err(VerseError::runtime_at("bytecode CallSet failed", span));
                    }
                }
                Instruction::Return { value, span }
                | Instruction::ReturnTrailed { value, span } => {
                    if current_task.is_none() {
                        self.drive_scheduler_until_idle(span)?;
                    }
                    return self
                        .get_operand(&frame, value, span)
                        .map(ChunkOutcome::Value);
                }
                Instruction::NewRef { dest, domain, span } => {
                    if let Some(domain) = domain {
                        let _ = self.get_operand(&frame, domain, span)?;
                    }
                    frame.set_register(
                        dest,
                        VmValue::Ref(Rc::new(RefCell::new(VmValue::Uninitialized))),
                    )?;
                }
                Instruction::RefGet {
                    dest,
                    ref_value,
                    span,
                } => {
                    let ref_value = self.get_ref_operand(&frame, ref_value, span)?;
                    let value = self.get_ref_value(&ref_value, span)?;
                    frame.set_register(dest, value)?;
                }
                Instruction::RefSet {
                    ref_value,
                    value,
                    span,
                } => {
                    let ref_value = self.get_ref_operand(&frame, ref_value, span)?;
                    let value = self.get_operand(&frame, value, span)?;
                    self.set_ref_value(&ref_value, value, span)?;
                    tick_entry_after_instruction = true;
                }
                Instruction::RefSetLive {
                    ref_value,
                    value,
                    task,
                    span,
                } => {
                    let _ = self.get_operand(&frame, task, span)?;
                    let ref_value = self.get_ref_operand(&frame, ref_value, span)?;
                    let value = self.get_operand(&frame, value, span)?;
                    self.set_ref_value(&ref_value, value, span)?;
                    tick_entry_after_instruction = true;
                }
                Instruction::Freeze { dest, value, span }
                | Instruction::Melt { dest, value, span } => {
                    let value = self.get_runtime_operand(&frame, value, span)?;
                    frame.set_register(dest, VmValue::Runtime(copy_runtime_value(&value)))?;
                }
                Instruction::FreezeIfAccessor { dest, value, span } => {
                    let value = self.get_operand(&frame, value, span)?;
                    frame.set_register(dest, value)?;
                }
                Instruction::NewArray { dest, values, span }
                | Instruction::NewMutableArray { dest, values, span } => {
                    let values = self.runtime_values_from_operands(&frame, &values, span)?;
                    frame.set_register(
                        dest,
                        VmValue::Runtime(Value::Array(Rc::new(RefCell::new(values)))),
                    )?;
                }
                Instruction::NewMutableArrayWithCapacity { dest, .. } => {
                    frame.set_register(
                        dest,
                        VmValue::Runtime(Value::Array(Rc::new(RefCell::new(Vec::new())))),
                    )?;
                }
                Instruction::ArrayAdd {
                    dest,
                    container,
                    value_to_add,
                    span,
                } => {
                    let container = self.get_runtime_operand(&frame, container, span)?;
                    let value = self.get_runtime_operand(&frame, value_to_add, span)?;
                    let Value::Array(items) = &container else {
                        return Err(VerseError::runtime_at(
                            format!("ArrayAdd expected array, got {container}"),
                            span,
                        ));
                    };
                    items.borrow_mut().push(value);
                    frame.set_register(dest, VmValue::Runtime(container))?;
                }
                Instruction::InPlaceMakeImmutable {
                    dest,
                    container,
                    span,
                } => {
                    let container = self.get_operand(&frame, container, span)?;
                    frame.set_register(dest, container)?;
                }
                Instruction::Length {
                    dest,
                    container,
                    span,
                } => {
                    let container = self.get_runtime_operand(&frame, container, span)?;
                    frame.set_register(dest, VmValue::Runtime(length_value(container, span)?))?;
                }
                Instruction::LengthWithEffects {
                    dest,
                    container,
                    span,
                } => {
                    let container = match self.get_operand(&frame, container, span)? {
                        VmValue::Ref(ref_value) => into_runtime(ref_value.borrow().clone(), span)?,
                        VmValue::FieldRef(field_ref) => {
                            into_runtime(self.get_ref_value(&VmRef::Field(field_ref), span)?, span)?
                        }
                        value => into_runtime(value, span)?,
                    };
                    frame.set_register(dest, VmValue::Runtime(length_value(container, span)?))?;
                }
                Instruction::NewOption { dest, value, span } => {
                    let value = self.get_operand(&frame, value, span)?;
                    frame.set_register(dest, VmValue::Option(Some(Box::new(value))))?;
                }
                Instruction::NewMap {
                    dest,
                    keys,
                    values,
                    span,
                } => {
                    if keys.len() != values.len() {
                        return Err(VerseError::runtime_at(
                            "NewMap key/value operand count mismatch",
                            span,
                        ));
                    }
                    let keys = self.runtime_values_from_operands(&frame, &keys, span)?;
                    let values = self.runtime_values_from_operands(&frame, &values, span)?;
                    let mut entries = Vec::with_capacity(keys.len());
                    for (key, value) in keys.into_iter().zip(values) {
                        upsert_map_entry(&mut entries, key, value);
                    }
                    frame.set_register(
                        dest,
                        VmValue::Runtime(Value::Map(Rc::new(RefCell::new(entries)))),
                    )?;
                }
                Instruction::NewObject {
                    dest,
                    class_name,
                    object_kind,
                    fields,
                    span,
                } => {
                    let fields = fields
                        .iter()
                        .map(|(name, mutable, value)| {
                            self.get_class_field_operand(&frame, *value, span)
                                .map(|value| (name.clone(), *mutable, value))
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let object = match object_kind {
                        ObjectKind::Class => {
                            let unique = self
                                .program_class_by_runtime_name(&class_name)
                                .is_some_and(|class| class.unique());
                            let object =
                                self.class_instance_value(class_name.clone(), unique, fields);
                            self.run_class_blocks(
                                &object,
                                &class_name,
                                span,
                                current_task.clone(),
                            )?;
                            object
                        }
                        ObjectKind::Struct { computes } => Value::StructInstance {
                            struct_name: class_name,
                            computes,
                            fields: fields
                                .into_iter()
                                .map(|(name, _, value)| (name, value))
                                .collect(),
                        },
                    };
                    frame.set_register(dest, VmValue::Runtime(object))?;
                }
                Instruction::LoadField {
                    dest,
                    object,
                    name,
                    span,
                } => {
                    let object = self.get_runtime_operand(&frame, object, span)?;
                    let value = self.load_field_value(object, &name, span)?;
                    frame.set_register(dest, value)?;
                }
                Instruction::LoadFieldFromSuper {
                    dest,
                    object,
                    base_class,
                    name,
                    span,
                } => {
                    let object = self.get_runtime_operand(&frame, object, span)?;
                    let value = self.load_super_field_value(object, &base_class, &name, span)?;
                    frame.set_register(dest, value)?;
                }
                Instruction::SetField {
                    object,
                    name,
                    value,
                    span,
                } => {
                    let value = self.get_runtime_operand(&frame, value, span)?;
                    self.set_field_operand(&mut frame, object, &name, value, span)?;
                }
                Instruction::MapKey {
                    dest,
                    map,
                    index,
                    span,
                } => {
                    let map = self.get_runtime_operand(&frame, map, span)?;
                    let index = self.get_runtime_operand(&frame, index, span)?;
                    frame.set_register(dest, VmValue::Runtime(map_key_value(map, index, span)?))?;
                }
                Instruction::MapValue {
                    dest,
                    map,
                    index,
                    span,
                } => {
                    let map = self.get_runtime_operand(&frame, map, span)?;
                    let index = self.get_runtime_operand(&frame, index, span)?;
                    frame
                        .set_register(dest, VmValue::Runtime(map_entry_value(map, index, span)?))?;
                }
                Instruction::NewFunction {
                    dest,
                    procedure,
                    parent_scope,
                    span,
                    ..
                } => {
                    let procedure = self.get_operand(&frame, procedure, span)?;
                    let VmValue::Function(mut function) = procedure else {
                        return Err(VerseError::runtime_at(
                            "NewFunction expected procedure operand",
                            span,
                        ));
                    };
                    match self.get_operand(&frame, parent_scope, span)? {
                        VmValue::Scope(scope) => function.captures = Some(scope),
                        VmValue::Runtime(Value::None) | VmValue::Uninitialized => {}
                        other => {
                            return Err(VerseError::runtime_at(
                                format!(
                                    "NewFunction expected parent scope, got {}",
                                    vm_value_kind(&other)
                                ),
                                span,
                            ));
                        }
                    }
                    frame.set_register(dest, VmValue::Function(function))?;
                }
                Instruction::NewScope { dest, values, span } => {
                    let values = values
                        .iter()
                        .map(|value| self.get_operand(&frame, *value, span))
                        .collect::<Result<Vec<_>, _>>()?;
                    frame.set_register(dest, VmValue::Scope(Rc::new(values)))?;
                }
                Instruction::LoadCapture {
                    dest,
                    scope,
                    index,
                    span,
                } => {
                    let scope = self.get_operand(&frame, scope, span)?;
                    let VmValue::Scope(values) = scope else {
                        return Err(VerseError::runtime_at(
                            "LoadCapture expected scope operand",
                            span,
                        ));
                    };
                    let value = values.get(index).cloned().ok_or_else(|| {
                        VerseError::runtime_at("capture index out of range", span)
                    })?;
                    frame.set_register(dest, value)?;
                }
                Instruction::BeginProfileBlock { dest, .. } => {
                    frame.set_register(
                        dest,
                        VmValue::Runtime(Value::Int(i128::from(profile_wall_time_start()))),
                    )?;
                }
                Instruction::EndProfileBlock {
                    wall_time_start,
                    user_tag,
                    snippet_path,
                    begin_row,
                    begin_column,
                    end_row,
                    end_column,
                    span,
                } => {
                    let _ = snippet_path;
                    let _ = expect_int(
                        self.get_runtime_operand(&frame, wall_time_start, span)?,
                        "profile wall time start",
                        span,
                    )?;
                    let user_tag = self.get_runtime_operand(&frame, user_tag, span)?;
                    if !matches!(user_tag, Value::String(_)) {
                        return Err(VerseError::runtime_at(
                            format!("profile user tag expected string, got {user_tag}"),
                            span,
                        ));
                    }
                    let _ = expect_int(
                        self.get_runtime_operand(&frame, begin_row, span)?,
                        "profile begin row",
                        span,
                    )?;
                    let _ = expect_int(
                        self.get_runtime_operand(&frame, begin_column, span)?,
                        "profile begin column",
                        span,
                    )?;
                    let _ = expect_int(
                        self.get_runtime_operand(&frame, end_row, span)?,
                        "profile end row",
                        span,
                    )?;
                    let _ = expect_int(
                        self.get_runtime_operand(&frame, end_column, span)?,
                        "profile end column",
                        span,
                    )?;
                }
            }
            frame.ip += 1;
            if current_task.is_none() && tick_entry_after_instruction {
                self.tick_entry_scheduler(Span::new(0, 0, 1, 1))?;
            }
        }

        Ok(ChunkOutcome::Failure)
    }

    fn chunk_outcome_from_op_result(
        result: OpResult<'program>,
    ) -> Result<ChunkOutcome<'program>, VerseError> {
        match result {
            OpResult::Block(_) => Ok(ChunkOutcome::Failure),
            OpResult::Yield(suspension) => Ok(ChunkOutcome::Suspended(suspension)),
        }
    }

    fn get_operand(
        &self,
        frame: &Frame<'program>,
        operand: ValueOperand,
        span: Span,
    ) -> Result<VmValue, VerseError> {
        match operand {
            ValueOperand::Register(register) => frame
                .registers
                .get(register.index())
                .cloned()
                .ok_or_else(|| {
                    VerseError::runtime_at(
                        format!("bytecode register {} out of range", register.index()),
                        span,
                    )
                }),
            ValueOperand::Constant(constant) => frame
                .chunk
                .constants()
                .get(constant)
                .map(|constant| match constant {
                    Constant::GlobalRef(name) => VmValue::Ref(self.global_ref(name)),
                    _ => self.value_from_constant(constant),
                })
                .ok_or_else(|| {
                    VerseError::runtime_at("bytecode constant index out of range", span)
                }),
            ValueOperand::Uninitialized => Ok(VmValue::Uninitialized),
        }
    }

    fn global_ref(&self, name: &str) -> Rc<RefCell<VmValue>> {
        self.globals
            .borrow_mut()
            .entry(name.to_string())
            .or_insert_with(|| Rc::new(RefCell::new(VmValue::Uninitialized)))
            .clone()
    }

    fn value_from_constant(&self, constant: &Constant) -> VmValue {
        match constant {
            Constant::ExternalAggregate {
                class_name,
                unique,
                object_kind,
                fields,
            } => match object_kind {
                ObjectKind::Class => VmValue::Runtime(
                    self.class_instance_value(
                        class_name.clone(),
                        *unique,
                        fields
                            .iter()
                            .map(|(name, mutable, type_name)| {
                                let mut visiting = Vec::new();
                                (
                                    name.clone(),
                                    *mutable,
                                    self.program_external_return_value(type_name, &mut visiting),
                                )
                            })
                            .collect(),
                    ),
                ),
                ObjectKind::Struct { computes } => VmValue::Runtime(Value::StructInstance {
                    struct_name: class_name.clone(),
                    computes: *computes,
                    fields: fields
                        .iter()
                        .map(|(name, _, type_name)| {
                            let mut visiting = Vec::new();
                            (
                                name.clone(),
                                self.program_external_return_value(type_name, &mut visiting),
                            )
                        })
                        .collect(),
                }),
            },
            Constant::ExternalInterface {
                interface_name,
                fields,
            } => VmValue::Runtime(bytecode_class_instance_value(
                interface_name.clone(),
                false,
                fields
                    .iter()
                    .map(|(name, mutable, type_name)| {
                        let mut visiting = Vec::new();
                        (
                            name.clone(),
                            *mutable,
                            self.program_external_return_value(type_name, &mut visiting),
                        )
                    })
                    .collect(),
            )),
            _ => value_from_constant(constant),
        }
    }

    fn ensure_vm_task(&mut self, task: Rc<RuntimeTask>, parent: Option<usize>) {
        let id = task_id(&task);
        if !self.tasks.contains_key(&id) {
            let result_placeholder = self.new_placeholder();
            self.tasks.insert(
                id,
                VmTaskState::new(task.clone(), parent, result_placeholder),
            );
        }
        if let Some(parent) = parent
            && let Some(parent_state) = self.tasks.get_mut(&parent)
            && !parent_state.children.contains(&id)
        {
            parent_state.children.push(id);
        }
    }

    fn add_task_to_current_group(&mut self, task_id: usize) {
        let group_index = if self.task_groups.is_empty() {
            self.task_groups.push(VmTaskGroup::default());
            0
        } else {
            self.task_groups.len() - 1
        };
        self.task_groups[group_index].active_tasks.insert(task_id);
        if let Some(task) = self.tasks.get_mut(&task_id) {
            task.task_group = Some(group_index);
        }
    }

    fn new_placeholder(&mut self) -> usize {
        let id = self.next_placeholder;
        self.next_placeholder += 1;
        self.placeholders.insert(id, VmPlaceholder::new());
        id
    }

    fn block_on_placeholder(
        &mut self,
        placeholder: usize,
        task: Rc<RuntimeTask>,
    ) -> Option<VmValue> {
        let placeholder = self.placeholders.get_mut(&placeholder)?;
        if let Some(value) = placeholder.value.clone() {
            Some(value)
        } else {
            let task_id = task_id(&task);
            if !placeholder.waiters.contains(&task_id) {
                placeholder.waiters.push(task_id);
            }
            None
        }
    }

    fn define_placeholder(
        &mut self,
        placeholder: usize,
        value: VmValue,
        span: Span,
    ) -> Result<(), VerseError> {
        let Some(placeholder) = self.placeholders.get_mut(&placeholder) else {
            return Err(VerseError::runtime_at("unknown bytecode placeholder", span));
        };
        placeholder.value = Some(value.clone());
        let waiters = std::mem::take(&mut placeholder.waiters);
        let runtime_value = into_runtime(value, span)?;
        for waiter in waiters {
            self.ready_tasks
                .push_back((waiter, copy_runtime_value(&runtime_value)));
        }
        Ok(())
    }

    fn task_from_operand(
        &self,
        frame: &Frame<'program>,
        operand: ValueOperand,
        span: Span,
    ) -> Result<Option<Rc<RuntimeTask>>, VerseError> {
        if matches!(operand, ValueOperand::Uninitialized) {
            return Ok(None);
        }
        match self.get_operand(frame, operand, span)? {
            VmValue::Runtime(Value::Task(task)) => Ok(Some(task)),
            VmValue::Uninitialized => Ok(None),
            other => Err(VerseError::runtime_at(
                format!("task operand expected task, got {}", vm_value_kind(&other)),
                span,
            )),
        }
    }

    fn get_semaphore_operand(
        &self,
        frame: &Frame<'program>,
        operand: ValueOperand,
        span: Span,
    ) -> Result<Rc<RefCell<VmSemaphore>>, VerseError> {
        match self.get_operand(frame, operand, span)? {
            VmValue::Semaphore(semaphore) => Ok(semaphore),
            other => Err(VerseError::runtime_at(
                format!(
                    "semaphore operand expected semaphore, got {}",
                    vm_value_kind(&other)
                ),
                span,
            )),
        }
    }

    fn signal_semaphore(
        &mut self,
        frame: &Frame<'program>,
        operand: ValueOperand,
        span: Span,
    ) -> Result<(), VerseError> {
        let semaphore = self.get_semaphore_operand(frame, operand, span)?;
        if let Some(awaiter) = semaphore.borrow_mut().increment_count(1) {
            self.ready_tasks.push_back((task_id(&awaiter), Value::None));
        }
        Ok(())
    }

    fn set_task_suspension(&mut self, id: usize, suspension: BytecodeSuspension<'program>) {
        if let Some(task) = self.tasks.get_mut(&id) {
            task.running = false;
            task.suspension = Some(suspension);
        }
    }

    fn take_task_suspension(&mut self, id: usize) -> Option<BytecodeSuspension<'program>> {
        let task = self.tasks.get_mut(&id)?;
        task.running = true;
        task.suspension.take()
    }

    fn run_ready_tasks(&mut self, span: Span) -> Result<(), VerseError> {
        while !self.ready_tasks.is_empty() {
            self.run_ready_tasks_tick(span)?;
        }
        Ok(())
    }

    fn run_ready_tasks_tick(&mut self, span: Span) -> Result<(), VerseError> {
        let mut processed = HashSet::new();
        let mut deferred = VecDeque::new();
        while let Some((id, value)) = self.ready_tasks.pop_front() {
            if processed.contains(&id) {
                deferred.push_back((id, value));
                continue;
            }
            processed.insert(id);
            self.run_ready_task(id, value, span)?;
        }
        self.ready_tasks.extend(deferred);
        Ok(())
    }

    fn run_ready_task(&mut self, id: usize, value: Value, span: Span) -> Result<(), VerseError> {
        let Some(suspension) = self.take_task_suspension(id) else {
            return Ok(());
        };
        match self.resume_suspension(suspension, value, id, span)? {
            ChunkOutcome::Value(value) => {
                let value = into_runtime(value, span)?;
                self.complete_bytecode_task(id, value, span)?;
            }
            ChunkOutcome::Failure => {
                return Err(VerseError::runtime_at("bytecode task failed", span));
            }
            ChunkOutcome::Suspended(suspension) => {
                self.set_task_suspension(id, suspension);
            }
        }
        Ok(())
    }

    fn run_ready_tasks_to_quiescence(&mut self, span: Span) -> Result<(), VerseError> {
        self.run_ready_tasks(span)
    }

    fn poll_host_ready_tasks(&mut self, span: Span) -> Result<bool, VerseError> {
        let ready = self.host.poll_ready();
        let mut woke_any = false;
        for (token, value) in ready {
            let Some(wake) = self.pending_wakes.remove(&token) else {
                continue;
            };
            match wake {
                HostWake::Task(id) => {
                    let Some(task) = self.tasks.get_mut(&id) else {
                        continue;
                    };
                    if task.pending_token == Some(token) {
                        task.pending_token = None;
                    }
                    if task.active() {
                        self.ready_tasks.push_back((id, value));
                        woke_any = true;
                    }
                }
                HostWake::Subscribers(subscribers) => {
                    let key = subscription_key(&subscribers);
                    self.subscription_signal_tokens.remove(&key);
                    if self.run_subscription_callbacks(subscribers.clone(), value, span)? {
                        woke_any = true;
                    }
                    if !subscribers.borrow().is_empty() {
                        self.arm_subscription_signal_future(subscribers);
                    }
                }
            }
        }
        Ok(woke_any)
    }

    fn next_pending_token(&mut self) -> PendingToken {
        let token = PendingToken(self.next_pending_token);
        self.next_pending_token += 1;
        token
    }

    fn arm_task_timer(&mut self, id: usize, seconds: f64) {
        let token = self.next_pending_token();
        self.pending_wakes.insert(token, HostWake::Task(id));
        if let Some(task) = self.tasks.get_mut(&id) {
            task.pending_token = Some(token);
        }
        self.host.arm_timer(duration_from_seconds(seconds), token);
    }

    fn arm_task_future(
        &mut self,
        id: usize,
        future: std::pin::Pin<Box<dyn std::future::Future<Output = Value>>>,
    ) {
        let token = self.next_pending_token();
        self.pending_wakes.insert(token, HostWake::Task(id));
        if let Some(task) = self.tasks.get_mut(&id) {
            task.pending_token = Some(token);
        }
        self.host.arm_future(future, token);
    }

    fn arm_subscription_signal_future(
        &mut self,
        subscribers: Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>,
    ) {
        let key = subscription_key(&subscribers);
        if self.subscription_signal_tokens.contains_key(&key) {
            return;
        }
        let token = self.next_pending_token();
        self.pending_wakes
            .insert(token, HostWake::Subscribers(subscribers));
        self.subscription_signal_tokens.insert(key, token);
        self.host
            .arm_future(Box::pin(std::future::pending::<Value>()), token);
    }

    fn register_subscription_callback(
        &mut self,
        subscribers: &Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>,
        id: u64,
        arity: usize,
        callback: VmValue,
    ) {
        let key = subscription_key(subscribers);
        self.subscription_callbacks
            .entry(key)
            .or_default()
            .push(VmSubscriptionCallback {
                id,
                arity,
                callback,
            });
    }

    fn remove_subscription_callback(
        &mut self,
        subscribers: &Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>,
        subscriber_id: u64,
    ) {
        let key = subscription_key(subscribers);
        if let Some(callbacks) = self.subscription_callbacks.get_mut(&key) {
            callbacks.retain(|entry| entry.id != subscriber_id);
        }
    }

    fn clear_subscription_signal_token(
        &mut self,
        subscribers: &Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>,
    ) {
        let key = subscription_key(subscribers);
        if let Some(token) = self.subscription_signal_tokens.remove(&key) {
            self.pending_wakes.remove(&token);
            self.host.cancel(token);
        }
    }

    fn run_subscription_callbacks(
        &mut self,
        subscribers: Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>,
        payload: Value,
        span: Span,
    ) -> Result<bool, VerseError> {
        let key = subscription_key(&subscribers);
        let Some(callbacks) = self.subscription_callbacks.get(&key).cloned() else {
            return Ok(false);
        };
        let active_ids = subscribers
            .borrow()
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        let mut ran_any = false;
        for id in active_ids {
            let Some(callback) = callbacks.iter().find(|entry| entry.id == id).cloned() else {
                continue;
            };
            let args = if callback.arity == 0 {
                Vec::new()
            } else {
                vec![VmValue::Runtime(copy_runtime_value(&payload))]
            };
            match self.call(callback.callback, args, Vec::new(), span, None)? {
                CallOutcome::Value(_) => ran_any = true,
                CallOutcome::Failure => {
                    return Err(VerseError::runtime_at("subscription callback failed", span));
                }
                CallOutcome::Yield | CallOutcome::Block(_) | CallOutcome::Suspended(_) => {
                    return Err(VerseError::runtime_at(
                        "subscription callback suspended during host signal",
                        span,
                    ));
                }
            }
        }
        Ok(ran_any)
    }

    fn clear_task_pending_token(&mut self, id: usize) {
        if let Some(token) = self
            .tasks
            .get_mut(&id)
            .and_then(|task| task.pending_token.take())
        {
            self.pending_wakes.remove(&token);
            self.host.cancel(token);
        }
    }

    fn run_ready_tasks_until_first_completed(
        &mut self,
        watched_tasks: &[(usize, Rc<RuntimeTask>)],
        span: Span,
    ) -> Result<(), VerseError> {
        let watched = watched_tasks
            .iter()
            .map(|(_, task)| task_id(task))
            .collect::<HashSet<_>>();
        while let Some((id, value)) = self.ready_tasks.pop_front() {
            let Some(suspension) = self.take_task_suspension(id) else {
                continue;
            };
            match self.resume_suspension(suspension, value, id, span)? {
                ChunkOutcome::Value(value) => {
                    let value = into_runtime(value, span)?;
                    self.complete_bytecode_task(id, value, span)?;
                    if watched.contains(&id) {
                        return Ok(());
                    }
                }
                ChunkOutcome::Failure => {
                    return Err(VerseError::runtime_at("bytecode task failed", span));
                }
                ChunkOutcome::Suspended(suspension) => {
                    self.set_task_suspension(id, suspension);
                }
            }
        }
        Ok(())
    }

    fn run_ready_tasks_until_all_completed(
        &mut self,
        watched_tasks: &[(usize, Rc<RuntimeTask>)],
        span: Span,
    ) -> Result<(), VerseError> {
        let watched = watched_tasks
            .iter()
            .map(|(_, task)| task_id(task))
            .collect::<HashSet<_>>();
        while !watched.iter().all(|id| {
            self.tasks
                .get(id)
                .and_then(|task| task.result.as_ref())
                .is_some()
        }) {
            let Some((id, value)) = self.ready_tasks.pop_front() else {
                break;
            };
            let Some(suspension) = self.take_task_suspension(id) else {
                continue;
            };
            match self.resume_suspension(suspension, value, id, span)? {
                ChunkOutcome::Value(value) => {
                    let value = into_runtime(value, span)?;
                    self.complete_bytecode_task(id, value, span)?;
                }
                ChunkOutcome::Failure => {
                    return Err(VerseError::runtime_at("bytecode task failed", span));
                }
                ChunkOutcome::Suspended(suspension) => {
                    self.set_task_suspension(id, suspension);
                }
            }
        }
        Ok(())
    }

    fn drive_scheduler_until_idle(&mut self, span: Span) -> Result<(), VerseError> {
        loop {
            self.run_ready_tasks_to_quiescence(span)?;
            if !self.host.has_pending() {
                return Ok(());
            }
            if !self.poll_host_ready_tasks(span)? {
                return Ok(());
            }
        }
    }

    fn tick_entry_scheduler(&mut self, span: Span) -> Result<(), VerseError> {
        self.run_ready_tasks_tick(span)?;
        if self.ready_tasks.is_empty()
            && self.host.has_pending()
            && self.poll_host_ready_tasks(span)?
        {
            self.run_ready_tasks_tick(span)?;
        }
        Ok(())
    }

    fn create_join(&mut self, kind: VmJoinKind, tasks: &[(usize, Rc<RuntimeTask>)]) -> usize {
        let placeholder = self.new_placeholder();
        let join_id = self.joins.len();
        let task_ids = tasks
            .iter()
            .map(|(index, task)| (*index, task_id(task)))
            .collect::<Vec<_>>();
        self.joins.push(VmJoin {
            kind,
            tasks: task_ids.clone(),
            placeholder,
            completed: false,
        });
        for (_, id) in task_ids {
            if let Some(task) = self.tasks.get_mut(&id)
                && !task.joins.contains(&join_id)
            {
                task.joins.push(join_id);
            }
        }
        placeholder
    }

    fn notify_task_joins(&mut self, id: usize, span: Span) -> Result<(), VerseError> {
        let joins = self
            .tasks
            .get(&id)
            .map(|task| task.joins.clone())
            .unwrap_or_default();
        for join in joins {
            self.try_complete_join(join, id, span)?;
        }
        Ok(())
    }

    fn try_complete_join(
        &mut self,
        join_id: usize,
        completed_task: usize,
        span: Span,
    ) -> Result<(), VerseError> {
        let Some(join) = self.joins.get(join_id) else {
            return Ok(());
        };
        if join.completed {
            return Ok(());
        }
        let kind = join.kind;
        let tasks = join.tasks.clone();
        let placeholder = join.placeholder;

        match kind {
            VmJoinKind::Sync => {
                let mut values = vec![None; tasks.len()];
                for (index, id) in &tasks {
                    let Some(value) = self
                        .tasks
                        .get(id)
                        .and_then(|task| task.result.as_ref().map(copy_runtime_value))
                    else {
                        return Ok(());
                    };
                    if let Some(slot) = values.get_mut(*index) {
                        *slot = Some(value);
                    }
                }
                let values = values
                    .into_iter()
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| VerseError::runtime_at("sync join task result missing", span))?;
                if let Some(join) = self.joins.get_mut(join_id) {
                    join.completed = true;
                }
                self.define_placeholder(placeholder, VmValue::Runtime(Value::Tuple(values)), span)?;
            }
            VmJoinKind::Race | VmJoinKind::Rush => {
                let Some((winner_index, value)) = tasks.iter().find_map(|(index, id)| {
                    (*id == completed_task).then(|| {
                        self.tasks
                            .get(id)
                            .and_then(|task| task.result.as_ref().map(copy_runtime_value))
                            .map(|value| (*index, value))
                    })?
                }) else {
                    return Ok(());
                };
                if let Some(join) = self.joins.get_mut(join_id) {
                    join.completed = true;
                }
                self.define_placeholder(placeholder, VmValue::Runtime(value), span)?;
                if kind == VmJoinKind::Race {
                    for (index, id) in tasks {
                        if index != winner_index {
                            self.cancel_bytecode_task(id, span)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn complete_bytecode_task(
        &mut self,
        id: usize,
        value: Value,
        span: Span,
    ) -> Result<(), VerseError> {
        self.detach_task_from_parent(id);
        let children = self
            .tasks
            .get(&id)
            .map(|task| task.children.clone())
            .unwrap_or_default();
        for child in children.into_iter().rev() {
            self.cancel_bytecode_task(child, span)?;
        }
        let (defers, awaiters, result_placeholder, task_handle) =
            if let Some(task) = self.tasks.get_mut(&id) {
                task.running = false;
                task.result = Some(copy_runtime_value(&value));
                task.suspension = None;
                task.children.clear();
                let task_handle = task.handle.clone();
                let defers = std::mem::take(&mut task.native_defers);
                let awaiters = std::mem::take(&mut task.awaits);
                let result_placeholder = task.result_placeholder;
                if let Some(group) = task.task_group
                    && let Some(group) = self.task_groups.get_mut(group)
                {
                    group.active_tasks.remove(&id);
                }
                (
                    defers,
                    awaiters,
                    Some(result_placeholder),
                    Some(task_handle),
                )
            } else {
                (Vec::new(), Vec::new(), None, None)
            };
        if let Some(result_placeholder) = result_placeholder {
            self.define_placeholder(
                result_placeholder,
                VmValue::Runtime(copy_runtime_value(&value)),
                span,
            )?;
        }
        for function in defers.into_iter().rev() {
            self.run_defer_function(function, span, task_handle.clone())?;
        }
        self.notify_task_joins(id, span)?;
        for awaiter in awaiters {
            self.ready_tasks
                .push_back((awaiter, copy_runtime_value(&value)));
        }
        Ok(())
    }

    fn start_bytecode_task(
        &mut self,
        function: VmFunction,
        args: Vec<VmValue>,
        span: Span,
        parent_task: Option<Rc<RuntimeTask>>,
        drain_ready: bool,
    ) -> Result<Rc<RuntimeTask>, VerseError> {
        let descriptor = self
            .program
            .functions()
            .get(function.function)
            .ok_or_else(|| VerseError::runtime_at("bytecode function index out of range", span))?;
        let task = bytecode_new_running_task();
        let id = task_id(&task);
        let parent_id = parent_task.as_ref().map(task_id);
        self.ensure_vm_task(task.clone(), parent_id);
        self.add_task_to_current_group(id);
        if let Some(state) = self.tasks.get_mut(&id) {
            state.running = true;
        }
        match self.run_chunk(
            descriptor.chunk(),
            args,
            span,
            Some(task.clone()),
            function.captures.clone(),
        )? {
            ChunkOutcome::Value(value) => {
                let value = into_runtime(value, span)?;
                self.complete_bytecode_task(id, value, span)?;
            }
            ChunkOutcome::Failure => {
                return Err(VerseError::runtime_at("bytecode task failed", span));
            }
            ChunkOutcome::Suspended(suspension) => {
                self.set_task_suspension(id, suspension);
            }
        }
        if drain_ready {
            self.run_ready_tasks_tick(span)?;
        }
        Ok(task)
    }

    fn task_result(&self, task: &Rc<RuntimeTask>, span: Span) -> Result<Option<Value>, VerseError> {
        let _ = span;
        Ok(self
            .tasks
            .get(&task_id(task))
            .and_then(|task| task.result.as_ref().map(copy_runtime_value)))
    }

    fn first_completed_task(
        &self,
        tasks: &[(usize, Rc<RuntimeTask>)],
        span: Span,
    ) -> Result<Option<(usize, Value)>, VerseError> {
        for (index, task) in tasks {
            if let Some(value) = self.task_result(task, span)? {
                return Ok(Some((*index, value)));
            }
        }
        Ok(None)
    }

    fn cancel_bytecode_losers(
        &mut self,
        tasks: &[(usize, Rc<RuntimeTask>)],
        winner_index: usize,
        span: Span,
    ) -> Result<(), VerseError> {
        for (index, task) in tasks {
            if *index != winner_index {
                self.cancel_bytecode_task(task_id(task), span)?;
            }
        }
        Ok(())
    }

    fn cancel_bytecode_task(&mut self, id: usize, span: Span) -> Result<(), VerseError> {
        let Some(task) = self.tasks.get(&id) else {
            return Ok(());
        };
        if !task.active() {
            return Ok(());
        }
        if let Some(task) = self.tasks.get_mut(&id) {
            task.phase = TaskPhase::CancelRequested;
        }
        self.detach_task_from_parent(id);
        let children = self
            .tasks
            .get(&id)
            .map(|task| task.children.clone())
            .unwrap_or_default();
        for child in children.into_iter().rev() {
            self.cancel_bytecode_task(child, span)?;
        }
        self.ready_tasks.retain(|(queued, _)| *queued != id);
        self.clear_task_pending_token(id);
        if let Some(task) = self.tasks.get_mut(&id) {
            task.phase = TaskPhase::CancelStarted;
        }
        let (defers, awaiters, result_placeholder, task_handle) =
            if let Some(task) = self.tasks.get_mut(&id) {
                task.phase = TaskPhase::CancelUnwind;
                task.running = false;
                task.result = Some(Value::Bool(false));
                task.suspension = None;
                task.children.clear();
                let task_handle = task.handle.clone();
                let defers = std::mem::take(&mut task.native_defers);
                let awaiters = std::mem::take(&mut task.awaits);
                let result_placeholder = task.result_placeholder;
                task.cancels.clear();
                if let Some(group) = task.task_group
                    && let Some(group) = self.task_groups.get_mut(group)
                {
                    group.active_tasks.remove(&id);
                }
                (
                    defers,
                    awaiters,
                    Some(result_placeholder),
                    Some(task_handle),
                )
            } else {
                (Vec::new(), Vec::new(), None, None)
            };
        if let Some(result_placeholder) = result_placeholder {
            self.define_placeholder(
                result_placeholder,
                VmValue::Runtime(Value::Bool(false)),
                span,
            )?;
        }
        for function in defers.into_iter().rev() {
            self.run_defer_function(function, span, task_handle.clone())?;
        }
        self.notify_task_joins(id, span)?;
        for awaiter in awaiters {
            self.ready_tasks.push_back((awaiter, Value::Bool(false)));
        }
        if let Some(task) = self.tasks.get_mut(&id) {
            task.phase = TaskPhase::Canceled;
        }
        Ok(())
    }

    fn detach_task_from_parent(&mut self, id: usize) {
        let Some(parent_id) = self.tasks.get_mut(&id).and_then(|task| task.parent.take()) else {
            return;
        };
        if let Some(parent) = self.tasks.get_mut(&parent_id) {
            parent.children.retain(|child| *child != id);
        }
    }

    fn begin_defer_scope(
        &mut self,
        current_task: Option<Rc<RuntimeTask>>,
        span: Span,
    ) -> Result<(), VerseError> {
        if let Some(task) = current_task {
            let id = task_id(&task);
            let Some(task) = self.tasks.get_mut(&id) else {
                return Err(VerseError::runtime_at(
                    "defer scope requires a known task",
                    span,
                ));
            };
            task.native_defer_scopes.push(task.native_defers.len());
        } else {
            self.entry_native_defer_scopes
                .push(self.entry_native_defers.len());
        }
        Ok(())
    }

    fn push_native_defer(
        &mut self,
        function: VmFunction,
        current_task: Option<Rc<RuntimeTask>>,
        span: Span,
    ) -> Result<(), VerseError> {
        if let Some(task) = current_task {
            let id = task_id(&task);
            let Some(task) = self.tasks.get_mut(&id) else {
                return Err(VerseError::runtime_at(
                    "defer registration requires a known task",
                    span,
                ));
            };
            task.native_defers.push(function);
        } else {
            self.entry_native_defers.push(function);
        }
        Ok(())
    }

    fn end_defer_scope(
        &mut self,
        current_task: Option<Rc<RuntimeTask>>,
        span: Span,
    ) -> Result<(), VerseError> {
        let marker = if let Some(task) = current_task.as_ref() {
            let id = task_id(task);
            let Some(task) = self.tasks.get_mut(&id) else {
                return Err(VerseError::runtime_at(
                    "defer scope requires a known task",
                    span,
                ));
            };
            task.native_defer_scopes.pop().unwrap_or(0)
        } else {
            self.entry_native_defer_scopes.pop().unwrap_or(0)
        };

        loop {
            let function = if let Some(task) = current_task.as_ref() {
                let id = task_id(task);
                let Some(task) = self.tasks.get_mut(&id) else {
                    return Err(VerseError::runtime_at(
                        "defer scope requires a known task",
                        span,
                    ));
                };
                if task.native_defers.len() <= marker {
                    None
                } else {
                    task.native_defers.pop()
                }
            } else if self.entry_native_defers.len() <= marker {
                None
            } else {
                self.entry_native_defers.pop()
            };

            let Some(function) = function else {
                break;
            };
            self.run_defer_function(function, span, current_task.clone())?;
        }

        Ok(())
    }

    fn run_defer_function(
        &mut self,
        function: VmFunction,
        span: Span,
        current_task: Option<Rc<RuntimeTask>>,
    ) -> Result<(), VerseError> {
        let descriptor = self
            .program
            .functions()
            .get(function.function)
            .ok_or_else(|| VerseError::runtime_at("defer function index out of range", span))?;
        match self.run_chunk(
            descriptor.chunk(),
            Vec::new(),
            span,
            current_task,
            function.captures.clone(),
        )? {
            ChunkOutcome::Value(_) => Ok(()),
            ChunkOutcome::Failure => Err(VerseError::runtime_at("defer function failed", span)),
            ChunkOutcome::Suspended(_) => Err(VerseError::runtime_at(
                "defer function suspended in bytecode VM",
                span,
            )),
        }
    }

    fn wait_for_pending_tasks(
        &mut self,
        tasks: &[(usize, Rc<RuntimeTask>)],
        current_task: Option<Rc<RuntimeTask>>,
        span: Span,
    ) -> Result<CallOutcome<'program>, VerseError> {
        let Some(current_task) = current_task else {
            return Ok(CallOutcome::Value(VmValue::Runtime(Value::Pending)));
        };
        let mut pending = Vec::new();
        for (_, task) in tasks {
            if self.task_result(task, span)?.is_none() {
                pending.push(task_id(task));
            }
        }
        if pending.is_empty() {
            return Ok(CallOutcome::Value(VmValue::Runtime(Value::None)));
        }
        if pending.len() == 1
            && let Some(placeholder) = self
                .tasks
                .get(&pending[0])
                .map(|task| task.result_placeholder)
        {
            return Ok(CallOutcome::Block(placeholder));
        }
        let current_id = task_id(&current_task);
        for id in pending {
            if let Some(awaited) = self.tasks.get_mut(&id)
                && !awaited.awaits.contains(&current_id)
            {
                awaited.awaits.push(current_id);
            }
        }
        Ok(CallOutcome::Yield)
    }

    fn call_structured_concurrency(
        &mut self,
        name: &'static str,
        args: Vec<VmValue>,
        named_args: Vec<(String, VmValue)>,
        span: Span,
        current_task: Option<Rc<RuntimeTask>>,
    ) -> Result<CallOutcome<'program>, VerseError> {
        if !named_args.is_empty() {
            return Err(VerseError::runtime_at(
                format!("`{name}` does not accept named arguments"),
                span,
            ));
        }
        let mut functions = Vec::with_capacity(args.len());
        for arg in args {
            let VmValue::Function(function) = arg else {
                return Err(VerseError::runtime_at(
                    format!("`{name}` expected procedure operands"),
                    span,
                ));
            };
            functions.push(function);
        }
        match name {
            "__verse_branch" => {
                for function in functions {
                    let _ = self.start_bytecode_task(
                        function,
                        Vec::new(),
                        span,
                        current_task.clone(),
                        false,
                    )?;
                }
                Ok(CallOutcome::Value(VmValue::Runtime(Value::None)))
            }
            "__verse_sync" => {
                let mut tasks = Vec::with_capacity(functions.len());
                for (index, function) in functions.into_iter().enumerate() {
                    tasks.push((
                        index,
                        self.start_bytecode_task(
                            function,
                            Vec::new(),
                            span,
                            current_task.clone(),
                            true,
                        )?,
                    ));
                }
                let mut values = vec![None; tasks.len()];
                let mut pending = false;
                for (index, task) in &tasks {
                    if let Some(value) = self.task_result(task, span)? {
                        values[*index] = Some(value);
                    } else {
                        pending = true;
                    }
                }
                if pending {
                    self.run_ready_tasks_until_all_completed(&tasks, span)?;
                    pending = false;
                    for (index, task) in &tasks {
                        if let Some(value) = self.task_result(task, span)? {
                            values[*index] = Some(value);
                        } else {
                            pending = true;
                        }
                    }
                    if pending {
                        let placeholder = self.create_join(VmJoinKind::Sync, &tasks);
                        return Ok(CallOutcome::Block(placeholder));
                    }
                }
                let values = values
                    .into_iter()
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| VerseError::runtime_at("sync task result missing", span))?;
                Ok(CallOutcome::Value(VmValue::Runtime(Value::Tuple(values))))
            }
            "__verse_race" => {
                let mut tasks = Vec::with_capacity(functions.len());
                for (index, function) in functions.into_iter().enumerate() {
                    let task = self.start_bytecode_task(
                        function,
                        Vec::new(),
                        span,
                        current_task.clone(),
                        false,
                    )?;
                    tasks.push((index, task));
                    if let Some((winner_index, value)) = self.first_completed_task(&tasks, span)? {
                        self.cancel_bytecode_losers(&tasks, winner_index, span)?;
                        return Ok(CallOutcome::Value(VmValue::Runtime(value)));
                    }
                }
                self.run_ready_tasks_until_first_completed(&tasks, span)?;
                if let Some((winner_index, value)) = self.first_completed_task(&tasks, span)? {
                    self.cancel_bytecode_losers(&tasks, winner_index, span)?;
                    return Ok(CallOutcome::Value(VmValue::Runtime(value)));
                }
                let placeholder = self.create_join(VmJoinKind::Race, &tasks);
                Ok(CallOutcome::Block(placeholder))
            }
            "__verse_rush" => {
                let mut tasks = Vec::with_capacity(functions.len());
                let mut winner = None;
                for (index, function) in functions.into_iter().enumerate() {
                    let task = self.start_bytecode_task(
                        function,
                        Vec::new(),
                        span,
                        current_task.clone(),
                        false,
                    )?;
                    tasks.push((index, task));
                    if winner.is_none() {
                        self.run_ready_tasks_until_first_completed(&tasks, span)?;
                    }
                    if winner.is_none() {
                        winner = self.first_completed_task(&tasks, span)?;
                    }
                }
                if let Some((_, value)) = winner {
                    return Ok(CallOutcome::Value(VmValue::Runtime(value)));
                }
                self.run_ready_tasks_until_first_completed(&tasks, span)?;
                if let Some((_, value)) = self.first_completed_task(&tasks, span)? {
                    return Ok(CallOutcome::Value(VmValue::Runtime(value)));
                }
                let placeholder = self.create_join(VmJoinKind::Rush, &tasks);
                Ok(CallOutcome::Block(placeholder))
            }
            _ => Err(VerseError::runtime_at(
                format!("unknown structured concurrency hook `{name}`"),
                span,
            )),
        }
    }

    fn resume_suspension(
        &mut self,
        suspension: BytecodeSuspension<'program>,
        value: Value,
        current_task: usize,
        span: Span,
    ) -> Result<ChunkOutcome<'program>, VerseError> {
        let task_handle = self
            .tasks
            .get(&current_task)
            .map(|task| task.handle.clone())
            .ok_or_else(|| VerseError::runtime_at("unknown bytecode task", span))?;
        match suspension {
            BytecodeSuspension::AwaitCall { mut frame, dest } => {
                frame.set_register(dest, VmValue::Runtime(value))?;
                self.run_frame(frame, Some(task_handle))
            }
            BytecodeSuspension::Yield { frame } => self.run_frame(frame, Some(task_handle)),
            BytecodeSuspension::Call {
                mut parent,
                dest,
                child,
            } => match self.resume_suspension(*child, value, current_task, span)? {
                ChunkOutcome::Value(value) => {
                    parent.set_register(dest, value)?;
                    self.run_frame(parent, Some(task_handle))
                }
                ChunkOutcome::Failure => {
                    if Self::fail_current_context(&mut parent) {
                        self.run_frame(parent, Some(task_handle))
                    } else {
                        Ok(ChunkOutcome::Failure)
                    }
                }
                ChunkOutcome::Suspended(child) => {
                    Ok(ChunkOutcome::Suspended(BytecodeSuspension::Call {
                        parent,
                        dest,
                        child: Box::new(child),
                    }))
                }
            },
        }
    }

    fn load_field_value(
        &mut self,
        object: Value,
        name: &str,
        span: Span,
    ) -> Result<VmValue, VerseError> {
        if let Some(value) = bytecode_native_array_method_value(object.clone(), name) {
            return Ok(VmValue::Runtime(value));
        }
        let (qualifier, member_name) = parse_qualified_member_name(name);
        if qualifier.is_none()
            && let Some(value) = self.load_predicts_field_value(&object, member_name)
        {
            return Ok(VmValue::Runtime(value));
        }
        if qualifier.is_none()
            && let Some(value) = bytecode_load_field_value(&object, member_name)
        {
            return Ok(VmValue::Runtime(value));
        }
        if qualifier.is_none()
            && let Some(value) = bytecode_native_member_value(&object, member_name)
        {
            return Ok(VmValue::Runtime(value));
        }
        if qualifier.is_none()
            && let Some(value) = number_method_value(&object, member_name)
        {
            return Ok(value);
        }
        if let Some(method) = self.bound_method_value(&object, qualifier, member_name) {
            return Ok(VmValue::BoundMethod(method));
        }
        Err(VerseError::runtime_at(
            format!("field `{name}` not found"),
            span,
        ))
    }

    fn load_predicts_field_value(&mut self, object: &Value, name: &str) -> Option<Value> {
        let Value::ClassInstance { fields, .. } = object else {
            return None;
        };
        let object = Rc::as_ptr(fields) as usize;
        let (owner_class, field_name, default) = {
            let fields = fields.borrow();
            let field = fields.iter().find(|field| field.name == name)?;
            if !field.predicts {
                return None;
            }
            (
                field.owner_class.clone(),
                field.name.clone(),
                field.value.clone(),
            )
        };
        Some(
            self.host
                .prediction_value(object, &owner_class, &field_name, &default),
        )
    }

    fn bound_method_value(
        &self,
        object: &Value,
        qualifier: Option<&str>,
        name: &str,
    ) -> Option<VmBoundMethod> {
        let Value::ClassInstance {
            class_name, fields, ..
        } = object
        else {
            return None;
        };
        let (methods, type_params) =
            if let Some(class) = self.program_class_by_runtime_name(class_name) {
                (class.methods(), class.type_params())
            } else {
                let interface = self.program_interface_by_runtime_name(class_name)?;
                (interface.methods(), interface.type_params())
            };
        let mut candidates = methods
            .iter()
            .filter(|method| {
                method.name() == name
                    && qualifier.is_none_or(|qualifier| method.qualifier() == Some(qualifier))
            })
            .map(|method| bytecode_method_candidate(method, type_params))
            .collect::<Vec<_>>();
        if candidates.is_empty() && qualifier.is_some() {
            candidates = methods
                .iter()
                .filter(|method| method.name() == name)
                .map(|method| bytecode_method_candidate(method, type_params))
                .collect::<Vec<_>>();
        }
        if candidates.is_empty() {
            return None;
        }
        Some(VmBoundMethod {
            name: name.to_string(),
            candidates,
            self_value: object.clone(),
            fields: fields.clone(),
        })
    }

    fn load_super_field_value(
        &self,
        object: Value,
        base_class: &str,
        name: &str,
        span: Span,
    ) -> Result<VmValue, VerseError> {
        let Value::ClassInstance { fields, .. } = &object else {
            return Err(VerseError::runtime_at(
                format!("cannot load super field `{name}` from non-class value"),
                span,
            ));
        };
        let fields = fields.clone();
        let class = self.program.class(base_class).ok_or_else(|| {
            VerseError::runtime_at(format!("unknown super class `{base_class}`"), span)
        })?;
        let candidates = class
            .methods()
            .iter()
            .filter(|method| method.name() == name)
            .map(|method| bytecode_method_candidate(method, class.type_params()))
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Err(VerseError::runtime_at(
                format!("class `{base_class}` has no method `{name}`"),
                span,
            ));
        }
        Ok(VmValue::BoundMethod(VmBoundMethod {
            name: name.to_string(),
            candidates,
            self_value: object,
            fields,
        }))
    }

    fn run_class_blocks(
        &mut self,
        object: &Value,
        class_name: &str,
        span: Span,
        current_task: Option<Rc<RuntimeTask>>,
    ) -> Result<(), VerseError> {
        let blocks = self
            .program_class_by_runtime_name(class_name)
            .map(|class| class.blocks().to_vec())
            .unwrap_or_default();
        let Value::ClassInstance { fields, .. } = object else {
            return Ok(());
        };
        for block in blocks {
            let descriptor = self
                .program
                .functions()
                .get(block.function())
                .ok_or_else(|| {
                    VerseError::runtime_at("bytecode class block index out of range", span)
                })?;
            let mut args = Vec::with_capacity(1 + block.field_count());
            args.push(VmValue::Runtime(object.clone()));
            for index in 0..block.field_count() {
                args.push(VmValue::FieldRef(VmFieldRef {
                    fields: fields.clone(),
                    index,
                }));
            }
            match self.run_chunk(descriptor.chunk(), args, span, current_task.clone(), None)? {
                ChunkOutcome::Value(_) => {}
                ChunkOutcome::Failure => {
                    return Err(VerseError::runtime_at("class block failed", span));
                }
                ChunkOutcome::Suspended(_) => {
                    return Err(VerseError::runtime_at(
                        "class block suspended in bytecode VM",
                        span,
                    ));
                }
            }
        }
        Ok(())
    }

    fn get_runtime_operand(
        &self,
        frame: &Frame<'program>,
        operand: ValueOperand,
        span: Span,
    ) -> Result<Value, VerseError> {
        into_runtime(self.get_operand(frame, operand, span)?, span)
    }

    fn get_class_field_operand(
        &self,
        frame: &Frame<'program>,
        operand: ValueOperand,
        span: Span,
    ) -> Result<Value, VerseError> {
        match self.get_operand(frame, operand, span)? {
            VmValue::Runtime(value) => Ok(value),
            VmValue::Function(_) | VmValue::BoundMethod(_) => Ok(Value::External),
            VmValue::NumberMethod(_) => Ok(Value::External),
            VmValue::Scope(_) => Err(VerseError::runtime_at(
                "bytecode scopes cannot initialize class fields",
                span,
            )),
            VmValue::Option(value) => Ok(Value::Option(
                value
                    .map(|value| into_runtime(*value, span))
                    .transpose()?
                    .map(Box::new),
            )),
            VmValue::Semaphore(_) => Err(VerseError::runtime_at(
                "bytecode semaphores cannot initialize class fields",
                span,
            )),
            VmValue::Placeholder(_) => Err(VerseError::runtime_at(
                "bytecode placeholders cannot initialize class fields",
                span,
            )),
            VmValue::Ref(_) => Err(VerseError::runtime_at(
                "bytecode refs cannot initialize class fields",
                span,
            )),
            VmValue::FieldRef(_) => Err(VerseError::runtime_at(
                "bytecode field refs cannot initialize class fields",
                span,
            )),
            VmValue::Uninitialized => {
                Err(VerseError::runtime_at("uninitialized bytecode value", span))
            }
        }
    }

    fn set_field_operand(
        &mut self,
        frame: &mut Frame<'program>,
        object: ValueOperand,
        name: &str,
        value: Value,
        span: Span,
    ) -> Result<(), VerseError> {
        if let ValueOperand::Register(register) = object {
            let Some(slot) = frame.registers.get_mut(register.index()) else {
                return Err(VerseError::runtime_at(
                    format!("bytecode register {} out of range", register.index()),
                    span,
                ));
            };
            return self.set_field_vm_value(slot, name, value, span);
        }

        let object = self.get_runtime_operand(frame, object, span)?;
        self.set_field_runtime_value(object, name, value, span)
    }

    fn set_field_vm_value(
        &mut self,
        object: &mut VmValue,
        name: &str,
        value: Value,
        span: Span,
    ) -> Result<(), VerseError> {
        match object {
            VmValue::Runtime(Value::ClassInstance { fields, .. }) => {
                self.set_class_field_value(fields, name, value, span)
            }
            VmValue::Runtime(_) => set_field_vm_value(object, name, value, span),
            _ => set_field_vm_value(object, name, value, span),
        }
    }

    fn set_field_runtime_value(
        &mut self,
        object: Value,
        name: &str,
        value: Value,
        span: Span,
    ) -> Result<(), VerseError> {
        match object {
            Value::ClassInstance { fields, .. } => {
                self.set_class_field_value(&fields, name, value, span)
            }
            object => set_field_value(object, name, value, span),
        }
    }

    fn set_class_field_value(
        &mut self,
        fields: &Rc<RefCell<Vec<RuntimeClassInstanceField>>>,
        name: &str,
        value: Value,
        span: Span,
    ) -> Result<(), VerseError> {
        let object = Rc::as_ptr(fields) as usize;
        let mut fields = fields.borrow_mut();
        let Some(field) = fields.iter_mut().find(|field| field.name == name) else {
            return Err(VerseError::runtime_at(
                format!("class has no field `{name}`"),
                span,
            ));
        };
        if field.predicts {
            self.host
                .set_prediction_value(object, &field.owner_class, &field.name, value);
        } else {
            field.value = value;
        }
        Ok(())
    }

    fn get_ref_operand(
        &self,
        frame: &Frame<'program>,
        operand: ValueOperand,
        span: Span,
    ) -> Result<VmRef, VerseError> {
        match self.get_operand(frame, operand, span)? {
            VmValue::Ref(ref_value) => Ok(VmRef::Local(ref_value)),
            VmValue::FieldRef(field_ref) => Ok(VmRef::Field(field_ref)),
            VmValue::Runtime(value) => Err(VerseError::runtime_at(
                format!("bytecode ref operation expected ref, got {value}"),
                span,
            )),
            VmValue::Function(_) | VmValue::BoundMethod(_) => Err(VerseError::runtime_at(
                "bytecode ref operation expected ref, got function",
                span,
            )),
            VmValue::NumberMethod(_) => Err(VerseError::runtime_at(
                "bytecode ref operation expected ref, got number method",
                span,
            )),
            VmValue::Semaphore(_) => Err(VerseError::runtime_at(
                "bytecode ref operation expected ref, got semaphore",
                span,
            )),
            VmValue::Scope(_) => Err(VerseError::runtime_at(
                "bytecode ref operation expected ref, got scope",
                span,
            )),
            VmValue::Option(_) => Err(VerseError::runtime_at(
                "bytecode ref operation expected ref, got option",
                span,
            )),
            VmValue::Placeholder(_) => Err(VerseError::runtime_at(
                "bytecode ref operation expected ref, got placeholder",
                span,
            )),
            VmValue::Uninitialized => Err(VerseError::runtime_at(
                "bytecode ref operation expected initialized ref",
                span,
            )),
        }
    }

    fn get_ref_value(&mut self, ref_value: &VmRef, span: Span) -> Result<VmValue, VerseError> {
        match ref_value {
            VmRef::Local(value) => Ok(value.borrow().clone()),
            VmRef::Field(field_ref) => self.get_field_ref_value(field_ref, span),
        }
    }

    fn set_ref_value(
        &mut self,
        ref_value: &VmRef,
        value: VmValue,
        span: Span,
    ) -> Result<(), VerseError> {
        match ref_value {
            VmRef::Local(ref_value) => {
                *ref_value.borrow_mut() = value;
                Ok(())
            }
            VmRef::Field(field_ref) => self.set_field_ref_value(field_ref, value, span),
        }
    }

    fn get_field_ref_value(
        &mut self,
        field_ref: &VmFieldRef,
        span: Span,
    ) -> Result<VmValue, VerseError> {
        let object = Rc::as_ptr(&field_ref.fields) as usize;
        let (predicts, owner_class, name, value) = {
            let fields = field_ref.fields.borrow();
            let Some(field) = fields.get(field_ref.index) else {
                return Err(VerseError::runtime_at(
                    "class field ref index out of range",
                    span,
                ));
            };
            (
                field.predicts,
                field.owner_class.clone(),
                field.name.clone(),
                field.value.clone(),
            )
        };
        let value = if predicts {
            self.host
                .prediction_value(object, &owner_class, &name, &value)
        } else {
            copy_runtime_value(&value)
        };
        Ok(VmValue::Runtime(value))
    }

    fn set_field_ref_value(
        &mut self,
        field_ref: &VmFieldRef,
        value: VmValue,
        span: Span,
    ) -> Result<(), VerseError> {
        let value = into_runtime(value, span)?;
        let object = Rc::as_ptr(&field_ref.fields) as usize;
        let mut fields = field_ref.fields.borrow_mut();
        let Some(field) = fields.get_mut(field_ref.index) else {
            return Err(VerseError::runtime_at(
                "class field ref index out of range",
                span,
            ));
        };
        if field.predicts {
            self.host
                .set_prediction_value(object, &field.owner_class, &field.name, value);
        } else {
            field.value = value;
        }
        Ok(())
    }

    fn runtime_values_from_operands(
        &self,
        frame: &Frame<'program>,
        operands: &[ValueOperand],
        span: Span,
    ) -> Result<Vec<Value>, VerseError> {
        operands
            .iter()
            .map(|operand| self.get_runtime_operand(frame, *operand, span))
            .collect()
    }

    fn call(
        &mut self,
        callee: VmValue,
        args: Vec<VmValue>,
        named_args: Vec<(String, VmValue)>,
        span: Span,
        current_task: Option<Rc<RuntimeTask>>,
    ) -> Result<CallOutcome<'program>, VerseError> {
        match callee {
            VmValue::Function(function) => {
                if !named_args.is_empty() {
                    return Err(VerseError::runtime_at(
                        "bytecode function calls should lower named arguments before runtime",
                        span,
                    ));
                }
                let descriptor =
                    self.program
                        .functions()
                        .get(function.function)
                        .ok_or_else(|| {
                            VerseError::runtime_at("bytecode function index out of range", span)
                        })?;
                if descriptor.params().len() != args.len() {
                    let name = descriptor.name().unwrap_or("<anonymous>");
                    return Err(VerseError::runtime_at(
                        format!(
                            "`{name}` expected {} arguments, got {}",
                            descriptor.params().len(),
                            args.len()
                        ),
                        span,
                    ));
                }
                if let Some(native) = descriptor.injected_native() {
                    return self.call_injected_native(native, args, span);
                }
                let chunk = descriptor.chunk();
                let external_return_type = descriptor.external_return_type().cloned();
                let source_params = if external_return_type.is_some() {
                    descriptor.source_params().to_vec()
                } else {
                    Vec::new()
                };
                let return_args = external_return_type.is_some().then(|| args.clone());
                match self.run_chunk(chunk, args, span, current_task, function.captures.clone())? {
                    ChunkOutcome::Value(value) => Ok(CallOutcome::Value(
                        self.materialize_external_function_return(
                            external_return_type.as_ref(),
                            &source_params,
                            return_args.as_deref(),
                            value,
                        ),
                    )),
                    ChunkOutcome::Failure => Ok(CallOutcome::Failure),
                    ChunkOutcome::Suspended(suspension) => Ok(CallOutcome::Suspended(suspension)),
                }
            }
            VmValue::BoundMethod(method) => {
                let (candidate, mut user_args) =
                    select_bound_method_call(&method, args, named_args, span)?;
                let mut call_args = Vec::with_capacity(1 + candidate.field_count + user_args.len());
                call_args.push(VmValue::Runtime(method.self_value.clone()));
                for index in 0..candidate.field_count {
                    call_args.push(VmValue::FieldRef(VmFieldRef {
                        fields: method.fields.clone(),
                        index,
                    }));
                }
                call_args.append(&mut user_args);
                let descriptor = self
                    .program
                    .functions()
                    .get(candidate.function)
                    .ok_or_else(|| {
                        VerseError::runtime_at("bytecode method index out of range", span)
                    })?;
                match self.run_chunk(descriptor.chunk(), call_args, span, current_task, None)? {
                    ChunkOutcome::Value(value) => Ok(CallOutcome::Value(
                        self.materialize_external_bound_method_return(
                            &method.self_value,
                            candidate,
                            value,
                        ),
                    )),
                    ChunkOutcome::Failure => Ok(CallOutcome::Failure),
                    ChunkOutcome::Suspended(suspension) => Ok(CallOutcome::Suspended(suspension)),
                }
            }
            VmValue::NumberMethod(method) => {
                if !named_args.is_empty() {
                    return Err(VerseError::runtime_at(
                        format!("`{}` does not accept named arguments", method.name),
                        span,
                    ));
                }
                let args = args
                    .into_iter()
                    .map(|arg| into_runtime(arg, span))
                    .collect::<Result<Vec<_>, _>>()?;
                call_number_method(method, args, span).map(|value| {
                    value
                        .map(VmValue::Runtime)
                        .map_or(CallOutcome::Failure, CallOutcome::Value)
                })
            }
            VmValue::Runtime(value) => {
                if let Value::NativeArrayMethod { name, receiver } = value {
                    if !named_args.is_empty() {
                        return Err(VerseError::runtime_at(
                            format!("`{name}` does not accept named arguments"),
                            span,
                        ));
                    }
                    let args = args
                        .into_iter()
                        .map(|arg| into_runtime(arg, span))
                        .collect::<Result<Vec<_>, _>>()?;
                    return bytecode_call_native_array_method(&receiver, &name, args, span).map(
                        |value| {
                            value
                                .map(VmValue::Runtime)
                                .map_or(CallOutcome::Failure, CallOutcome::Value)
                        },
                    );
                }
                if let Value::NativeResultMethod { name, result } = value {
                    if !named_args.is_empty() {
                        return Err(VerseError::runtime_at(
                            format!("`{name}` does not accept named arguments"),
                            span,
                        ));
                    }
                    if !args.is_empty() {
                        return Err(VerseError::runtime_at(
                            format!("`{name}` expected 0 arguments, got {}", args.len()),
                            span,
                        ));
                    }
                    let Value::Result { succeeded, value } = *result else {
                        return Err(VerseError::runtime_at(
                            format!("`{name}` expected a result receiver"),
                            span,
                        ));
                    };
                    return match (name, succeeded) {
                        ("GetSuccess", true) | ("GetError", false) => Ok(CallOutcome::Value(
                            VmValue::Runtime(copy_runtime_value(&value)),
                        )),
                        ("GetSuccess", false) | ("GetError", true) => Ok(CallOutcome::Failure),
                        _ => Err(VerseError::runtime_at(
                            format!("unknown result method `{name}`"),
                            span,
                        )),
                    };
                }
                if let Value::ExternalFunction {
                    params,
                    return_type,
                    ..
                } = value
                {
                    if !named_args.is_empty() {
                        return Err(VerseError::runtime_at(
                            "external function calls should lower named arguments before runtime",
                            span,
                        ));
                    }
                    if params.len() != args.len() {
                        return Err(VerseError::runtime_at(
                            format!(
                                "external function expected {} arguments, got {}",
                                params.len(),
                                args.len()
                            ),
                            span,
                        ));
                    }
                    let _args = args
                        .into_iter()
                        .map(|arg| into_runtime(arg, span))
                        .collect::<Result<Vec<_>, _>>()?;
                    let mut visiting = Vec::new();
                    let value = self.program_external_return_value(&return_type, &mut visiting);
                    return Ok(CallOutcome::Value(VmValue::Runtime(value)));
                }
                if let Value::NativeFunction {
                    name,
                    arity,
                    decides,
                    function,
                } = value
                {
                    if matches!(
                        name,
                        "__verse_sync" | "__verse_race" | "__verse_rush" | "__verse_branch"
                    ) {
                        return self.call_structured_concurrency(
                            name,
                            args,
                            named_args,
                            span,
                            current_task,
                        );
                    }
                    if name == "__verse_begin_defer_scope" || name == "__verse_end_defer_scope" {
                        if !named_args.is_empty() || !args.is_empty() {
                            return Err(VerseError::runtime_at(
                                format!("`{name}` does not accept arguments"),
                                span,
                            ));
                        }
                        if name == "__verse_begin_defer_scope" {
                            self.begin_defer_scope(current_task.clone(), span)?;
                        } else {
                            self.end_defer_scope(current_task.clone(), span)?;
                        }
                        return Ok(CallOutcome::Value(VmValue::Runtime(Value::None)));
                    }
                    if name == "__verse_defer" {
                        if !named_args.is_empty() {
                            return Err(VerseError::runtime_at(
                                "`__verse_defer` does not accept named arguments",
                                span,
                            ));
                        }
                        let [procedure]: [VmValue; 1] =
                            args.try_into().map_err(|args: Vec<VmValue>| {
                                VerseError::runtime_at(
                                    format!(
                                        "`__verse_defer` expected 1 argument, got {}",
                                        args.len()
                                    ),
                                    span,
                                )
                            })?;
                        let VmValue::Function(function) = procedure else {
                            return Err(VerseError::runtime_at(
                                "`__verse_defer` expected a procedure operand",
                                span,
                            ));
                        };
                        self.push_native_defer(function, current_task.clone(), span)?;
                        return Ok(CallOutcome::Value(VmValue::Runtime(Value::None)));
                    }
                    if name == "GetSimulationElapsedTime" {
                        if !named_args.is_empty() {
                            return Err(VerseError::runtime_at(
                                "`GetSimulationElapsedTime` does not accept named arguments in bytecode VM",
                                span,
                            ));
                        }
                        if !args.is_empty() {
                            return Err(VerseError::runtime_at(
                                format!(
                                    "`GetSimulationElapsedTime` expected 0 arguments, got {}",
                                    args.len()
                                ),
                                span,
                            ));
                        }
                        return Ok(CallOutcome::Value(VmValue::Runtime(Value::Float(
                            self.host.now().as_secs_f64(),
                        ))));
                    }
                    if name == "Sleep" {
                        if !named_args.is_empty() {
                            return Err(VerseError::runtime_at(
                                "`Sleep` does not accept named arguments in bytecode VM",
                                span,
                            ));
                        }
                        let [seconds]: [VmValue; 1] =
                            args.try_into().map_err(|args: Vec<VmValue>| {
                                VerseError::runtime_at(
                                    format!("`Sleep` expected 1 argument, got {}", args.len()),
                                    span,
                                )
                            })?;
                        let seconds = sleep_seconds(into_runtime(seconds, span)?, span)?;
                        if seconds < 0.0 {
                            return Ok(CallOutcome::Value(VmValue::Runtime(Value::None)));
                        }
                        if let Some(task) = current_task.clone() {
                            let id = task_id(&task);
                            if seconds == 0.0 {
                                self.ready_tasks.push_back((id, Value::None));
                            } else if seconds.is_finite() {
                                self.arm_task_timer(id, seconds);
                            }
                        }
                        return Ok(CallOutcome::Yield);
                    }
                    if name == "FitsInPlayerMap"
                        && args.iter().any(|arg| matches!(arg, VmValue::Function(_)))
                    {
                        return Ok(CallOutcome::Failure);
                    }
                    let args = args
                        .into_iter()
                        .map(|arg| into_runtime(arg, span))
                        .collect::<Result<Vec<_>, _>>()?;
                    let named_args = named_args
                        .into_iter()
                        .map(|(name, value)| {
                            into_runtime(value, span).map(|value| (name, value, span))
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    return bytecode_call_native_function_named(
                        name, arity, decides, function, args, named_args, span,
                    )
                    .map(|value| {
                        value
                            .map(VmValue::Runtime)
                            .map_or(CallOutcome::Failure, CallOutcome::Value)
                    });
                }
                if let Value::NativeEventMethod {
                    name,
                    payload,
                    waiters,
                    subscribers,
                    sticky_signal,
                } = value
                {
                    if !named_args.is_empty() {
                        return Err(VerseError::runtime_at(
                            format!("`{name}` does not accept named arguments"),
                            span,
                        ));
                    }
                    let args = args
                        .into_iter()
                        .map(|arg| into_runtime(arg, span).map(|value| (value, span)))
                        .collect::<Result<Vec<_>, _>>()?;
                    if name == "Await" {
                        let value = bytecode_call_native_event_method(
                            name,
                            payload,
                            waiters.clone(),
                            sticky_signal,
                            args,
                            span,
                        )?;
                        if matches!(value, Value::Pending) {
                            if let Some(waiters) = waiters
                                && let Some(task) = current_task.clone()
                            {
                                waiters.borrow_mut().push(task);
                                return Ok(CallOutcome::Yield);
                            }
                            if let Some(task) = current_task.clone() {
                                self.arm_task_future(
                                    task_id(&task),
                                    Box::pin(std::future::pending::<Value>()),
                                );
                                return Ok(CallOutcome::Yield);
                            }
                        }
                        return Ok(CallOutcome::Value(VmValue::Runtime(value)));
                    }
                    if matches!(name, "Signal" | "Broadcast") {
                        let signal_value =
                            bytecode_event_signal_payload(payload.as_ref(), args.clone());
                        let value = bytecode_call_native_event_method(
                            name,
                            payload,
                            waiters.clone(),
                            sticky_signal,
                            args,
                            span,
                        )?;
                        if let Some(waiters) = waiters {
                            let waiters = std::mem::take(&mut *waiters.borrow_mut());
                            let resumed_any = !waiters.is_empty();
                            for task in waiters {
                                self.ready_tasks
                                    .push_back((task_id(&task), copy_runtime_value(&signal_value)));
                            }
                            if let Some(subscribers) = subscribers.clone() {
                                self.run_subscription_callbacks(
                                    subscribers,
                                    copy_runtime_value(&signal_value),
                                    span,
                                )?;
                            }
                            if resumed_any && let Some(task) = current_task.clone() {
                                let id = task_id(&task);
                                self.detach_task_from_parent(id);
                                self.ready_tasks.push_back((id, Value::None));
                                return Ok(CallOutcome::Yield);
                            }
                            self.run_ready_tasks(span)?;
                        } else if let Some(subscribers) = subscribers {
                            self.run_subscription_callbacks(subscribers, signal_value, span)?;
                        }
                        return Ok(CallOutcome::Value(VmValue::Runtime(value)));
                    }
                    return bytecode_call_native_event_method(
                        name,
                        payload,
                        waiters,
                        sticky_signal,
                        args,
                        span,
                    )
                    .map(|value| {
                        if name == "IsSignaled" && matches!(value, Value::Option(None)) {
                            CallOutcome::Failure
                        } else {
                            CallOutcome::Value(VmValue::Runtime(value))
                        }
                    });
                }
                if let Value::NativeSubscribableMethod {
                    name,
                    payload,
                    subscribers,
                    next_subscriber_id,
                } = value
                {
                    if !named_args.is_empty() {
                        return Err(VerseError::runtime_at(
                            format!("`{name}` does not accept named arguments"),
                            span,
                        ));
                    }
                    let expected_arity = usize::from(payload.is_some());
                    let callback_accepts_arity = args
                        .first()
                        .is_some_and(|arg| self.vm_callable_accepts_arity(arg, expected_arity));
                    let callback = match args.first() {
                        Some(VmValue::Runtime(value)) => Some(value.clone()),
                        _ => None,
                    };
                    let callback_value = args.first().cloned();
                    let expected_arity = usize::from(payload.is_some());
                    let value = bytecode_call_native_subscribable_method(
                        name,
                        payload,
                        subscribers.clone(),
                        next_subscriber_id,
                        callback_accepts_arity,
                        callback,
                        args.len(),
                        span,
                    )?;
                    if let Value::SubscriptionCancelHandle { subscriber_id, .. } = &value
                        && let Some(callback) = callback_value
                    {
                        self.register_subscription_callback(
                            &subscribers,
                            *subscriber_id,
                            expected_arity,
                            callback,
                        );
                        self.arm_subscription_signal_future(subscribers);
                    }
                    return Ok(CallOutcome::Value(VmValue::Runtime(value)));
                }
                if let Value::NativeSubscriptionCancelMethod {
                    name,
                    subscribers,
                    subscriber_id,
                } = value
                {
                    if !named_args.is_empty() {
                        return Err(VerseError::runtime_at(
                            format!("`{name}` does not accept named arguments"),
                            span,
                        ));
                    }
                    let value = bytecode_call_native_subscription_cancel_method(
                        name,
                        subscribers.clone(),
                        subscriber_id,
                        args.len(),
                        span,
                    )?;
                    self.remove_subscription_callback(&subscribers, subscriber_id);
                    if subscribers.borrow().is_empty() {
                        self.clear_subscription_signal_token(&subscribers);
                    }
                    return Ok(CallOutcome::Value(VmValue::Runtime(value)));
                }
                if let Value::NativeTaskMethod { name, task } = value {
                    if !named_args.is_empty() {
                        return Err(VerseError::runtime_at(
                            format!("`{name}` does not accept named arguments"),
                            span,
                        ));
                    }
                    match name {
                        "Await" => {
                            if !args.is_empty() {
                                return Err(VerseError::runtime_at(
                                    format!("`Await` expected 0 arguments, got {}", args.len()),
                                    span,
                                ));
                            }
                            if let Some(value) = self.task_result(&task, span)? {
                                return Ok(CallOutcome::Value(VmValue::Runtime(value)));
                            }
                            return self.wait_for_pending_tasks(&[(0, task)], current_task, span);
                        }
                        "Cancel" => {
                            if !args.is_empty() {
                                return Err(VerseError::runtime_at(
                                    format!("`Cancel` expected 0 arguments, got {}", args.len()),
                                    span,
                                ));
                            }
                            self.cancel_bytecode_task(task_id(&task), span)?;
                            return Ok(CallOutcome::Value(VmValue::Runtime(Value::None)));
                        }
                        _ => {
                            return Err(VerseError::runtime_at(
                                format!("unknown task method `{name}`"),
                                span,
                            ));
                        }
                    }
                }
                if let Value::NativeCancelMethod {
                    name,
                    entries,
                    entry_id,
                } = value
                {
                    if !named_args.is_empty() {
                        return Err(VerseError::runtime_at(
                            format!("`{name}` does not accept named arguments"),
                            span,
                        ));
                    }
                    return bytecode_call_native_cancel_method(
                        name,
                        entries,
                        entry_id,
                        args.len(),
                        span,
                    )
                    .map(|value| CallOutcome::Value(VmValue::Runtime(value)));
                }
                if let Value::NativeModifierMethod { name, receiver } = value {
                    if !named_args.is_empty() {
                        return Err(VerseError::runtime_at(
                            format!("`{name}` does not accept named arguments"),
                            span,
                        ));
                    }
                    let receiver = *receiver;
                    let args = args
                        .into_iter()
                        .map(|arg| into_runtime(arg, span).map(|value| (value, span)))
                        .collect::<Result<Vec<_>, _>>()?;
                    let value = match (name, receiver) {
                        ("Evaluate", Value::Modifier { .. }) => {
                            let [(value, _)]: [(Value, Span); 1] =
                                args.try_into().map_err(|args: Vec<(Value, Span)>| {
                                    VerseError::runtime_at(
                                        format!(
                                            "`Evaluate` expected 1 arguments, got {}",
                                            args.len()
                                        ),
                                        span,
                                    )
                                })?;
                            value
                        }
                        ("Evaluate", stack @ Value::ModifierStack { .. }) => {
                            let [(value, _)]: [(Value, Span); 1] =
                                args.try_into().map_err(|args: Vec<(Value, Span)>| {
                                    VerseError::runtime_at(
                                        format!(
                                            "`Evaluate` expected 1 arguments, got {}",
                                            args.len()
                                        ),
                                        span,
                                    )
                                })?;
                            self.evaluate_modifier_stack(stack, value, span)?
                        }
                        ("AddModifier", stack @ Value::ModifierStack { .. }) => {
                            bytecode_modifier_stack_add(stack, args, span)?
                        }
                        (method, receiver) => {
                            return Err(VerseError::runtime_at(
                                format!(
                                    "value `{receiver}` has no native modifier method `{method}`"
                                ),
                                span,
                            ));
                        }
                    };
                    return Ok(CallOutcome::Value(VmValue::Runtime(value)));
                }
                if !named_args.is_empty() {
                    return Err(VerseError::runtime_at(
                        "non-function bytecode call does not accept named arguments",
                        span,
                    ));
                }
                if let Some(target) = self.runtime_cast_target(&value) {
                    let [candidate] = args.as_slice() else {
                        return Err(VerseError::runtime_at(
                            "type value cast expected one argument",
                            span,
                        ));
                    };
                    let candidate = into_runtime(candidate.clone(), span)?;
                    return Ok(
                        if self.runtime_value_matches_cast_target(&candidate, &target) {
                            CallOutcome::Value(VmValue::Runtime(candidate))
                        } else {
                            CallOutcome::Failure
                        },
                    );
                }
                let [index] = args.as_slice() else {
                    return Err(VerseError::runtime_at(
                        "non-function bytecode call expected one argument",
                        span,
                    ));
                };
                let index = into_runtime(index.clone(), span)?;
                self.index_value_failable(value, index, span).map(|value| {
                    value
                        .map(VmValue::Runtime)
                        .map_or(CallOutcome::Failure, CallOutcome::Value)
                })
            }
            VmValue::Ref(_) | VmValue::FieldRef(_) => Err(VerseError::runtime_at(
                "cannot call bytecode ref without RefGet",
                span,
            )),
            VmValue::Semaphore(_) => Err(VerseError::runtime_at(
                "cannot call bytecode semaphore",
                span,
            )),
            VmValue::Scope(_) => Err(VerseError::runtime_at("cannot call bytecode scope", span)),
            VmValue::Option(_) => Err(VerseError::runtime_at(
                "cannot call bytecode option without query",
                span,
            )),
            VmValue::Placeholder(_) => Err(VerseError::runtime_at(
                "cannot call unresolved bytecode placeholder",
                span,
            )),
            VmValue::Uninitialized => Err(VerseError::runtime_at(
                "cannot call uninitialized bytecode value",
                span,
            )),
        }
    }

    fn call_injected_native(
        &mut self,
        native: &InjectedNativeFunction,
        args: Vec<VmValue>,
        span: Span,
    ) -> Result<CallOutcome<'program>, VerseError> {
        let args = args
            .into_iter()
            .map(|arg| into_runtime(arg, span))
            .collect::<Result<Vec<_>, _>>()?;
        let Some(result) =
            self.native_registry
                .call(&native.runtime_name, native.arity, args, span)
        else {
            return Err(VerseError::runtime_at(
                format!(
                    "native function `{}`/{} was declared but no host implementation was registered",
                    native.runtime_name, native.arity
                ),
                span,
            ));
        };
        match result {
            NativeCallResult::Value(value) => Ok(CallOutcome::Value(VmValue::Runtime(value))),
            NativeCallResult::Failure(reason) if native.decides() => {
                let _ = reason;
                Ok(CallOutcome::Failure)
            }
            NativeCallResult::Failure(reason) => Err(VerseError::runtime_at(
                format!("native function `{}` failed: {reason}", native.runtime_name),
                span,
            )),
            NativeCallResult::RuntimeError(message) => Err(VerseError::runtime_at(
                format!(
                    "native function `{}` errored: {message}",
                    native.runtime_name
                ),
                span,
            )),
        }
    }

    fn vm_callable_accepts_arity(&self, value: &VmValue, expected_arity: usize) -> bool {
        match value {
            VmValue::Function(function) => self
                .program
                .functions()
                .get(function.function)
                .is_some_and(|descriptor| descriptor.params().len() == expected_arity),
            VmValue::BoundMethod(method) => method
                .candidates
                .iter()
                .any(|candidate| candidate.params.len() == expected_arity),
            VmValue::NumberMethod(method) => match method.name {
                "IsFinite" => expected_arity == 0,
                "IsAlmostZero" => expected_arity == 1,
                _ => false,
            },
            VmValue::Runtime(Value::ExternalFunction { params, .. }) => {
                params.len() == expected_arity
            }
            VmValue::Runtime(Value::External) => true,
            VmValue::Runtime(Value::NativeFunction { arity, .. }) => {
                arity.is_none_or(|arity| arity == expected_arity)
            }
            VmValue::Runtime(_)
            | VmValue::Ref(_)
            | VmValue::FieldRef(_)
            | VmValue::Scope(_)
            | VmValue::Option(_)
            | VmValue::Semaphore(_)
            | VmValue::Placeholder(_)
            | VmValue::Uninitialized => false,
        }
    }

    fn class_instance_is_a(&self, actual: &str, expected: &str) -> bool {
        let mut current = Some(actual.to_string());
        while let Some(class_name) = current {
            if runtime_type_names_match(&class_name, expected) {
                return true;
            }
            current = self
                .program_class_by_runtime_name(&class_name)
                .and_then(|class| class.base_class().map(str::to_string));
        }
        false
    }

    fn class_instance_implements_interface(&self, actual: &str, expected: &str) -> bool {
        let mut current = Some(actual.to_string());
        while let Some(class_name) = current {
            let Some(class) = self.program_class_by_runtime_name(&class_name) else {
                return false;
            };
            if class
                .interfaces()
                .iter()
                .any(|interface| runtime_type_names_match(interface, expected))
            {
                return true;
            }
            current = class.base_class().map(str::to_string);
        }
        false
    }

    fn program_class_by_runtime_name(&self, name: &str) -> Option<&ClassDescriptor> {
        self.program.class(name).or_else(|| {
            self.program
                .classes()
                .iter()
                .rev()
                .find(|class| runtime_type_names_match(class.name(), name))
        })
    }

    fn program_interface_by_runtime_name(&self, name: &str) -> Option<&InterfaceDescriptor> {
        self.program.interface(name).or_else(|| {
            self.program
                .interfaces()
                .iter()
                .rev()
                .find(|interface| runtime_type_names_match(interface.name(), name))
        })
    }

    fn materialize_external_bound_method_return(
        &self,
        self_value: &Value,
        candidate: &VmBoundMethodCandidate,
        value: VmValue,
    ) -> VmValue {
        let Some(return_type) = candidate.external_return_type.as_ref() else {
            return value;
        };
        if !matches!(value, VmValue::Runtime(_)) {
            return value;
        }
        let return_type = self
            .bound_method_return_substitutions(self_value, candidate)
            .map(|substitutions| substitute_runtime_type_name_params(return_type, &substitutions))
            .unwrap_or_else(|| return_type.clone());
        let mut visiting = Vec::new();
        VmValue::Runtime(self.program_external_return_value(&return_type, &mut visiting))
    }

    fn materialize_external_function_return(
        &self,
        external_return_type: Option<&TypeName>,
        source_params: &[Param],
        args: Option<&[VmValue]>,
        value: VmValue,
    ) -> VmValue {
        let Some(return_type) = external_return_type else {
            return value;
        };
        if !matches!(value, VmValue::Runtime(_)) {
            return value;
        }
        let return_type = args
            .and_then(|args| self.function_return_substitutions(source_params, args))
            .map(|substitutions| substitute_runtime_type_name_params(return_type, &substitutions))
            .unwrap_or_else(|| return_type.clone());
        let mut visiting = Vec::new();
        VmValue::Runtime(self.program_external_return_value(&return_type, &mut visiting))
    }

    fn function_return_substitutions(
        &self,
        source_params: &[Param],
        args: &[VmValue],
    ) -> Option<HashMap<String, TypeName>> {
        if source_params.len() != args.len() {
            return None;
        }
        let mut substitutions = HashMap::new();
        for (param, arg) in source_params.iter().zip(args) {
            let Some(annotation) = param.annotation.as_ref() else {
                continue;
            };
            if type_value_parameter_annotation(&annotation.name)
                && let VmValue::Runtime(value) = arg
                && let Some(type_name) = runtime_type_value_payload(value)
            {
                substitutions.insert(param.name.clone(), type_name);
            }
            let capture_names = param
                .type_params
                .iter()
                .map(|type_param| type_param.name.as_str())
                .collect::<HashSet<_>>();
            if capture_names.is_empty() {
                continue;
            }
            if let VmValue::Runtime(value) = arg {
                if let Some(actual_type) = runtime_value_type_name(value) {
                    if let TypeName::Named(name) = &annotation.name
                        && capture_names.contains(name.as_str())
                    {
                        record_type_name_substitution(name, &actual_type, &mut substitutions);
                    }
                    for type_param in &param.type_params {
                        if let Some(actual) = substitutions.get(&type_param.name).cloned() {
                            infer_type_param_constraint_substitutions(
                                &type_param.constraint,
                                &actual,
                                &capture_names,
                                &mut substitutions,
                            );
                        }
                    }
                }
                self.infer_runtime_type_name_substitutions(
                    &annotation.name,
                    value,
                    &capture_names,
                    &mut substitutions,
                );
            }
        }
        (!substitutions.is_empty()).then_some(substitutions)
    }

    fn infer_runtime_type_name_substitutions(
        &self,
        expected: &TypeName,
        actual_value: &Value,
        capture_names: &HashSet<&str>,
        substitutions: &mut HashMap<String, TypeName>,
    ) {
        let Some(actual_type) = runtime_value_type_name(actual_value) else {
            return;
        };
        infer_type_name_substitutions(expected, &actual_type, false, capture_names, substitutions);

        let Value::ClassInstance { class_name, .. } = actual_value else {
            return;
        };
        let Some((actual_name, actual_args)) =
            runtime_aggregate_type_name_parts(&parse_runtime_type_name(class_name))
        else {
            return;
        };
        let Some(class) = self.program_class_by_runtime_name(&actual_name) else {
            return;
        };
        let owner_substitutions = class
            .type_params()
            .iter()
            .cloned()
            .zip(actual_args)
            .collect::<HashMap<_, _>>();
        for interface in class.interfaces() {
            let interface_type = substitute_runtime_type_name_params(
                &parse_runtime_type_name(interface),
                &owner_substitutions,
            );
            infer_type_name_substitutions(
                expected,
                &interface_type,
                false,
                capture_names,
                substitutions,
            );
        }
    }

    fn bound_method_return_substitutions(
        &self,
        self_value: &Value,
        candidate: &VmBoundMethodCandidate,
    ) -> Option<HashMap<String, TypeName>> {
        let Value::ClassInstance { class_name, .. } = self_value else {
            return None;
        };
        let args = runtime_type_args(class_name)?;
        let substitutions = candidate
            .owner_type_params
            .iter()
            .cloned()
            .zip(args)
            .collect::<HashMap<_, _>>();
        (!substitutions.is_empty()).then_some(substitutions)
    }

    fn program_external_return_value(
        &self,
        type_name: &TypeName,
        visiting: &mut Vec<String>,
    ) -> Value {
        if let Some(value) = self.external_aggregate_return_value(type_name, visiting) {
            return value;
        }
        if let Some(value) = self.external_interface_return_value(type_name, visiting) {
            return value;
        }
        match type_name {
            TypeName::Tuple(items) => Value::Tuple(
                items
                    .iter()
                    .map(|item| self.program_external_return_value(item, visiting))
                    .collect(),
            ),
            TypeName::Applied { name, args } if name == "result" && args.len() == 2 => {
                Value::Result {
                    succeeded: true,
                    value: Box::new(self.program_external_return_value(&args[0], visiting)),
                }
            }
            TypeName::Applied { name, args } if name == "success_result" && args.len() == 1 => {
                Value::Result {
                    succeeded: true,
                    value: Box::new(self.program_external_return_value(&args[0], visiting)),
                }
            }
            TypeName::Applied { name, args } if name == "error_result" && args.len() == 1 => {
                Value::Result {
                    succeeded: false,
                    value: Box::new(self.program_external_return_value(&args[0], visiting)),
                }
            }
            _ => bytecode_external_return_value(type_name),
        }
    }

    fn external_aggregate_return_value(
        &self,
        type_name: &TypeName,
        visiting: &mut Vec<String>,
    ) -> Option<Value> {
        let (name, args) = runtime_aggregate_type_name_parts(type_name)?;
        let class = self.program_class_by_runtime_name(&name)?;
        let runtime_name = rendered_runtime_aggregate_name(class.name(), &args)?;
        if visiting.iter().any(|name| name == &runtime_name) {
            return None;
        }
        visiting.push(runtime_name.clone());
        let fields =
            self.external_descriptor_fields(class.fields(), class.type_params(), &args, visiting);
        visiting.pop();
        Some(self.class_instance_value(runtime_name, class.unique(), fields))
    }

    fn external_interface_return_value(
        &self,
        type_name: &TypeName,
        visiting: &mut Vec<String>,
    ) -> Option<Value> {
        let (name, args) = runtime_aggregate_type_name_parts(type_name)?;
        let interface = self.program_interface_by_runtime_name(&name)?;
        let runtime_name = rendered_runtime_aggregate_name(interface.name(), &args)?;
        if visiting.iter().any(|name| name == &runtime_name) {
            return None;
        }
        visiting.push(runtime_name.clone());
        let fields = self.external_descriptor_fields(
            interface.fields(),
            interface.type_params(),
            &args,
            visiting,
        );
        visiting.pop();
        Some(self.class_instance_value(runtime_name, false, fields))
    }

    fn external_descriptor_fields(
        &self,
        fields: &[FieldDescriptor],
        type_params: &[String],
        args: &[TypeName],
        visiting: &mut Vec<String>,
    ) -> Vec<(String, bool, Value)> {
        let substitutions = type_params
            .iter()
            .cloned()
            .zip(args.iter().cloned())
            .collect::<HashMap<_, _>>();
        fields
            .iter()
            .map(|field| {
                let field_type = if substitutions.is_empty() {
                    field.type_name().clone()
                } else {
                    substitute_runtime_type_name_params(field.type_name(), &substitutions)
                };
                (
                    field.name().to_string(),
                    field.mutable(),
                    self.program_external_return_value(&field_type, visiting),
                )
            })
            .collect()
    }

    fn runtime_cast_target(&self, value: &Value) -> Option<RuntimeCastTarget> {
        match value {
            Value::ClassType { name, .. } => Some(RuntimeCastTarget::Class(name.clone())),
            Value::InterfaceType { name, .. } => Some(RuntimeCastTarget::Interface(name.clone())),
            Value::Type(TypeName::Named(name))
                if self.program_class_by_runtime_name(name).is_some() =>
            {
                Some(RuntimeCastTarget::Class(name.clone()))
            }
            Value::Type(TypeName::Named(name)) => Some(RuntimeCastTarget::Interface(name.clone())),
            _ => None,
        }
    }

    fn runtime_value_matches_cast_target(&self, value: &Value, target: &RuntimeCastTarget) -> bool {
        let Value::ClassInstance { class_name, .. } = value else {
            return false;
        };
        match target {
            RuntimeCastTarget::Class(name) => self.class_instance_is_a(class_name, name),
            RuntimeCastTarget::Interface(name) => {
                self.class_instance_implements_interface(class_name, name)
            }
        }
    }

    fn index_value_failable(
        &self,
        collection: Value,
        index: Value,
        span: Span,
    ) -> Result<Option<Value>, VerseError> {
        if let Some(target) = self.runtime_cast_target(&collection) {
            return Ok(if self.runtime_value_matches_cast_target(&index, &target) {
                Some(index)
            } else {
                None
            });
        }
        index_value_failable(collection, index, span)
    }

    fn evaluate_modifier_stack(
        &mut self,
        stack: Value,
        input: Value,
        span: Span,
    ) -> Result<Value, VerseError> {
        let Some((_item_type, modifiers)) = bytecode_modifier_stack_ordered_modifiers(&stack)
        else {
            return Err(VerseError::runtime_at(
                "Evaluate expected modifier_stack receiver",
                span,
            ));
        };
        let mut value = input;
        for modifier in modifiers {
            value = self.evaluate_modifier_value(modifier, value, span)?;
        }
        Ok(value)
    }

    fn evaluate_modifier_value(
        &mut self,
        modifier: Value,
        input: Value,
        span: Span,
    ) -> Result<Value, VerseError> {
        match modifier {
            Value::Modifier { .. } => Ok(input),
            stack @ Value::ModifierStack { .. } => self.evaluate_modifier_stack(stack, input, span),
            other => {
                let callee = if let Some(method) = self.bound_method_value(&other, None, "Evaluate")
                {
                    VmValue::BoundMethod(method)
                } else if let Some(method) = bytecode_native_member_value(&other, "Evaluate") {
                    VmValue::Runtime(method)
                } else {
                    return Err(VerseError::runtime_at(
                        format!("modifier value `{other}` has no Evaluate method"),
                        span,
                    ));
                };
                match self.call(
                    callee,
                    vec![VmValue::Runtime(input)],
                    Vec::new(),
                    span,
                    None,
                )? {
                    CallOutcome::Value(value) => into_runtime(value, span),
                    CallOutcome::Failure => {
                        Err(VerseError::runtime_at("modifier Evaluate failed", span))
                    }
                    CallOutcome::Yield | CallOutcome::Block(_) | CallOutcome::Suspended(_) => Err(
                        VerseError::runtime_at("modifier Evaluate suspended in bytecode VM", span),
                    ),
                }
            }
        }
    }

    fn jump_to_failure(frame: &mut Frame<'program>, target: usize) {
        if frame
            .failure_stack
            .last()
            .is_some_and(|context| context.on_failure == target)
        {
            let context = frame
                .failure_stack
                .pop()
                .expect("failure context should exist after last check");
            context.transaction.restore(frame);
        }
        frame.ip = target;
    }

    fn fail_current_context(frame: &mut Frame<'program>) -> bool {
        let Some(context) = frame.failure_stack.pop() else {
            return false;
        };
        let target = context.on_failure;
        context.transaction.restore(frame);
        frame.ip = target;
        true
    }
}

enum CallOutcome<'program> {
    Value(VmValue),
    Failure,
    Yield,
    Block(usize),
    Suspended(BytecodeSuspension<'program>),
}

enum ChunkOutcome<'program> {
    Value(VmValue),
    Failure,
    Suspended(BytecodeSuspension<'program>),
}

fn bytecode_method_candidate(
    method: &ClassMethodDescriptor,
    owner_type_params: &[String],
) -> VmBoundMethodCandidate {
    VmBoundMethodCandidate {
        owner_type_params: owner_type_params.to_vec(),
        function: method.function(),
        params: method.params().to_vec(),
        param_types: method.param_types().to_vec(),
        external_return_type: method.external_return_type().cloned(),
        field_count: method.field_count(),
        decides: method.decides(),
    }
}

fn parse_qualified_member_name(name: &str) -> (Option<&str>, &str) {
    if let Some(rest) = name.strip_prefix('(')
        && let Some((qualifier, member)) = rest.split_once(":)")
    {
        return (Some(qualifier), member);
    }
    (None, name)
}

fn number_method_value(object: &Value, name: &str) -> Option<VmValue> {
    if !matches!(object, Value::Float(_)) {
        return None;
    }
    let name = match name {
        "IsFinite" => "IsFinite",
        "IsAlmostZero" => "IsAlmostZero",
        _ => return None,
    };
    Some(VmValue::NumberMethod(VmNumberMethod {
        name,
        receiver: copy_runtime_value(object),
    }))
}

fn call_number_method(
    method: VmNumberMethod,
    args: Vec<Value>,
    span: Span,
) -> Result<Option<Value>, VerseError> {
    match method.name {
        "IsFinite" => {
            if !args.is_empty() {
                return Err(VerseError::runtime_at(
                    format!("`IsFinite` expected 0 arguments, got {}", args.len()),
                    span,
                ));
            }
            let finite = expect_float_ref(&method.receiver, "`IsFinite` Val", span)?.is_finite();
            Ok(finite.then_some(method.receiver))
        }
        "IsAlmostZero" => {
            let [tolerance]: [Value; 1] = args.try_into().map_err(|args: Vec<Value>| {
                VerseError::runtime_at(
                    format!("`IsAlmostZero` expected 1 argument, got {}", args.len()),
                    span,
                )
            })?;
            let value = expect_float_ref(&method.receiver, "`IsAlmostZero` Val", span)?;
            let tolerance = expect_float_ref(&tolerance, "`IsAlmostZero` AbsoluteTolerance", span)?;
            Ok((value.abs() <= tolerance).then_some(Value::None))
        }
        _ => Err(VerseError::runtime_at(
            format!("unknown number method `{}`", method.name),
            span,
        )),
    }
}

fn expect_float_ref(value: &Value, context: &str, span: Span) -> Result<f64, VerseError> {
    match value {
        Value::Float(value) => Ok(*value),
        other => Err(VerseError::runtime_at(
            format!("{context} expected `float`, got {other}"),
            span,
        )),
    }
}

fn select_bound_method_call(
    method: &VmBoundMethod,
    args: Vec<VmValue>,
    named_args: Vec<(String, VmValue)>,
    span: Span,
) -> Result<(&VmBoundMethodCandidate, Vec<VmValue>), VerseError> {
    let argument_count = args.len() + named_args.len();
    for candidate in &method.candidates {
        if candidate.params.len() != argument_count {
            continue;
        }
        let user_args =
            reorder_bound_method_args(candidate, args.clone(), named_args.clone(), span)?;
        if bound_method_args_match_types(candidate, &user_args) {
            return Ok((candidate, user_args));
        }
    }
    Err(VerseError::runtime_at(
        format!(
            "`{}` expected one of [{}] arguments, got {}",
            method.name,
            method
                .candidates
                .iter()
                .map(|candidate| candidate.params.len().to_string())
                .collect::<Vec<_>>()
                .join(", "),
            argument_count
        ),
        span,
    ))
}

fn reorder_bound_method_args(
    candidate: &VmBoundMethodCandidate,
    args: Vec<VmValue>,
    named_args: Vec<(String, VmValue)>,
    span: Span,
) -> Result<Vec<VmValue>, VerseError> {
    let _callee_yields = candidate.decides;
    if named_args.is_empty() {
        return Ok(args);
    }

    let mut values = vec![None; candidate.params.len()];
    for (index, arg) in args.into_iter().enumerate() {
        let Some(slot) = values.get_mut(index) else {
            return Err(VerseError::runtime_at("too many method arguments", span));
        };
        if slot.is_some() {
            return Err(VerseError::runtime_at("duplicate method argument", span));
        }
        *slot = Some(arg);
    }

    for (name, value) in named_args {
        let Some(index) = candidate.params.iter().position(|param| param == &name) else {
            return Err(VerseError::runtime_at(
                format!("unknown method argument `{name}`"),
                span,
            ));
        };
        if values[index].is_some() {
            return Err(VerseError::runtime_at(
                format!("duplicate method argument `{name}`"),
                span,
            ));
        }
        values[index] = Some(value);
    }

    values
        .into_iter()
        .map(|value| value.ok_or_else(|| VerseError::runtime_at("missing method argument", span)))
        .collect()
}

fn bound_method_args_match_types(candidate: &VmBoundMethodCandidate, args: &[VmValue]) -> bool {
    candidate
        .param_types
        .iter()
        .zip(args)
        .all(|(expected, value)| vm_value_matches_type(value, expected.as_ref()))
}

fn vm_value_matches_type(value: &VmValue, expected: Option<&TypeName>) -> bool {
    let Some(expected) = expected else {
        return true;
    };
    match expected {
        TypeName::Function | TypeName::FunctionSignature { .. } => matches!(
            value,
            VmValue::Function(_)
                | VmValue::BoundMethod(_)
                | VmValue::NumberMethod(_)
                | VmValue::Runtime(
                    Value::NativeFunction { .. } | Value::ExternalFunction { .. } | Value::External,
                )
        ),
        _ => match value {
            VmValue::Runtime(value) => runtime_value_matches_type_name(value, expected),
            VmValue::Option(_) => matches!(expected, TypeName::Option(_)),
            _ => false,
        },
    }
}

fn runtime_type_names_match(left: &str, right: &str) -> bool {
    let left_erased = runtime_erased_type_name(left);
    let right_erased = runtime_erased_type_name(right);
    left == right
        || left_erased == right_erased
        || runtime_local_type_name(left) == right
        || runtime_local_type_name(right) == left
        || runtime_local_type_name(left_erased) == right_erased
        || runtime_local_type_name(right_erased) == left_erased
        || runtime_local_type_name(left_erased) == runtime_local_type_name(right_erased)
}

fn runtime_erased_type_name(name: &str) -> &str {
    name.split_once('(')
        .map(|(generic, _)| generic)
        .unwrap_or(name)
}

fn runtime_type_args(name: &str) -> Option<Vec<TypeName>> {
    let (_, rest) = name.split_once('(')?;
    let args = rest.strip_suffix(')')?;
    Some(
        split_runtime_type_args(args)
            .into_iter()
            .map(parse_runtime_type_name)
            .collect(),
    )
}

fn runtime_aggregate_type_name_parts(type_name: &TypeName) -> Option<(String, Vec<TypeName>)> {
    match type_name {
        TypeName::Applied { name, args } => Some((name.clone(), args.clone())),
        TypeName::Named(name) => parse_runtime_parametric_name(name)
            .map(|(head, args)| {
                (
                    head,
                    args.into_iter().map(parse_runtime_type_name).collect(),
                )
            })
            .or_else(|| Some((name.clone(), Vec::new()))),
        _ => None,
    }
}

fn rendered_runtime_aggregate_name(name: &str, args: &[TypeName]) -> Option<String> {
    if args.is_empty() {
        return Some(name.to_string());
    }
    let args = args
        .iter()
        .map(render_runtime_type_name_from_type_name)
        .collect::<Option<Vec<_>>>()?;
    Some(format!("{name}({})", args.join(", ")))
}

fn split_runtime_type_args(args: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut paren_depth = 0usize;
    let mut angle_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut start = 0usize;
    for (index, ch) in args.char_indices() {
        match ch {
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '<' => angle_depth += 1,
            '>' => angle_depth = angle_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            ',' if paren_depth == 0 && angle_depth == 0 && brace_depth == 0 => {
                items.push(args[start..index].trim());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    let tail = args[start..].trim();
    if !tail.is_empty() {
        items.push(tail);
    }
    items
}

fn parse_runtime_type_name(name: &str) -> TypeName {
    let name = name.trim();
    if let Some(item) = name.strip_prefix('?').filter(|item| !item.is_empty()) {
        return TypeName::Option(Box::new(parse_runtime_type_name(item)));
    }
    if let Some(items) = paren_wrapped_runtime_type(name, "option") {
        let items = split_runtime_type_args(items);
        if let [item] = items.as_slice() {
            return TypeName::Option(Box::new(parse_runtime_type_name(item)));
        }
    }
    if let Some(item) = angle_wrapped_runtime_type(name, "array") {
        return if item == "unknown" {
            TypeName::Array(None)
        } else {
            TypeName::Array(Some(Box::new(parse_runtime_type_name(item))))
        };
    }
    if let Some(items) = angle_wrapped_runtime_type(name, "map") {
        let items = split_runtime_type_args(items);
        if let [key, value] = items.as_slice() {
            return TypeName::Map(
                Box::new(parse_runtime_type_name(key)),
                Box::new(parse_runtime_type_name(value)),
            );
        }
    }
    if let Some(items) = angle_wrapped_runtime_type(name, "weak_map") {
        let items = split_runtime_type_args(items);
        if let [key, value] = items.as_slice() {
            return TypeName::WeakMap(
                Box::new(parse_runtime_type_name(key)),
                Box::new(parse_runtime_type_name(value)),
            );
        }
    }
    if let Some(items) = paren_wrapped_runtime_type(name, "tuple") {
        return TypeName::Tuple(
            split_runtime_type_args(items)
                .into_iter()
                .map(parse_runtime_type_name)
                .collect(),
        );
    }
    if let Some((head, args)) = parse_runtime_parametric_name(name) {
        return TypeName::Applied {
            name: head,
            args: args.into_iter().map(parse_runtime_type_name).collect(),
        };
    }
    TypeName::parse(name.to_string())
}

fn angle_wrapped_runtime_type<'a>(name: &'a str, head: &str) -> Option<&'a str> {
    name.strip_prefix(head)
        .and_then(|rest| rest.strip_prefix('<'))
        .and_then(|rest| rest.strip_suffix('>'))
}

fn paren_wrapped_runtime_type<'a>(name: &'a str, head: &str) -> Option<&'a str> {
    name.strip_prefix(head)
        .and_then(|rest| rest.strip_prefix('('))
        .and_then(|rest| rest.strip_suffix(')'))
}

fn parse_runtime_parametric_name(name: &str) -> Option<(String, Vec<&str>)> {
    let (head, rest) = name.split_once('(')?;
    let args = rest.strip_suffix(')')?;
    Some((head.to_string(), split_runtime_type_args(args)))
}

fn render_runtime_type_name_from_type_name(name: &TypeName) -> Option<String> {
    Some(match name {
        TypeName::Int => "int".to_string(),
        TypeName::Float => "float".to_string(),
        TypeName::Rational => "rational".to_string(),
        TypeName::Number => "number".to_string(),
        TypeName::Bool => "bool".to_string(),
        TypeName::String => "string".to_string(),
        TypeName::Message => "message".to_string(),
        TypeName::Char => "char".to_string(),
        TypeName::Char8 => "char8".to_string(),
        TypeName::Char32 => "char32".to_string(),
        TypeName::None => "none".to_string(),
        TypeName::Any => "any".to_string(),
        TypeName::Comparable => "comparable".to_string(),
        TypeName::Type => "type".to_string(),
        TypeName::TypeBounds { lower, upper } => format!(
            "type({}, {})",
            render_runtime_type_name_from_type_name(lower)?,
            render_runtime_type_name_from_type_name(upper)?
        ),
        TypeName::IntRange { min, max } => {
            format!("type{{_X:int where {min} <= _X, _X <= {max}}}")
        }
        TypeName::FloatRange(range) => format!(
            "type{{_X:float where {} <= _X, _X <= {}}}",
            range.min.render(),
            range.max.render()
        ),
        TypeName::Array(Some(item)) => {
            format!("array<{}>", render_runtime_type_name_from_type_name(item)?)
        }
        TypeName::Array(None) => "array<unknown>".to_string(),
        TypeName::Map(key, value) => format!(
            "map<{}, {}>",
            render_runtime_type_name_from_type_name(key)?,
            render_runtime_type_name_from_type_name(value)?
        ),
        TypeName::WeakMap(key, value) => format!(
            "weak_map<{}, {}>",
            render_runtime_type_name_from_type_name(key)?,
            render_runtime_type_name_from_type_name(value)?
        ),
        TypeName::Tuple(items) => {
            let items = items
                .iter()
                .map(render_runtime_type_name_from_type_name)
                .collect::<Option<Vec<_>>>()?;
            format!("tuple({})", items.join(", "))
        }
        TypeName::Option(item) => format!("?{}", render_runtime_type_name_from_type_name(item)?),
        TypeName::Function => "function".to_string(),
        TypeName::FunctionSignature { .. } => return None,
        TypeName::Applied { name, args } => {
            if matches!(
                name.as_str(),
                "subtype" | "castable_subtype" | "concrete_subtype" | "castable_concrete_subtype"
            ) && args.len() == 1
            {
                render_runtime_type_name_from_type_name(&args[0])?
            } else {
                let args = args
                    .iter()
                    .map(render_runtime_type_name_from_type_name)
                    .collect::<Option<Vec<_>>>()?;
                format!("{name}({})", args.join(", "))
            }
        }
        TypeName::Named(name) => name.clone(),
    })
}

fn substitute_runtime_type_name_params(
    type_name: &TypeName,
    substitutions: &HashMap<String, TypeName>,
) -> TypeName {
    match type_name {
        TypeName::TypeBounds { lower, upper } => TypeName::TypeBounds {
            lower: Box::new(substitute_runtime_type_name_params(lower, substitutions)),
            upper: Box::new(substitute_runtime_type_name_params(upper, substitutions)),
        },
        TypeName::Array(item) => TypeName::Array(
            item.as_ref()
                .map(|item| Box::new(substitute_runtime_type_name_params(item, substitutions))),
        ),
        TypeName::Map(key, value) => TypeName::Map(
            Box::new(substitute_runtime_type_name_params(key, substitutions)),
            Box::new(substitute_runtime_type_name_params(value, substitutions)),
        ),
        TypeName::WeakMap(key, value) => TypeName::WeakMap(
            Box::new(substitute_runtime_type_name_params(key, substitutions)),
            Box::new(substitute_runtime_type_name_params(value, substitutions)),
        ),
        TypeName::Tuple(items) => TypeName::Tuple(
            items
                .iter()
                .map(|item| substitute_runtime_type_name_params(item, substitutions))
                .collect(),
        ),
        TypeName::Option(item) => TypeName::Option(Box::new(substitute_runtime_type_name_params(
            item,
            substitutions,
        ))),
        TypeName::FunctionSignature {
            params,
            effects,
            return_type,
        } => TypeName::FunctionSignature {
            params: params
                .iter()
                .map(|param| substitute_runtime_type_name_params(param, substitutions))
                .collect(),
            effects: effects.clone(),
            return_type: Box::new(substitute_runtime_type_name_params(
                return_type,
                substitutions,
            )),
        },
        TypeName::Applied { name, args } => TypeName::Applied {
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| substitute_runtime_type_name_params(arg, substitutions))
                .collect(),
        },
        TypeName::Named(name) => {
            if let Some(replacement) = substitutions.get(name) {
                replacement.clone()
            } else if let Some((head, args)) = parse_runtime_parametric_name(name) {
                TypeName::Applied {
                    name: head,
                    args: args
                        .into_iter()
                        .map(parse_runtime_type_name)
                        .map(|arg| substitute_runtime_type_name_params(&arg, substitutions))
                        .collect(),
                }
            } else {
                TypeName::Named(name.clone())
            }
        }
        other => other.clone(),
    }
}

fn runtime_local_type_name(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

fn runtime_value_type_name(value: &Value) -> Option<TypeName> {
    match value {
        Value::Int(_) => Some(TypeName::Int),
        Value::Float(_) => Some(TypeName::Float),
        Value::Rational(_) => Some(TypeName::Rational),
        Value::Bool(_) => Some(TypeName::Bool),
        Value::String(_) => Some(TypeName::String),
        Value::Char(_) => Some(TypeName::Char),
        Value::Char32(_) => Some(TypeName::Char32),
        Value::None => Some(TypeName::None),
        Value::Array(items) => {
            let items = items.borrow();
            items
                .first()
                .and_then(runtime_value_type_name)
                .map(|item| TypeName::Array(Some(Box::new(item))))
                .or(Some(TypeName::Array(None)))
        }
        Value::Map(entries) => {
            let entries = entries.borrow();
            entries.first().and_then(|(key, value)| {
                Some(TypeName::Map(
                    Box::new(runtime_value_type_name(key)?),
                    Box::new(runtime_value_type_name(value)?),
                ))
            })
        }
        Value::Tuple(items) => Some(TypeName::Tuple(
            items
                .iter()
                .map(runtime_value_type_name)
                .collect::<Option<Vec<_>>>()?,
        )),
        Value::Option(Some(item)) => {
            runtime_value_type_name(item).map(|item| TypeName::Option(Box::new(item)))
        }
        Value::Option(None) => None,
        Value::Result { succeeded, value } => {
            let name = if *succeeded {
                "success_result"
            } else {
                "error_result"
            };
            Some(TypeName::Applied {
                name: name.to_string(),
                args: vec![runtime_value_type_name(value)?],
            })
        }
        Value::EnumValue { enum_name, .. } => Some(parse_runtime_type_name(enum_name)),
        Value::StructInstance { struct_name, .. } => Some(parse_runtime_type_name(struct_name)),
        Value::ClassInstance { class_name, .. } => Some(parse_runtime_type_name(class_name)),
        Value::StructType { name, .. }
        | Value::ClassType { name, .. }
        | Value::InterfaceType { name, .. }
        | Value::Module { name, .. } => Some(parse_runtime_type_name(name)),
        Value::Type(type_name) => Some(type_name.clone()),
        Value::NativeFunction { .. } | Value::ExternalFunction { .. } => Some(TypeName::Function),
        _ => None,
    }
}

fn runtime_type_value_payload(value: &Value) -> Option<TypeName> {
    match value {
        Value::Type(type_name)
        | Value::Subtype(type_name)
        | Value::CastableSubtype(type_name)
        | Value::ConcreteSubtype(type_name) => Some(type_name.clone()),
        Value::EnumType { name, .. }
        | Value::StructType { name, .. }
        | Value::ClassType { name, .. }
        | Value::InterfaceType { name, .. } => Some(parse_runtime_type_name(name)),
        _ => None,
    }
}

fn type_value_parameter_annotation(type_name: &TypeName) -> bool {
    match type_name {
        TypeName::Type | TypeName::TypeBounds { .. } => true,
        TypeName::Applied { name, .. } => {
            matches!(
                name.as_str(),
                "subtype" | "castable_subtype" | "concrete_subtype" | "castable_concrete_subtype"
            ) || !is_builtin_runtime_type_constructor(name)
        }
        _ => false,
    }
}

fn is_builtin_runtime_type_constructor(name: &str) -> bool {
    matches!(
        name,
        "array"
            | "map"
            | "weak_map"
            | "tuple"
            | "option"
            | "event"
            | "task"
            | "generator"
            | "result"
            | "success_result"
            | "error_result"
            | "subscribable_event"
            | "subscribable_event_intrnl"
            | "sticky_event"
            | "classifiable_subset"
            | "classifiable_subset_key"
            | "classifiable_subset_var"
            | "modifier"
            | "modifier_stack"
            | "awaitable"
            | "signalable"
            | "subscribable"
            | "listenable"
    )
}

fn infer_type_param_constraint_substitutions(
    constraint: &TypeParamConstraint,
    actual: &TypeName,
    capture_names: &HashSet<&str>,
    substitutions: &mut HashMap<String, TypeName>,
) {
    match constraint {
        TypeParamConstraint::Type => {}
        TypeParamConstraint::Subtype(parent) => {
            infer_type_name_constraint_substitutions(parent, actual, capture_names, substitutions);
        }
        TypeParamConstraint::TypeBounds { lower, upper } => {
            infer_type_name_constraint_substitutions(lower, actual, capture_names, substitutions);
            infer_type_name_constraint_substitutions(upper, actual, capture_names, substitutions);
        }
    }
}

fn infer_type_name_constraint_substitutions(
    expected: &TypeName,
    actual: &TypeName,
    capture_names: &HashSet<&str>,
    substitutions: &mut HashMap<String, TypeName>,
) {
    if let TypeName::Applied { name, args } = expected
        && type_wrapper_for_inference(name)
        && let [inner] = args.as_slice()
    {
        infer_type_name_constraint_substitutions(inner, actual, capture_names, substitutions);
        return;
    }
    infer_type_name_substitutions(expected, actual, true, capture_names, substitutions);
}

fn infer_type_name_substitutions(
    expected: &TypeName,
    actual: &TypeName,
    allow_named_capture: bool,
    capture_names: &HashSet<&str>,
    substitutions: &mut HashMap<String, TypeName>,
) {
    if let TypeName::Named(name) = expected
        && allow_named_capture
        && capture_names.contains(name.as_str())
    {
        record_type_name_substitution(name, actual, substitutions);
        return;
    }

    if let Some((expected_name, expected_args)) = runtime_aggregate_type_name_parts(expected)
        && !expected_args.is_empty()
        && let Some((actual_name, actual_args)) = runtime_aggregate_type_name_parts(actual)
        && runtime_type_names_match(&expected_name, &actual_name)
        && expected_args.len() == actual_args.len()
    {
        for (expected_arg, actual_arg) in expected_args.iter().zip(actual_args.iter()) {
            infer_type_name_substitutions(
                expected_arg,
                actual_arg,
                true,
                capture_names,
                substitutions,
            );
        }
        return;
    }

    match (expected, actual) {
        (TypeName::Array(Some(expected_item)), TypeName::Array(Some(actual_item)))
        | (TypeName::Option(expected_item), TypeName::Option(actual_item)) => {
            infer_type_name_substitutions(
                expected_item,
                actual_item,
                true,
                capture_names,
                substitutions,
            );
        }
        (TypeName::Map(expected_key, expected_value), TypeName::Map(actual_key, actual_value))
        | (
            TypeName::WeakMap(expected_key, expected_value),
            TypeName::WeakMap(actual_key, actual_value),
        ) => {
            infer_type_name_substitutions(
                expected_key,
                actual_key,
                true,
                capture_names,
                substitutions,
            );
            infer_type_name_substitutions(
                expected_value,
                actual_value,
                true,
                capture_names,
                substitutions,
            );
        }
        (TypeName::Tuple(expected_items), TypeName::Tuple(actual_items))
            if expected_items.len() == actual_items.len() =>
        {
            for (expected_item, actual_item) in expected_items.iter().zip(actual_items.iter()) {
                infer_type_name_substitutions(
                    expected_item,
                    actual_item,
                    true,
                    capture_names,
                    substitutions,
                );
            }
        }
        (
            TypeName::FunctionSignature {
                params: expected_params,
                return_type: expected_return,
                ..
            },
            TypeName::FunctionSignature {
                params: actual_params,
                return_type: actual_return,
                ..
            },
        ) if expected_params.len() == actual_params.len() => {
            for (expected_param, actual_param) in expected_params.iter().zip(actual_params.iter()) {
                infer_type_name_substitutions(
                    expected_param,
                    actual_param,
                    true,
                    capture_names,
                    substitutions,
                );
            }
            infer_type_name_substitutions(
                expected_return,
                actual_return,
                true,
                capture_names,
                substitutions,
            );
        }
        (TypeName::Applied { name, args }, _) if type_wrapper_for_inference(name) => {
            if let [inner] = args.as_slice() {
                infer_type_name_substitutions(
                    inner,
                    actual,
                    allow_named_capture,
                    capture_names,
                    substitutions,
                );
            }
        }
        _ => {}
    }
}

fn type_wrapper_for_inference(name: &str) -> bool {
    matches!(
        name,
        "subtype" | "castable_subtype" | "concrete_subtype" | "castable_concrete_subtype"
    )
}

fn record_type_name_substitution(
    name: &str,
    actual: &TypeName,
    substitutions: &mut HashMap<String, TypeName>,
) {
    match substitutions.get(name) {
        Some(existing) if existing != actual => {}
        _ => {
            substitutions.insert(name.to_string(), actual.clone());
        }
    }
}

fn runtime_value_matches_type_name(value: &Value, expected: &TypeName) -> bool {
    match expected {
        TypeName::Int => matches!(value, Value::Int(_)),
        TypeName::IntRange { min, max } => {
            matches!(value, Value::Int(value) if i128::from(*min) <= *value && *value <= i128::from(*max))
        }
        TypeName::Float => matches!(value, Value::Float(_)),
        TypeName::FloatRange(range) => {
            runtime_floatish_value_to_f64(value).is_some_and(|value| range.contains(value))
        }
        TypeName::Rational => matches!(value, Value::Rational(_)),
        TypeName::Number => matches!(value, Value::Int(_) | Value::Float(_) | Value::Rational(_)),
        TypeName::Bool => matches!(value, Value::Bool(_)),
        TypeName::String => matches!(value, Value::String(_)),
        TypeName::Message => matches!(value, Value::String(_) | Value::Diagnostic(_)),
        TypeName::Char | TypeName::Char8 => matches!(value, Value::Char(_)),
        TypeName::Char32 => matches!(value, Value::Char32(_)),
        TypeName::None => matches!(value, Value::None),
        TypeName::Any | TypeName::Comparable => true,
        TypeName::Type | TypeName::TypeBounds { .. } => matches!(
            value,
            Value::StructType { .. }
                | Value::ClassType { .. }
                | Value::InterfaceType { .. }
                | Value::ParametricType { .. }
                | Value::Subtype(_)
                | Value::CastableSubtype(_)
                | Value::ConcreteSubtype(_)
                | Value::Type(_)
                | Value::External
        ),
        TypeName::Named(name) => runtime_named_value_matches_type(value, name),
        TypeName::Applied { name, args } if name == "option" && args.len() == 1 => {
            runtime_value_matches_type_name(value, &TypeName::Option(Box::new(args[0].clone())))
        }
        TypeName::Applied { name, .. } => match value {
            Value::SubscribableEventIntrnl { .. } => matches!(
                name.as_str(),
                "subscribable_event_intrnl"
                    | "event"
                    | "listenable"
                    | "awaitable"
                    | "signalable"
                    | "subscribable"
            ),
            Value::SubscribableEvent { .. } => matches!(
                name.as_str(),
                "subscribable_event"
                    | "subscribable_event_intrnl"
                    | "event"
                    | "listenable"
                    | "awaitable"
                    | "signalable"
                    | "subscribable"
            ),
            Value::StickyEvent { .. } => matches!(
                name.as_str(),
                "sticky_event" | "event" | "awaitable" | "signalable"
            ),
            other => runtime_named_value_matches_type(other, name),
        },
        TypeName::Array(_) => matches!(value, Value::Array(_) | Value::Tuple(_)),
        TypeName::Map(_, _) | TypeName::WeakMap(_, _) => matches!(value, Value::Map(_)),
        TypeName::Tuple(_) => matches!(value, Value::Tuple(_)),
        TypeName::Option(_) => matches!(value, Value::Option(_)),
        TypeName::Function | TypeName::FunctionSignature { .. } => {
            matches!(
                value,
                Value::NativeFunction { .. } | Value::ExternalFunction { .. } | Value::External
            )
        }
    }
}

fn runtime_named_value_matches_type(value: &Value, expected: &str) -> bool {
    match value {
        Value::External => true,
        Value::Diagnostic(_) => expected == "diagnostic",
        Value::EnumValue { enum_name, .. } => runtime_type_names_match(enum_name, expected),
        Value::StructInstance { struct_name, .. } => {
            runtime_type_names_match(struct_name, expected)
        }
        Value::ClassInstance { class_name, .. } => runtime_type_names_match(class_name, expected),
        Value::EnumType { name, .. }
        | Value::StructType { name, .. }
        | Value::ClassType { name, .. }
        | Value::InterfaceType { name, .. }
        | Value::Module { name, .. } => runtime_type_names_match(name, expected),
        Value::Type(TypeName::Named(name) | TypeName::Applied { name, .. }) => {
            runtime_type_names_match(name, expected)
        }
        Value::Result {
            succeeded: true, ..
        } => expected == "result" || expected == "success_result",
        Value::Result {
            succeeded: false, ..
        } => expected == "result" || expected == "error_result",
        Value::ClassifiableSubset(_) => expected == "classifiable_subset",
        Value::ClassifiableSubsetKey { .. } => expected == "classifiable_subset_key",
        Value::ClassifiableSubsetVar { .. } => expected == "classifiable_subset_var",
        Value::ModifierCancelHandle { .. } | Value::SubscriptionCancelHandle { .. } => {
            expected == "cancelable"
        }
        _ => false,
    }
}

fn runtime_floatish_value_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Int(value) => Some(*value as f64),
        Value::Float(value) => Some(*value),
        _ => None,
    }
}

fn query_vm_value(value: VmValue) -> Option<VmValue> {
    match value {
        VmValue::Option(value) => value.map(|value| *value),
        VmValue::Runtime(value) => query_value(value).map(VmValue::Runtime),
        _ => None,
    }
}

fn set_field_vm_value(
    object: &mut VmValue,
    name: &str,
    value: Value,
    span: Span,
) -> Result<(), VerseError> {
    match object {
        VmValue::Runtime(Value::ClassInstance {
            class_name, fields, ..
        }) => {
            let mut fields = fields.borrow_mut();
            let Some(field) = fields.iter_mut().find(|field| field.name == name) else {
                return Err(VerseError::runtime_at(
                    format!("class `{class_name}` has no field `{name}`"),
                    span,
                ));
            };
            field.value = value;
            Ok(())
        }
        VmValue::Runtime(Value::StructInstance {
            struct_name,
            fields,
            ..
        }) => {
            let Some((_, field)) = fields.iter_mut().find(|(field_name, _)| field_name == name)
            else {
                return Err(VerseError::runtime_at(
                    format!("struct `{struct_name}` has no field `{name}`"),
                    span,
                ));
            };
            *field = value;
            Ok(())
        }
        VmValue::Runtime(_) => Err(VerseError::runtime_at(
            format!("value has no field `{name}`"),
            span,
        )),
        _ => Err(VerseError::runtime_at(
            format!(
                "bytecode {} value has no field `{name}`",
                vm_value_kind(object)
            ),
            span,
        )),
    }
}

fn set_field_value(object: Value, name: &str, value: Value, span: Span) -> Result<(), VerseError> {
    match object {
        Value::ClassInstance {
            class_name, fields, ..
        } => {
            let mut fields = fields.borrow_mut();
            let Some(field) = fields.iter_mut().find(|field| field.name == name) else {
                return Err(VerseError::runtime_at(
                    format!("class `{class_name}` has no field `{name}`"),
                    span,
                ));
            };
            field.value = value;
            Ok(())
        }
        Value::StructInstance {
            struct_name,
            mut fields,
            ..
        } => {
            let Some((_, field)) = fields.iter_mut().find(|(field_name, _)| field_name == name)
            else {
                return Err(VerseError::runtime_at(
                    format!("struct `{struct_name}` has no field `{name}`"),
                    span,
                ));
            };
            *field = value;
            Ok(())
        }
        _ => Err(VerseError::runtime_at(
            format!("value has no field `{name}`"),
            span,
        )),
    }
}

fn profile_wall_time_start() -> i64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    i64::try_from(nanos).unwrap_or(i64::MAX)
}

fn into_runtime(value: VmValue, span: Span) -> Result<Value, VerseError> {
    match value {
        VmValue::Runtime(value) => Ok(value),
        VmValue::Option(value) => Ok(Value::Option(
            value
                .map(|value| into_runtime(*value, span))
                .transpose()?
                .map(Box::new),
        )),
        VmValue::Function(_) => Err(VerseError::runtime_at(
            "bytecode function values cannot cross the runtime boundary yet",
            span,
        )),
        VmValue::BoundMethod(_) => Err(VerseError::runtime_at(
            "bytecode method values cannot cross the runtime boundary yet",
            span,
        )),
        VmValue::NumberMethod(_) => Err(VerseError::runtime_at(
            "bytecode number methods cannot cross the runtime boundary",
            span,
        )),
        VmValue::Scope(_) => Err(VerseError::runtime_at(
            "bytecode scopes cannot cross the runtime boundary",
            span,
        )),
        VmValue::Ref(_) => Err(VerseError::runtime_at(
            "bytecode refs cannot cross the runtime boundary",
            span,
        )),
        VmValue::FieldRef(_) => Err(VerseError::runtime_at(
            "bytecode field refs cannot cross the runtime boundary",
            span,
        )),
        VmValue::Semaphore(_) => Err(VerseError::runtime_at(
            "bytecode semaphores cannot cross the runtime boundary",
            span,
        )),
        VmValue::Placeholder(_) => Err(VerseError::runtime_at(
            "bytecode placeholders cannot cross the runtime boundary",
            span,
        )),
        VmValue::Uninitialized => Err(VerseError::runtime_at("uninitialized bytecode value", span)),
    }
}

fn vm_value_kind(value: &VmValue) -> &'static str {
    match value {
        VmValue::Runtime(_) => "runtime value",
        VmValue::Function(_) => "function",
        VmValue::BoundMethod(_) => "bound method",
        VmValue::NumberMethod(_) => "number method",
        VmValue::Scope(_) => "scope",
        VmValue::Option(_) => "option",
        VmValue::Ref(_) => "ref",
        VmValue::FieldRef(_) => "field ref",
        VmValue::Semaphore(_) => "semaphore",
        VmValue::Placeholder(_) => "placeholder",
        VmValue::Uninitialized => "uninitialized",
    }
}

fn value_from_constant(constant: &Constant) -> VmValue {
    match constant {
        Constant::Int(value) => VmValue::Runtime(Value::Int(*value)),
        Constant::Float(value) => VmValue::Runtime(Value::Float(*value)),
        Constant::Char {
            value,
            kind: CharacterKind::Char,
        } => VmValue::Runtime(Value::Char(*value)),
        Constant::Char {
            value,
            kind: CharacterKind::Char32,
        } => VmValue::Runtime(Value::Char32(*value)),
        Constant::Bool(value) => VmValue::Runtime(Value::Bool(*value)),
        Constant::String(value) => VmValue::Runtime(Value::String(value.clone())),
        Constant::None => VmValue::Runtime(Value::None),
        Constant::Type(type_name) => VmValue::Runtime(Value::Type(type_name.clone())),
        Constant::Option(value) => VmValue::Runtime(Value::Option(value.as_ref().map(|value| {
            Box::new(
                into_runtime(value_from_constant(value), Span::new(0, 0, 1, 1))
                    .expect("option constants should contain runtime values"),
            )
        }))),
        Constant::Range { start, end } => VmValue::Runtime(Value::Range {
            start: *start,
            end: *end,
        }),
        Constant::Tuple(items) => VmValue::Runtime(Value::Tuple(
            items
                .iter()
                .map(|item| into_runtime(value_from_constant(item), Span::new(0, 0, 1, 1)))
                .collect::<Result<Vec<_>, _>>()
                .expect("tuple constants should contain runtime values"),
        )),
        Constant::EnumValue { enum_name, variant } => VmValue::Runtime(Value::EnumValue {
            enum_name: enum_name.clone(),
            variant: variant.clone(),
        }),
        Constant::Function(function) => VmValue::Function(VmFunction {
            function: *function,
            captures: None,
        }),
        Constant::NativeFunction(name) => VmValue::Runtime(
            bytecode_native_function_value(name).expect("known native function constant"),
        ),
        Constant::StructType { name, computes } => {
            VmValue::Runtime(bytecode_struct_type_value(name.clone(), *computes))
        }
        Constant::ClassType {
            name,
            base,
            interfaces,
            unique,
            abstract_class,
            epic_internal_class,
            final_class,
            final_super,
            concrete,
            castable,
        } => VmValue::Runtime(bytecode_class_type_value(RuntimeClassTypeInfo {
            name: name.clone(),
            base: base.clone(),
            interfaces: interfaces.clone(),
            unique: *unique,
            abstract_class: *abstract_class,
            epic_internal_class: *epic_internal_class,
            final_class: *final_class,
            final_super: *final_super,
            concrete: *concrete,
            castable: *castable,
        })),
        Constant::InterfaceType { name } => {
            VmValue::Runtime(bytecode_interface_type_value(name.clone()))
        }
        Constant::ParametricType { name, params } => {
            let span = Span::new(0, 0, 1, 1);
            VmValue::Runtime(Value::ParametricType {
                name: name.clone(),
                params: params
                    .iter()
                    .map(|name| TypeParam {
                        name: name.clone(),
                        constraint: TypeParamConstraint::Type,
                        span,
                    })
                    .collect(),
                body: Box::new(Expr::new(ExprKind::External, span)),
                closure: Env,
            })
        }
        Constant::External(type_name) => VmValue::Runtime(bytecode_external_value(type_name)),
        Constant::ExternalAggregate {
            class_name,
            unique,
            object_kind,
            fields,
        } => match object_kind {
            ObjectKind::Class => VmValue::Runtime(bytecode_class_instance_value(
                class_name.clone(),
                *unique,
                fields
                    .iter()
                    .map(|(name, mutable, type_name)| {
                        (
                            name.clone(),
                            *mutable,
                            bytecode_external_return_value(type_name),
                        )
                    })
                    .collect(),
            )),
            ObjectKind::Struct { computes } => VmValue::Runtime(Value::StructInstance {
                struct_name: class_name.clone(),
                computes: *computes,
                fields: fields
                    .iter()
                    .map(|(name, _, type_name)| {
                        (name.clone(), bytecode_external_return_value(type_name))
                    })
                    .collect(),
            }),
        },
        Constant::ExternalInterface {
            interface_name,
            fields,
        } => VmValue::Runtime(bytecode_class_instance_value(
            interface_name.clone(),
            false,
            fields
                .iter()
                .map(|(name, mutable, type_name)| {
                    (
                        name.clone(),
                        *mutable,
                        bytecode_external_return_value(type_name),
                    )
                })
                .collect(),
        )),
        Constant::ExternalReturn(type_name) => {
            VmValue::Runtime(bytecode_external_return_value(type_name))
        }
        Constant::GlobalRef(name) => {
            panic!("global ref `{name}` must be resolved by BytecodeExecutor")
        }
    }
}

fn task_id(task: &Rc<RuntimeTask>) -> usize {
    Rc::as_ptr(task) as usize
}

fn subscription_key(subscribers: &Rc<RefCell<Vec<RuntimeSubscriptionEntry>>>) -> usize {
    Rc::as_ptr(subscribers) as usize
}

fn sleep_seconds(value: Value, span: Span) -> Result<f64, VerseError> {
    let seconds = match value {
        Value::Int(value) => value as f64,
        Value::Float(value) => value,
        Value::Rational(value) => value.to_f64(),
        other => {
            return Err(VerseError::runtime_at(
                format!("`Sleep` Seconds expected number, got {other}"),
                span,
            ));
        }
    };
    if seconds.is_nan() {
        return Err(VerseError::runtime_at(
            "`Sleep` Seconds cannot be NaN",
            span,
        ));
    }
    Ok(seconds)
}

fn duration_from_seconds(seconds: f64) -> Duration {
    if seconds <= 0.0 {
        return Duration::ZERO;
    }
    Duration::try_from_secs_f64(seconds).unwrap_or(Duration::MAX)
}

fn neg_value(value: Value, span: Span) -> Result<Value, VerseError> {
    match value {
        Value::Int(value) => value
            .checked_neg()
            .map(Value::Int)
            .ok_or_else(|| VerseError::runtime_at("integer overflow", span)),
        Value::Float(value) => Ok(Value::Float(-value)),
        Value::Rational(value) => value
            .checked_neg()
            .map(Value::Rational)
            .ok_or_else(|| VerseError::runtime_at("integer overflow", span)),
        other => Err(VerseError::runtime_at(
            format!("Neg expected number, got {other}"),
            span,
        )),
    }
}

fn query_value(value: Value) -> Option<Value> {
    match value {
        Value::Bool(false) | Value::None => None,
        Value::Option(value) => value.map(|value| *value),
        other => Some(other),
    }
}

fn comparison_succeeds(
    instruction: &Instruction,
    left: &Value,
    right: &Value,
    span: Span,
) -> Result<bool, VerseError> {
    match instruction {
        Instruction::Neq { .. } => Ok(left != right),
        Instruction::Lt { .. } => compare_value_refs(left, right, span, |left, right| left < right),
        Instruction::Lte { .. } => {
            compare_value_refs(left, right, span, |left, right| left <= right)
        }
        Instruction::Gt { .. } => compare_value_refs(left, right, span, |left, right| left > right),
        Instruction::Gte { .. } => {
            compare_value_refs(left, right, span, |left, right| left >= right)
        }
        _ => unreachable!("comparison_succeeds called with non-comparison instruction"),
    }
}

fn fast_comparison_succeeds(
    instruction: &Instruction,
    left: &Value,
    right: &Value,
    span: Span,
) -> Result<bool, VerseError> {
    match instruction {
        Instruction::EqFastFail { .. } => Ok(left == right),
        Instruction::NeqFastFail { .. } => Ok(left != right),
        Instruction::LtFastFail { .. } => {
            compare_value_refs(left, right, span, |left, right| left < right)
        }
        Instruction::LteFastFail { .. } => {
            compare_value_refs(left, right, span, |left, right| left <= right)
        }
        Instruction::GtFastFail { .. } => {
            compare_value_refs(left, right, span, |left, right| left > right)
        }
        Instruction::GteFastFail { .. } => {
            compare_value_refs(left, right, span, |left, right| left >= right)
        }
        _ => unreachable!("fast_comparison_succeeds called with non-fast-fail instruction"),
    }
}

fn compare_value_refs(
    left: &Value,
    right: &Value,
    span: Span,
    predicate: fn(f64, f64) -> bool,
) -> Result<bool, VerseError> {
    Ok(predicate(
        expect_number_ref(left, "left operand", span)?,
        expect_number_ref(right, "right operand", span)?,
    ))
}

fn index_value_failable(
    collection: Value,
    index: Value,
    span: Span,
) -> Result<Option<Value>, VerseError> {
    match collection {
        Value::Array(items) => {
            let index = expect_index(&index, "array index", span)?;
            Ok(items.borrow().get(index).map(copy_runtime_value))
        }
        Value::Generator { values, .. } => {
            let index = expect_index(&index, "generator index", span)?;
            Ok(values.borrow().get(index).map(copy_runtime_value))
        }
        Value::Tuple(items) => {
            let index = expect_index(&index, "tuple index", span)?;
            Ok(items.get(index).map(copy_runtime_value))
        }
        Value::Map(entries) => Ok(entries
            .borrow()
            .iter()
            .find_map(|(key, value)| (key == &index).then(|| copy_runtime_value(value)))),
        Value::String(text) => string_index_value_failable(&text, &index, span),
        other => Err(VerseError::runtime_at(
            format!("cannot index value `{other}`"),
            span,
        )),
    }
}

enum CallSetEffect {
    Updated,
    Replace(Box<VmValue>),
    Failed,
}

fn call_set_vm_value(
    container: VmValue,
    index: Value,
    value: Value,
    span: Span,
) -> Result<bool, VerseError> {
    match call_set_owned_value(container, index, value, span)? {
        CallSetEffect::Updated => Ok(true),
        CallSetEffect::Failed => Ok(false),
        CallSetEffect::Replace(_) => Err(VerseError::runtime_at(
            "CallSet cannot replace an immutable container",
            span,
        )),
    }
}

fn call_set_owned_value(
    container: VmValue,
    index: Value,
    value: Value,
    span: Span,
) -> Result<CallSetEffect, VerseError> {
    match container {
        VmValue::Runtime(container) => call_set_runtime_value(container, index, value, span),
        VmValue::Ref(ref_value) => {
            let current = ref_value.borrow().clone();
            match call_set_owned_value(current, index, value, span)? {
                CallSetEffect::Replace(value) => {
                    *ref_value.borrow_mut() = *value;
                    Ok(CallSetEffect::Updated)
                }
                effect => Ok(effect),
            }
        }
        VmValue::FieldRef(field_ref) => {
            let field_ref = VmRef::Field(field_ref);
            let current = field_ref.get(span)?;
            match call_set_owned_value(current, index, value, span)? {
                CallSetEffect::Replace(value) => {
                    field_ref.set(*value, span)?;
                    Ok(CallSetEffect::Updated)
                }
                effect => Ok(effect),
            }
        }
        other => Err(VerseError::runtime_at(
            format!(
                "CallSet expected mutable container, got {}",
                vm_value_kind(&other)
            ),
            span,
        )),
    }
}

fn call_set_runtime_value(
    container: Value,
    index: Value,
    value: Value,
    span: Span,
) -> Result<CallSetEffect, VerseError> {
    match container {
        Value::Array(items) => {
            let Some(index) = index_to_usize_failable(&index) else {
                return Ok(CallSetEffect::Failed);
            };
            let mut items = items.borrow_mut();
            if index >= items.len() {
                return Ok(CallSetEffect::Failed);
            }
            items[index] =
                if matches!(&items[index], Value::Array(_)) && matches!(value, Value::Tuple(_)) {
                    tuple_value_to_array(value)
                } else {
                    value
                };
            Ok(CallSetEffect::Updated)
        }
        Value::Map(entries) => {
            let mut entries = entries.borrow_mut();
            upsert_map_entry(&mut entries, index, value);
            Ok(CallSetEffect::Updated)
        }
        Value::String(text) => Ok(
            match replace_string_byte_failable(text, &index, value, span)? {
                Some(value) => {
                    CallSetEffect::Replace(Box::new(VmValue::Runtime(Value::String(value))))
                }
                None => CallSetEffect::Failed,
            },
        ),
        other => Err(VerseError::runtime_at(
            format!("CallSet expected mutable container, got {other}"),
            span,
        )),
    }
}

fn map_key_value(map: Value, index: Value, span: Span) -> Result<Value, VerseError> {
    let Value::Map(entries) = map else {
        return Err(VerseError::runtime_at(
            format!("MapKey expected map, got {map}"),
            span,
        ));
    };
    let index = expect_index(&index, "map key index", span)?;
    entries
        .borrow()
        .get(index)
        .map(|(key, _)| copy_runtime_value(key))
        .ok_or_else(|| {
            VerseError::runtime_at(format!("map key index out of bounds: {index}"), span)
        })
}

fn map_entry_value(map: Value, index: Value, span: Span) -> Result<Value, VerseError> {
    let Value::Map(entries) = map else {
        return Err(VerseError::runtime_at(
            format!("MapValue expected map, got {map}"),
            span,
        ));
    };
    let index = expect_index(&index, "map value index", span)?;
    entries
        .borrow()
        .get(index)
        .map(|(_, value)| copy_runtime_value(value))
        .ok_or_else(|| {
            VerseError::runtime_at(format!("map value index out of bounds: {index}"), span)
        })
}

fn index_to_usize_failable(value: &Value) -> Option<usize> {
    let Value::Int(index) = value else {
        return None;
    };
    usize::try_from(*index).ok()
}

fn tuple_value_to_array(value: Value) -> Value {
    match value {
        Value::Tuple(items) => array_value(items),
        other => other,
    }
}

fn string_index_value_failable(
    text: &str,
    index: &Value,
    span: Span,
) -> Result<Option<Value>, VerseError> {
    let index = expect_index(index, "string index", span)?;
    Ok(text
        .as_bytes()
        .get(index)
        .map(|byte| Value::Char(char::from(*byte))))
}

fn expect_index(value: &Value, context: &str, span: Span) -> Result<usize, VerseError> {
    let Value::Int(index) = value else {
        return Err(VerseError::runtime_at(
            format!("{context} expected int, got {value}"),
            span,
        ));
    };
    if *index < 0 {
        return Err(VerseError::runtime_at(
            format!("{context} cannot be negative: {index}"),
            span,
        ));
    }
    Ok(*index as usize)
}

fn upsert_map_entry(entries: &mut Vec<(Value, Value)>, key: Value, value: Value) {
    if let Some((_, existing_value)) = entries
        .iter_mut()
        .find(|(existing_key, _)| existing_key == &key)
    {
        *existing_value = value;
    } else {
        entries.push((key, value));
    }
}

fn copy_runtime_value(value: &Value) -> Value {
    match value {
        Value::Int(_)
        | Value::Float(_)
        | Value::Rational(_)
        | Value::Char(_)
        | Value::Char32(_)
        | Value::Bool(_)
        | Value::String(_)
        | Value::Diagnostic(_)
        | Value::External
        | Value::ExternalFunction { .. }
        | Value::None
        | Value::Pending
        | Value::Suspended(_)
        | Value::Session
        | Value::Range { .. }
        | Value::EnumType { .. }
        | Value::EnumValue { .. }
        | Value::StructType { .. }
        | Value::ClassType { .. }
        | Value::InterfaceType { .. }
        | Value::Event { .. }
        | Value::Awaitable { .. }
        | Value::Signalable { .. }
        | Value::Task(_)
        | Value::Generator { .. }
        | Value::Modifier { .. }
        | Value::ModifierStack { .. }
        | Value::ModifierCancelHandle { .. }
        | Value::Subtype(_)
        | Value::CastableSubtype(_)
        | Value::ConcreteSubtype(_)
        | Value::Type(_)
        | Value::NativeFunction { .. }
        | Value::NativeArrayMethod { .. }
        | Value::NativeResultMethod { .. }
        | Value::NativeEventMethod { .. }
        | Value::NativeSubscribableMethod { .. }
        | Value::NativeTaskMethod { .. }
        | Value::NativeModifierMethod { .. }
        | Value::NativeCancelMethod { .. }
        | Value::NativeSubscriptionCancelMethod { .. }
        | Value::SubscriptionCancelHandle { .. } => value.clone(),
        Value::Array(items) => Value::Array(Rc::new(RefCell::new(
            items.borrow().iter().map(copy_runtime_value).collect(),
        ))),
        Value::Map(entries) => Value::Map(Rc::new(RefCell::new(
            entries
                .borrow()
                .iter()
                .map(|(key, value)| (copy_runtime_value(key), copy_runtime_value(value)))
                .collect(),
        ))),
        Value::Tuple(items) => Value::Tuple(items.iter().map(copy_runtime_value).collect()),
        Value::Option(value) => Value::Option(
            value
                .as_ref()
                .map(|value| Box::new(copy_runtime_value(value))),
        ),
        Value::Result { succeeded, value } => Value::Result {
            succeeded: *succeeded,
            value: Box::new(copy_runtime_value(value)),
        },
        Value::Subscribable {
            payload,
            subscribers,
            next_subscriber_id,
        } => Value::Subscribable {
            payload: payload.clone(),
            subscribers: subscribers.clone(),
            next_subscriber_id: next_subscriber_id.clone(),
        },
        Value::SubscribableEventIntrnl {
            payload,
            waiters,
            subscribers,
            next_subscriber_id,
        } => Value::SubscribableEventIntrnl {
            payload: payload.clone(),
            waiters: waiters.clone(),
            subscribers: subscribers.clone(),
            next_subscriber_id: next_subscriber_id.clone(),
        },
        Value::SubscribableEvent {
            payload,
            waiters,
            subscribers,
            next_subscriber_id,
        } => Value::SubscribableEvent {
            payload: payload.clone(),
            waiters: waiters.clone(),
            subscribers: subscribers.clone(),
            next_subscriber_id: next_subscriber_id.clone(),
        },
        Value::StickyEvent {
            payload,
            waiters,
            signal,
        } => Value::StickyEvent {
            payload: payload.clone(),
            waiters: waiters.clone(),
            signal: signal.clone(),
        },
        Value::Listenable {
            payload,
            subscribers,
            next_subscriber_id,
        } => Value::Listenable {
            payload: payload.clone(),
            subscribers: subscribers.clone(),
            next_subscriber_id: next_subscriber_id.clone(),
        },
        Value::ClassifiableSubset(items) => Value::ClassifiableSubset(Rc::new(RefCell::new(
            items.borrow().iter().map(copy_runtime_value).collect(),
        ))),
        Value::ClassifiableSubsetKey { entries, entry_id } => Value::ClassifiableSubsetKey {
            entries: entries.clone(),
            entry_id: *entry_id,
        },
        Value::ClassifiableSubsetVar { entries, next_key } => Value::ClassifiableSubsetVar {
            entries: entries.clone(),
            next_key: next_key.clone(),
        },
        Value::StructInstance {
            struct_name,
            computes,
            fields,
        } => Value::StructInstance {
            struct_name: struct_name.clone(),
            computes: *computes,
            fields: fields
                .iter()
                .map(|(name, value)| (name.clone(), copy_runtime_value(value)))
                .collect(),
        },
        Value::ParametricType { .. }
        | Value::Function { .. }
        | Value::Overload(_)
        | Value::BoundMethod { .. }
        | Value::Module { .. }
        | Value::ClassInstance { .. } => value.clone(),
    }
}

fn add_values(left: Value, right: Value, span: Span) -> Result<Value, VerseError> {
    if let Some(value) = bytecode_color_add_values(&left, &right) {
        return Ok(value);
    }
    match (left, right) {
        (Value::Diagnostic(left), Value::Diagnostic(right)) => {
            Ok(Value::Diagnostic(format!("{left}{right}")))
        }
        (Value::Diagnostic(left), Value::String(right)) => {
            Ok(Value::Diagnostic(format!("{left}{right}")))
        }
        (Value::String(left), Value::Diagnostic(right)) => {
            Ok(Value::Diagnostic(format!("{left}{right}")))
        }
        (Value::String(left), Value::String(right)) => Ok(Value::String(left + &right)),
        (Value::ClassifiableSubset(left), Value::ClassifiableSubset(right)) => {
            let left = left.borrow();
            let right = right.borrow();
            let mut values = Vec::new();
            for value in left.iter().chain(right.iter()) {
                if !values.iter().any(|existing| existing == value) {
                    values.push(copy_runtime_value(value));
                }
            }
            Ok(Value::ClassifiableSubset(Rc::new(RefCell::new(values))))
        }
        (
            Value::ClassifiableSubsetVar { entries: left, .. },
            Value::ClassifiableSubsetVar { entries: right, .. },
        ) => {
            let left = left.borrow();
            let right = right.borrow();
            let mut values = Vec::new();
            for entry in left.iter().chain(right.iter()) {
                if !values.iter().any(|existing| existing == &entry.value) {
                    values.push(copy_runtime_value(&entry.value));
                }
            }
            Ok(Value::ClassifiableSubset(Rc::new(RefCell::new(values))))
        }
        (Value::String(left), Value::Array(right)) => {
            let Some(right) = char_array_to_string(right.borrow().as_slice()) else {
                return Err(VerseError::runtime_at(
                    "`+` expected string-compatible `[]char`",
                    span,
                ));
            };
            Ok(Value::String(format!("{left}{right}")))
        }
        (Value::Array(left), Value::String(right)) => {
            let Some(left) = char_array_to_string(left.borrow().as_slice()) else {
                return Err(VerseError::runtime_at(
                    "`+` expected string-compatible `[]char`",
                    span,
                ));
            };
            Ok(Value::String(format!("{left}{right}")))
        }
        (Value::Array(left), Value::Array(right)) => {
            let mut values = left
                .borrow()
                .iter()
                .map(copy_runtime_value)
                .collect::<Vec<_>>();
            values.extend(right.borrow().iter().map(copy_runtime_value));
            Ok(array_value(values))
        }
        (Value::Array(left), Value::Tuple(right)) => {
            let mut values = left
                .borrow()
                .iter()
                .map(copy_runtime_value)
                .collect::<Vec<_>>();
            values.extend(right.iter().map(copy_runtime_value));
            Ok(array_value(values))
        }
        (left, right) => numeric_values(
            left,
            right,
            span,
            checked_add,
            |left, right| left + right,
            RationalValue::add,
        ),
    }
}

fn subtract_values(left: Value, right: Value, span: Span) -> Result<Value, VerseError> {
    if let Some(value) = bytecode_color_subtract_values(&left, &right) {
        return Ok(value);
    }
    numeric_values(
        left,
        right,
        span,
        checked_sub,
        |left, right| left - right,
        RationalValue::subtract,
    )
}

fn multiply_values(left: Value, right: Value, span: Span) -> Result<Value, VerseError> {
    if let Some(value) = bytecode_color_multiply_or_scale_values(&left, &right, span)? {
        return Ok(value);
    }
    numeric_values(
        left,
        right,
        span,
        checked_mul,
        |left, right| left * right,
        RationalValue::multiply,
    )
}

fn mutable_add_values(left: Value, right: Value, span: Span) -> Result<Value, VerseError> {
    let (Value::Array(left), Value::Array(right)) = (left, right) else {
        return Err(VerseError::runtime_at(
            "MutableAdd expected array operands",
            span,
        ));
    };
    let mut values = left
        .borrow()
        .iter()
        .map(copy_runtime_value)
        .collect::<Vec<_>>();
    values.extend(right.borrow().iter().map(copy_runtime_value));
    Ok(array_value(values))
}

fn fast_append_to_array(left: Value, right: Value, span: Span) -> Result<(), VerseError> {
    let Value::Array(left) = left else {
        return Err(VerseError::runtime_at(
            "FastAppendToArray expected mutable array left operand",
            span,
        ));
    };
    let Value::Array(right) = right else {
        return Err(VerseError::runtime_at(
            "FastAppendToArray expected array right operand",
            span,
        ));
    };
    left.borrow_mut()
        .extend(right.borrow().iter().map(copy_runtime_value));
    Ok(())
}

fn length_value(container: Value, span: Span) -> Result<Value, VerseError> {
    let length = match container {
        Value::Array(items) => items.borrow().len(),
        Value::Generator { values, .. } => values.borrow().len(),
        Value::Map(entries) => entries.borrow().len(),
        Value::Tuple(items) => items.len(),
        Value::String(text) => text.len(),
        other => {
            return Err(VerseError::runtime_at(
                format!("Length expected container, got {other}"),
                span,
            ));
        }
    };
    Ok(Value::Int(length as i128))
}

fn array_value(values: Vec<Value>) -> Value {
    Value::Array(Rc::new(RefCell::new(values)))
}

fn char_value_to_byte(value: &Value) -> Option<u8> {
    match value {
        Value::Char(value) => u8::try_from(*value as u32).ok(),
        _ => None,
    }
}

fn char_array_to_string(items: &[Value]) -> Option<String> {
    let bytes = items
        .iter()
        .map(char_value_to_byte)
        .collect::<Option<Vec<_>>>()?;
    String::from_utf8(bytes).ok()
}

fn divide_values(left: Value, right: Value, span: Span) -> Result<Value, VerseError> {
    if let Some(value) = bytecode_color_divide_values(&left, &right, span)? {
        return Ok(value);
    }
    if numeric_is_zero(&right) {
        return Err(VerseError::runtime_at("division by zero", span));
    }

    match (left, right) {
        (Value::Int(left), Value::Int(right)) => {
            Ok(Value::Rational(RationalValue::new(left, right)))
        }
        (left, right)
            if matches!(left, Value::Int(_) | Value::Rational(_))
                && matches!(right, Value::Int(_) | Value::Rational(_)) =>
        {
            let left = rational_operand(left, "`/` left operand", span)?;
            let right = rational_operand(right, "`/` right operand", span)?;
            left.divide(right)
                .map(Value::Rational)
                .ok_or_else(|| VerseError::runtime_at("division by zero", span))
        }
        (left, right) => Ok(Value::Float(
            expect_number_ref(&left, "left operand", span)?
                / expect_number_ref(&right, "right operand", span)?,
        )),
    }
}

fn rational_operand(value: Value, context: &str, span: Span) -> Result<RationalValue, VerseError> {
    match value {
        Value::Int(value) => Ok(RationalValue::from_int(value)),
        Value::Rational(value) => Ok(value),
        other => Err(VerseError::runtime_at(
            format!("{context} expected rational-compatible number, got {other}"),
            span,
        )),
    }
}

fn remainder_values(left: Value, right: Value, span: Span) -> Result<Value, VerseError> {
    let left = expect_int(left, "`%` left operand", span)?;
    let right = expect_int(right, "`%` right operand", span)?;
    if right == 0 {
        return Err(VerseError::runtime_at("remainder by zero", span));
    }
    Ok(Value::Int(left % right))
}

fn numeric_values(
    left: Value,
    right: Value,
    span: Span,
    int_op: fn(i128, i128) -> Option<i128>,
    float_op: fn(f64, f64) -> f64,
    rational_op: fn(RationalValue, RationalValue) -> RationalValue,
) -> Result<Value, VerseError> {
    if let (Value::Int(left), Value::Int(right)) = (&left, &right) {
        return int_op(*left, *right)
            .map(Value::Int)
            .ok_or_else(|| VerseError::runtime_at("integer overflow", span));
    }
    if matches!((&left, &right), (Value::Float(_), _) | (_, Value::Float(_))) {
        return Ok(Value::Float(float_op(
            expect_number_ref(&left, "left operand", span)?,
            expect_number_ref(&right, "right operand", span)?,
        )));
    }
    if matches!(
        (&left, &right),
        (
            Value::Int(_) | Value::Rational(_),
            Value::Int(_) | Value::Rational(_)
        )
    ) {
        let left = rational_operand(left, "left operand", span)?;
        let right = rational_operand(right, "right operand", span)?;
        return Ok(rational_or_int(rational_op(left, right)));
    }
    Ok(Value::Float(float_op(
        expect_number_ref(&left, "left operand", span)?,
        expect_number_ref(&right, "right operand", span)?,
    )))
}

fn expect_int(value: Value, context: &str, span: Span) -> Result<i128, VerseError> {
    match value {
        Value::Int(value) => Ok(value),
        other => Err(VerseError::runtime_at(
            format!("{context} expected int, got {other}"),
            span,
        )),
    }
}

fn expect_number_ref(value: &Value, context: &str, span: Span) -> Result<f64, VerseError> {
    match value {
        Value::Int(value) => Ok(*value as f64),
        Value::Float(value) => Ok(*value),
        Value::Rational(value) => Ok(value.to_f64()),
        other => Err(VerseError::runtime_at(
            format!("{context} expected number, got {other}"),
            span,
        )),
    }
}

fn numeric_is_zero(value: &Value) -> bool {
    match value {
        Value::Int(value) => *value == 0,
        Value::Float(value) => *value == 0.0,
        Value::Rational(value) => value.is_zero(),
        _ => false,
    }
}

fn checked_add(left: i128, right: i128) -> Option<i128> {
    left.checked_add(right)
}

fn checked_sub(left: i128, right: i128) -> Option<i128> {
    left.checked_sub(right)
}

fn checked_mul(left: i128, right: i128) -> Option<i128> {
    left.checked_mul(right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::new(0, 0, 1, 1)
    }

    #[test]
    fn task_await_does_not_drain_unrelated_host_timers() {
        let source = r#"
var Trace:int = 0
var Result:int = 0
A()<suspends><transacts>:int =
    Sleep(1.0)
    set Trace = Trace * 10 + 1
    1
B()<suspends><transacts>:int =
    Sleep(2.0)
    set Trace = Trace * 10 + 2
    2
Run()<suspends><transacts>:void =
    TaskA := spawn{A()}
    TaskB := spawn{B()}
    TaskA.Await()
    set Result = Trace
    set Result = Result + (TaskB.Await() * 0)
spawn{Run()}
Result
"#;
        let ir = crate::pipeline::compile_source(source).expect("source should compile");
        let mut executor = BytecodeExecutor::new(ir.bytecode_program());
        let value = executor.run().expect("source should run");

        assert_eq!(value, Value::Int(1));
        assert_eq!(executor.host_now(), 2.0);
    }

    #[test]
    fn canceling_sleeping_task_clears_host_timer() {
        let source = r#"
var Result:int = 0
Worker()<suspends><transacts>:void =
    Sleep(5.0)
    set Result = 99
Run()<suspends><transacts>:void =
    Task:task(void) = spawn{Worker()}
    Task.Cancel()
spawn{Run()}
Result
"#;
        let ir = crate::pipeline::compile_source(source).expect("source should compile");
        let mut executor = BytecodeExecutor::new(ir.bytecode_program());
        let value = executor.run().expect("source should run");

        assert_eq!(value, Value::Int(0));
        assert_eq!(executor.host_now(), 0.0);
        assert!(!executor.host_has_pending());
    }

    #[test]
    fn canceling_external_awaitable_task_clears_host_future() {
        let source = r#"
var Result:int = 0
Source:awaitable(int) = external {}
Worker()<suspends><transacts>:void =
    set Result = Source.Await()
Run()<suspends><transacts>:void =
    Task:task(void) = spawn{Worker()}
    Task.Cancel()
spawn{Run()}
Result
"#;
        let ir = crate::pipeline::compile_source(source).expect("source should compile");
        let mut executor = BytecodeExecutor::new(ir.bytecode_program());
        let value = executor.run().expect("source should run");

        assert_eq!(value, Value::Int(0));
        assert!(!executor.host_has_pending());
    }

    #[test]
    fn host_future_resumes_external_awaitable_with_payload() {
        let source = r#"
var Result:int = 0
Source:awaitable(int) = external {}
Run()<suspends><transacts>:void =
    set Result = Source.Await()
spawn{Run()}
Result
"#;
        let ir = crate::pipeline::compile_source(source).expect("source should compile");
        let host = MockHost::with_scripted_future_completion(1, Value::Int(42));
        let mut executor = BytecodeExecutor::with_host(ir.bytecode_program(), host);
        let value = executor.run().expect("source should run");

        assert_eq!(value, Value::Int(42));
    }

    #[test]
    fn pending_host_future_stops_without_busy_polling() {
        let source = r#"
var Result:int = 0
Source:awaitable(int) = external {}
Run()<suspends><transacts>:void =
    set Result = Source.Await()
spawn{Run()}
Result
"#;
        let ir = crate::pipeline::compile_source(source).expect("source should compile");
        let mut executor = BytecodeExecutor::new(ir.bytecode_program());
        let value = executor.run().expect("source should run");

        assert_eq!(value, Value::Int(0));
        assert!(executor.host_poll_count() <= 2);
    }

    #[test]
    fn host_signal_invokes_listenable_subscribers_fifo() {
        let source = r#"
var Trace:int = 0
First(Value:int)<transacts>:void =
    set Trace = Trace * 10 + Value
Second(Value:int)<transacts>:void =
    set Trace = Trace * 10 + Value + 1
Source:listenable(int) = external {}
Run()<suspends><transacts>:void =
    Source.Subscribe(First)
    Source.Subscribe(Second)
    Sleep(0.0)
spawn{Run()}
Trace
"#;
        let ir = crate::pipeline::compile_source(source).expect("source should compile");
        let host = MockHost::with_scripted_future_completion(1, Value::Int(4));
        let mut executor = BytecodeExecutor::with_host(ir.bytecode_program(), host);
        let value = executor.run().expect("source should run");

        assert_eq!(value, Value::Int(45));
    }

    #[test]
    fn predicts_extern_reads_host_prediction_default() {
        let source = r#"
sync_state := class:
    @predicts_extern
    State<predicts>:int = 0

sync_state{}.State
"#;
        let ir = crate::pipeline::compile_source(source).expect("source should compile");
        let host = MockHost::with_prediction_default("sync_state", "State", Value::Int(42));
        let mut executor = BytecodeExecutor::with_host(ir.bytecode_program(), host);
        let value = executor.run().expect("source should run");

        assert_eq!(value, Value::Int(42));
    }

    #[test]
    fn official_utility_control_flow_instructions_execute() {
        let slot = RegisterIndex(2);
        let result = RegisterIndex(3);
        let program = BytecodeProgram::test_entry(
            vec![
                Instruction::ResetNonTrailed {
                    dest: slot,
                    span: span(),
                },
                Instruction::JumpIfInitialized {
                    source: ValueOperand::Register(slot),
                    jump_offset: 4,
                    span: span(),
                },
                Instruction::MoveTrailed {
                    dest: slot,
                    source: ValueOperand::Constant(0),
                    span: span(),
                },
                Instruction::JumpIfInitialized {
                    source: ValueOperand::Register(slot),
                    jump_offset: 5,
                    span: span(),
                },
                Instruction::Err { span: span() },
                Instruction::Switch {
                    which: ValueOperand::Register(slot),
                    jump_offsets: vec![6, 8],
                    span: span(),
                },
                Instruction::Err { span: span() },
                Instruction::Err { span: span() },
                Instruction::Tracepoint {
                    name: "after-switch".to_string(),
                    span: span(),
                },
                Instruction::MoveNonComparable {
                    dest: result,
                    source: ValueOperand::Constant(1),
                    span: span(),
                },
                Instruction::Return {
                    value: ValueOperand::Register(result),
                    span: span(),
                },
            ],
            vec![Constant::Int(1), Constant::Int(42)],
            4,
        );
        let value = BytecodeExecutor::new(&program)
            .run()
            .expect("test bytecode should run");
        assert_eq!(value, Value::Int(42));
    }

    #[test]
    fn official_array_append_instructions_execute() {
        let left = RegisterIndex(2);
        let right = RegisterIndex(3);
        let combined = RegisterIndex(4);
        let length = RegisterIndex(5);
        let leniency = RegisterIndex(6);
        let program = BytecodeProgram::test_entry(
            vec![
                Instruction::NewMutableArray {
                    dest: left,
                    values: vec![ValueOperand::Constant(0), ValueOperand::Constant(1)],
                    span: span(),
                },
                Instruction::NewArray {
                    dest: right,
                    values: vec![ValueOperand::Constant(2), ValueOperand::Constant(3)],
                    span: span(),
                },
                Instruction::CanFastAppendToArrayFastFail {
                    leniency_indicator: leniency,
                    ref_value: ValueOperand::Uninitialized,
                    maybe_mutable_array: ValueOperand::Register(left),
                    on_failure: 7,
                    span: span(),
                },
                Instruction::FastAppendToArray {
                    left_source: ValueOperand::Register(left),
                    right_source: ValueOperand::Register(right),
                    span: span(),
                },
                Instruction::MutableAdd {
                    dest: combined,
                    left_source: ValueOperand::Register(left),
                    right_source: ValueOperand::Register(right),
                    span: span(),
                },
                Instruction::Length {
                    dest: length,
                    container: ValueOperand::Register(combined),
                    span: span(),
                },
                Instruction::Return {
                    value: ValueOperand::Register(length),
                    span: span(),
                },
                Instruction::Err { span: span() },
            ],
            vec![
                Constant::Int(10),
                Constant::Int(20),
                Constant::Int(30),
                Constant::Int(40),
            ],
            7,
        );
        let value = BytecodeExecutor::new(&program)
            .run()
            .expect("test bytecode should run");
        assert_eq!(value, Value::Int(6));
    }

    #[test]
    fn official_effect_and_trailing_value_instructions_execute() {
        let array = RegisterIndex(2);
        let frozen = RegisterIndex(3);
        let melted = RegisterIndex(4);
        let ref_value = RegisterIndex(5);
        let ref_length = RegisterIndex(6);
        let forwarded_ref = RegisterIndex(7);
        let forwarded_value = RegisterIndex(8);
        let final_length = RegisterIndex(9);
        let program = BytecodeProgram::test_entry(
            vec![
                Instruction::NewArray {
                    dest: array,
                    values: vec![ValueOperand::Constant(0), ValueOperand::Constant(1)],
                    span: span(),
                },
                Instruction::Freeze {
                    dest: frozen,
                    value: ValueOperand::Register(array),
                    span: span(),
                },
                Instruction::Melt {
                    dest: melted,
                    value: ValueOperand::Register(frozen),
                    span: span(),
                },
                Instruction::NewRef {
                    dest: ref_value,
                    domain: None,
                    span: span(),
                },
                Instruction::RefSetLive {
                    ref_value: ValueOperand::Register(ref_value),
                    value: ValueOperand::Register(melted),
                    task: ValueOperand::Constant(2),
                    span: span(),
                },
                Instruction::LengthWithEffects {
                    dest: ref_length,
                    container: ValueOperand::Register(ref_value),
                    span: span(),
                },
                Instruction::CallSetLive {
                    container: ValueOperand::Register(melted),
                    index: ValueOperand::Constant(2),
                    value_to_set: ValueOperand::Constant(3),
                    task: ValueOperand::Constant(2),
                    span: span(),
                },
                Instruction::FreezeIfAccessor {
                    dest: forwarded_ref,
                    value: ValueOperand::Register(ref_value),
                    span: span(),
                },
                Instruction::RefGet {
                    dest: forwarded_value,
                    ref_value: ValueOperand::Register(forwarded_ref),
                    span: span(),
                },
                Instruction::Length {
                    dest: final_length,
                    container: ValueOperand::Register(forwarded_value),
                    span: span(),
                },
                Instruction::ReturnTrailed {
                    value: ValueOperand::Register(final_length),
                    span: span(),
                },
            ],
            vec![
                Constant::Int(10),
                Constant::Int(20),
                Constant::Int(0),
                Constant::Int(99),
            ],
            10,
        );
        let value = BytecodeExecutor::new(&program)
            .run()
            .expect("test bytecode should run");
        assert_eq!(value, Value::Int(2));
    }
}
