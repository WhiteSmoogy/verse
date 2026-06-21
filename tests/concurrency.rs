//! Auto-grouped from the former monolithic tests/language.rs.
//! Shared helpers live in tests/common/mod.rs.

mod common;
use common::*;

#[test]
fn rejects_if_failure_binding_outside_then_branch() {
    let error = check_source(
        r#"
Values:[]int = array{42}
if (Value := Values[0]):
    Value
else:
    Value
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("undefined name `Value`"));
}

#[test]
fn checks_official_modifier_stack_member_surface() {
    let source = r#"
Use(Stack:modifier_stack(int), Modifier:modifier(int))<transacts>:void =
    block:
        First:?rational = Stack.FirstPosition
        Last:?rational = Stack.LastPosition
        Value:int = Stack.Evaluate(40)
        Modified:int = Modifier.Evaluate(Value)
        Subscription:cancelable = Stack.AddModifier(Modifier, 1)
        print("done")
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(2),
            vec!["transacts".to_string()],
            Some(vec![
                Type::ModifierStack(Box::new(Type::Int)),
                Type::Modifier(Box::new(Type::Int)),
            ]),
            Type::None
        )
    );
}

#[test]
fn checks_modifier_stack_assignable_to_modifier_interface() {
    let source = r#"
AsModifier(Stack:modifier_stack(int)):modifier(int) = Stack
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::ModifierStack(Box::new(Type::Int))]),
            Type::Modifier(Box::new(Type::Int))
        )
    );
}

#[test]
fn evaluates_external_modifier_as_identity_runtime_modifier() {
    let source = r#"
Modifier:modifier(int) = external {}
Modifier.Evaluate(42)
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
    assert_eq!(eval(source), Value::Int(42));
}

#[test]
fn evaluates_external_modifier_stack_as_empty_runtime_stack() {
    let source = r#"
Stack:modifier_stack(int) = external {}
NoFirst := if (Stack.FirstPosition?). 0 else. 20
NoLast := if (Stack.LastPosition?). 0 else. 22
NoFirst + NoLast + Stack.Evaluate(0)
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
    assert_eq!(eval(source), Value::Int(42));
}

#[test]
fn evaluates_user_modifier_stack_ordering_and_cancel_runtime() {
    let source = r#"
add := class(modifier(int)):
    Amount:int
    Evaluate<override>(InValue:int):int =
        InValue + Amount

multiply := class(modifier(int)):
    Factor:int
    Evaluate<override>(InValue:int):int =
        InValue * Factor

Stack:modifier_stack(int) = external {}
Stack.AddModifier(add{Amount := 2}, 0)
Handle:cancelable = Stack.AddModifier(multiply{Factor := 10}, 0)
BeforeCancel:int = Stack.Evaluate(4)
Handle.Cancel()
AfterCancel:int = Stack.Evaluate(4)
FirstIsZero := if (Position := Stack.FirstPosition?, Position = 0). 1 else. 0
BeforeCancel + AfterCancel + FirstIsZero
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
    assert_eq!(eval(source), Value::Int(67));
}

#[test]
fn rolls_back_modifier_stack_add_modifier_in_failure_context() {
    let source = r#"
add := class(modifier(int)):
    Amount:int
    Evaluate<override>(InValue:int):int =
        InValue + Amount

Stack:modifier_stack(int) = external {}
if:
    Stack.AddModifier(add{Amount := 40}, 0)
    false?
then:
    0
else:
    Stack.Evaluate(2)
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
    assert_eq!(eval(source), Value::Int(2));
}

#[test]
fn rejects_modifier_class_missing_evaluate_implementation() {
    let error = check_source(
        r#"
bad := class(modifier(int)):
    Value:int = 0
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("must be `abstract` or implement method `Evaluate`")
    );
}

#[test]
fn checks_official_generator_type_alias_and_for_iteration() {
    let source = r#"
int_generator := generator(int)
Collect(Values:int_generator):[]int =
    for (Value : Values):
        Value
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::Generator(Some(Box::new(Type::Int)))]),
            Type::Array(Box::new(Type::Int))
        )
    );
}

#[test]
fn checks_official_parameterless_generator_for_iteration() {
    let source = r#"
Collect(Values:generator()):[]any =
    for (Value : Values):
        Value
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::Generator(None)]),
            Type::Array(Box::new(Type::Any))
        )
    );
}

#[test]
fn checks_official_task_await_member() {
    let source = r#"
Wait(Task:task(int))<suspends>:int = Task.Await()
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            vec!["suspends".to_string()],
            Some(vec![Type::Task(Box::new(Type::Int))]),
            Type::Int
        )
    );
}

#[test]
fn checks_official_task_subtype_of_awaitable() {
    let source = r#"
AcceptAwaitable(Source:awaitable(int)):int = 42
Use(Task:task(int)):int = AcceptAwaitable(Task)
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::Task(Box::new(Type::Int))]),
            Type::Int
        )
    );
}

#[test]
fn checks_official_spawn_expression_returns_task() {
    let source = r#"
Compute()<suspends>:int = 42
Start():task(int) = spawn{Compute()}
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            Vec::<String>::new(),
            Some(vec![]),
            Type::Task(Box::new(Type::Int))
        )
    );
}

#[test]
fn checks_official_spawn_task_can_be_awaited_in_async_context() {
    let source = r#"
Compute()<suspends>:int = 42
Start()<suspends>:int =
    Task := spawn{Compute()}
    Task.Await()
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            vec!["suspends".to_string()],
            Some(vec![]),
            Type::Int
        )
    );
}

#[test]
fn checks_official_event_await_and_signal_members() {
    let source = r#"
WaitForPayload(Event:event(int))<suspends>:int = Event.Await()
Notify(Event:event(int)):void = Event.Signal(42)
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::Event(Some(Box::new(Type::Int)))]),
            Type::None
        )
    );
}

#[test]
fn checks_official_parameterless_event_members() {
    let source = r#"
Wait(Event:event())<suspends>:void = Event.Await()
Notify(Event:event()):void = Event.Signal()
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::Event(None)]),
            Type::None
        )
    );
}

#[test]
fn evaluates_official_parameterless_event_construction() {
    let source = r#"
Done:event() = event(){}
Done.Signal()
42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_payload_event_construction_and_signal() {
    let source = r#"
Payload:event(int) = event(int){}
Payload.Signal(7)
42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_tuple_payload_event_construction_and_expanded_signal() {
    let source = r#"
Move:event(tuple(int, string)) = event(tuple(int, string)){}
Move.Signal(7, "go")
42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_official_event_construction_with_tuple_payload_type() {
    let source = r#"
Move:event(tuple(int, string)) = event(tuple(int, string)){}
Notify():void = Move.Signal(7, "go")
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (Some(0), Vec::<String>::new(), Some(vec![]), Type::None)
    );
}

#[test]
fn evaluates_official_race_ignores_unsignaled_event_await_pending_branch() {
    let source = r#"
var Result:int = 0
Ready:event() = event(){}
WaitForReady()<suspends><transacts>:int =
    Ready.Await()
    999
Immediate()<suspends><transacts>:int =
    Sleep(-1.0)
    40
Run()<suspends><transacts>:void =
    Winner := race:
        WaitForReady()
        Immediate()
    set Result = Winner + 2
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_task_await_outside_async_context_through_run_source() {
    let source = r#"
Ready:event() = event(){}
WaitForReady()<suspends>:void =
    Ready.Await()
Task:task(void) = spawn{WaitForReady()}
Task.Await()
"#;

    let error = run_source(source).expect_err("source should fail");
    assert!(
        error
            .to_string()
            .contains("function with `<suspends>` effect can only be called in an async context")
    );
}

#[test]
fn evaluates_official_event_signal_resumes_spawned_await_task() {
    let source = r#"
var Result:int = 0
Ready:event() = event(){}
WaitForReady()<suspends><transacts>:void =
    Ready.Await()
    set Result = 42
spawn{WaitForReady()}
Ready.Signal()
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_event_signal_payload_resumes_await_binding() {
    let source = r#"
var Result:int = 0
Payload:event(int) = event(int){}
WaitForPayload()<suspends><transacts>:void =
    Value := Payload.Await()
    set Result = Value + 1
spawn{WaitForPayload()}
Payload.Signal(41)
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_event_signal_resumes_waiters_fifo() {
    let source = r#"
var Result:int = 0
Ready:event() = event(){}
First()<suspends><transacts>:void =
    Ready.Await()
    set Result = Result * 10 + 1
Second()<suspends><transacts>:void =
    Ready.Await()
    set Result = Result * 10 + 2
spawn{First()}
spawn{Second()}
Ready.Signal()
Result
"#;

    assert_eq!(eval(source), Value::Int(12));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_event_signal_does_not_cache_for_future_awaiters() {
    let source = r#"
var Result:int = 0
Ready:event() = event(){}
WaitForReady()<suspends><transacts>:void =
    Ready.Await()
    set Result = 42
Ready.Signal()
spawn{WaitForReady()}
Result
"#;

    assert_eq!(eval(source), Value::Int(0));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_event_signal_new_await_waits_for_next_signal() {
    let source = r#"
var Result:int = 0
Ready:event() = event(){}
WaitTwice()<suspends><transacts>:void =
    Ready.Await()
    set Result += 1
    Ready.Await()
    set Result += 10
spawn{WaitTwice()}
Ready.Signal()
AfterFirst:int = Result
Ready.Signal()
AfterFirst * 100 + Result
"#;

    assert_eq!(eval(source), Value::Int(111));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_official_event_construction_payload_type_mismatch() {
    let error = check_source("Bad:event(int) = event(string){}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("annotated as `event(int)` but expression has type `event(string)`")
    );
}

#[test]
fn rejects_official_event_construction_with_body_entries() {
    let error =
        check_source("Bad:event(int) = event(int){Value := 1}").expect_err("source should fail");

    assert!(error.to_string().contains("expects an empty body"));
}

#[test]
fn rejects_official_event_construction_wrong_arity() {
    let error =
        check_source("Bad:event(int) = event(int, string){}").expect_err("source should fail");

    assert!(error.to_string().contains("expected 0 or 1 type arguments"));
}

#[test]
fn rejects_non_event_call_archetype_syntax() {
    let error = parse_source("Value := Foo(){}").expect_err("source should fail");

    assert!(
        error.to_string().contains("expected")
            || error.to_string().contains("unexpected")
            || error.to_string().contains("extra")
    );
}

#[test]
fn checks_official_awaitable_signalable_subscribable_members() {
    let source = r#"
Wait(Waitable:awaitable(int))<suspends>:int = Waitable.Await()
Notify(Signalable:signalable(int)):void = Signalable.Signal(42)
Handler(Value:int):void = {}
SubscribeTo(Source:subscribable(int)):cancelable = Source.Subscribe(Handler)
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::Subscribable(Some(Box::new(Type::Int)))]),
            Type::Interface("cancelable".into())
        )
    );
}

#[test]
fn evaluates_external_event_and_signalable_runtime_signal() {
    let source = r#"
Done:event() = external {}
Payload:event(int) = external {}
Signal:signalable(int) = external {}
Done.Signal()
Payload.Signal(7)
Signal.Signal(42)
42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_external_subscribable_subscribe_and_cancel_runtime() {
    let source = r#"
Handler(Value:int):void = {}
Source:subscribable(int) = external {}
Handle:cancelable = Source.Subscribe(Handler)
Handle.Cancel()
42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_external_parameterless_subscribable_subscribe_and_cancel_runtime() {
    let source = r#"
Handler():void = {}
Source:subscribable() = external {}
Handle:cancelable = Source.Subscribe(Handler)
Handle.Cancel()
42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_external_listenable_subscribe_and_cancel_runtime() {
    let source = r#"
Handler(Value:int):void = {}
Source:listenable(int) = external {}
Handle:cancelable = Source.Subscribe(Handler)
Handle.Cancel()
42
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_external_subscribable_bad_callback_through_run_source() {
    let error = run_source("Source:subscribable(int) = external {}\nSource.Subscribe(42)")
        .expect_err("source should runtime error");

    assert!(
        error
            .to_string()
            .contains("argument 1 expected `function/1 -> none`, got `int`")
    );
}

#[test]
fn checks_official_listenable_exposed_awaitable_and_subscribable_members() {
    let source = r#"
Wait(Source:listenable(int))<suspends>:int = Source.Await()
Handler(Value:int):void = {}
SubscribeTo(Source:listenable(int)):cancelable = Source.Subscribe(Handler)
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::Listenable(Some(Box::new(Type::Int)))]),
            Type::Interface("cancelable".into())
        )
    );
}

#[test]
fn checks_official_event_subtype_of_awaitable_and_signalable() {
    let source = r#"
AcceptAwaitable(Source:awaitable(int)):int = 1
AcceptSignalable(Source:signalable(int)):int = 2
Use(Event:event(int)):int = AcceptAwaitable(Event) + AcceptSignalable(Event)
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::Event(Some(Box::new(Type::Int)))]),
            Type::Int
        )
    );
}

#[test]
fn checks_official_listenable_subtype_of_awaitable_and_subscribable() {
    let source = r#"
AcceptAwaitable(Source:awaitable(int)):int = 1
AcceptSubscribable(Source:subscribable(int)):int = 2
Use(Source:listenable(int)):int = AcceptAwaitable(Source) + AcceptSubscribable(Source)
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(1),
            Vec::<String>::new(),
            Some(vec![Type::Listenable(Some(Box::new(Type::Int)))]),
            Type::Int
        )
    );
}

#[test]
fn rejects_official_awaitable_await_outside_async_context() {
    let error = check_source("Wait(Waitable:awaitable(int)):int = Waitable.Await()")
        .expect_err("source should fail");

    assert!(error.to_string().contains("async context"));
}

#[test]
fn rejects_official_task_await_outside_async_context() {
    let error =
        check_source("Wait(Task:task(int)):int = Task.Await()").expect_err("source should fail");

    assert!(error.to_string().contains("async context"));
}

#[test]
fn rejects_official_event_signal_payload_type_mismatch() {
    let error = check_source(r#"Notify(Event:event(int)):void = Event.Signal("bad")"#)
        .expect_err("source should fail");

    assert!(error.to_string().contains("argument 1 expected `int`"));
}

#[test]
fn rejects_official_event_signal_in_failure_context() {
    let error = check_source(
        r#"
Notify(Event:event(int))<decides><transacts>:void = Event.Signal(42)
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("function with `<no_rollback>` effect cannot be called in a failure context")
    );
}

#[test]
fn rejects_official_subscribable_callback_type_mismatch() {
    let error = check_source(
        r#"
Bad(Value:string):void = {}
SubscribeTo(Source:subscribable(int)):cancelable = Source.Subscribe(Bad)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("argument 1 expected"));
}

#[test]
fn rejects_official_awaitable_unknown_signal_member() {
    let error = check_source("Notify(Waitable:awaitable(int)):void = Waitable.Signal(42)")
        .expect_err("source should fail");

    assert!(error.to_string().contains("has no member `Signal`"));
}

#[test]
fn rejects_official_task_unknown_signal_member() {
    let error = check_source("Notify(Task:task(int)):void = Task.Signal(42)")
        .expect_err("source should fail");

    assert!(error.to_string().contains("has no member `Signal`"));
}

#[test]
fn rejects_official_spawn_of_non_suspends_call() {
    let error = check_source(
        r#"
Immediate():int = 42
Bad():task(int) = spawn{Immediate()}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("`<suspends>` effect"));
}

#[test]
fn rejects_official_spawn_body_with_multiple_expressions() {
    let error = check_source(
        r#"
Compute()<suspends>:int = 42
Bad():task(int) = spawn{Compute(); Compute()}
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("exactly one expression"));
}

#[test]
fn evaluates_official_spawn_expression_runtime_task() {
    let source = r#"
Compute()<suspends>:int = 42
spawn{Compute()}
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Task(Box::new(Type::Int))
    );

    assert!(matches!(eval(source), Value::Task(_)));
}

#[test]
fn evaluates_official_spawn_task_await_runtime() {
    let source = r#"
var Result:int = 0
Compute()<suspends>:int = 42
Run()<suspends><transacts>:void =
    Task:task(int) = spawn{Compute()}
    set Result = Task.Await()
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn checks_official_sync_expression_tuple_result() {
    let source = r#"
ComputeScore()<suspends>:int = 42
ComputeLabel()<suspends>:string = "done"
Run()<suspends>:tuple(int, string) =
    sync:
        ComputeScore()
        ComputeLabel()
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            vec!["suspends".to_string()],
            Some(vec![]),
            Type::Tuple(vec![Type::Int, Type::String])
        )
    );
}

#[test]
fn checks_official_race_expression_result() {
    let source = r#"
Fast()<suspends>:int = 1
Slow()<suspends>:int = 2
Run()<suspends>:int =
    race:
        Fast()
        Slow()
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            vec!["suspends".to_string()],
            Some(vec![]),
            Type::Int
        )
    );
}

#[test]
fn checks_official_rush_expression_result() {
    let source = r#"
Fast()<suspends>:int = 1
Slow()<suspends>:int = 2
Run()<suspends>:int =
    rush:
        Fast()
        Slow()
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            vec!["suspends".to_string()],
            Some(vec![]),
            Type::Int
        )
    );
}

#[test]
fn checks_official_branch_expression_returns_void() {
    let source = r#"
Work()<suspends>:void = {}
Run()<suspends>:void =
    branch:
        Work()
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            vec!["suspends".to_string()],
            Some(vec![]),
            Type::None
        )
    );
}

#[test]
fn rejects_official_structured_concurrency_outside_async_context() {
    let error = check_source(
        r#"
Fast()<suspends>:int = 1
Slow()<suspends>:int = 2
Run():tuple(int, int) =
    sync:
        Fast()
        Slow()
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("async context"));
}

#[test]
fn rejects_official_structured_concurrency_binding_after_body() {
    let error = check_source(
        r#"
F()<suspends>:int = 42
G()<suspends>:int = 0
H(Value:int):int = Value
Run()<suspends>:int =
    race:
        X := F()
        G()
    H(X)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("undefined name `X`"));
}

#[test]
fn rejects_official_structured_concurrency_sibling_branch_binding_access() {
    for op in ["sync", "race", "rush", "branch"] {
        let source = format!(
            r#"
First()<suspends>:int = 1
Use(Value:int)<suspends>:int = Value
Run()<suspends>:void =
    {op}:
        X := First()
        Use(X)
"#
        );

        let error = check_source(&source).expect_err("source should fail");
        assert!(
            error.to_string().contains("undefined name `X`"),
            "{op} should isolate branch-local bindings, got {error}"
        );
    }
}

#[test]
fn rejects_official_structured_concurrency_immediate_branch() {
    for op in ["sync", "race", "rush", "branch"] {
        let source = format!(
            r#"
Wait()<suspends>:int = 1
Run()<suspends>:void =
    {op}:
        1
        Wait()
"#
        );

        let error = check_source(&source).expect_err("source should fail");
        assert!(
            error.to_string().contains("async expression"),
            "{op} should reject an immediate branch, got {error}"
        );
    }
}

#[test]
fn checks_official_structured_concurrency_async_block_branches() {
    let source = r#"
Wait()<suspends>:void = Sleep(-1.0)
Run()<suspends>:tuple(int, int) =
    sync:
        block:
            Wait()
            40
        block:
            Wait()
            2
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            vec!["suspends".to_string()],
            Some(vec![]),
            Type::Tuple(vec![Type::Int, Type::Int])
        )
    );
}

#[test]
fn checks_official_structured_concurrency_tuple_subexpression_suspends() {
    let source = r#"
Wait(Value:int)<suspends>:int =
    Sleep(-1.0)
    Value
Run()<suspends>:tuple(tuple(int, int), int) =
    sync:
        (Wait(40), 1)
        Wait(2)
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            vec!["suspends".to_string()],
            Some(vec![]),
            Type::Tuple(vec![Type::Tuple(vec![Type::Int, Type::Int]), Type::Int])
        )
    );
}

#[test]
fn checks_official_structured_concurrency_var_initializer_suspends() {
    let source = r#"
Wait(Value:int)<suspends>:int =
    Sleep(-1.0)
    Value
Run()<suspends>:tuple(int, int) =
    sync:
        var First:int = Wait(40)
        Wait(2)
"#;

    assert_eq!(
        function_shape(check_source(source).expect("source should check")),
        (
            Some(0),
            vec!["suspends".to_string()],
            Some(vec![]),
            Type::Tuple(vec![Type::Int, Type::Int])
        )
    );
}

#[test]
fn checks_official_structured_concurrency_branches_allow_same_local_name() {
    let sync_source = r#"
First()<suspends>:int = 1
Second()<suspends>:int = 2
Run()<suspends>:tuple(int, int) =
    sync:
        X := First()
        X := Second()
"#;

    assert_eq!(
        function_shape(check_source(sync_source).expect("source should check")),
        (
            Some(0),
            vec!["suspends".to_string()],
            Some(vec![]),
            Type::Tuple(vec![Type::Int, Type::Int])
        )
    );

    for op in ["race", "rush"] {
        let source = format!(
            r#"
First()<suspends>:int = 1
Second()<suspends>:int = 2
Run()<suspends>:int =
    {op}:
        X := First()
        X := Second()
"#
        );

        assert_eq!(
            function_shape(check_source(&source).expect("source should check")),
            (
                Some(0),
                vec!["suspends".to_string()],
                Some(vec![]),
                Type::Int
            )
        );
    }

    let branch_source = r#"
First()<suspends>:int = 1
Second()<suspends>:int = 2
Run()<suspends>:void =
    branch:
        X := First()
        X := Second()
"#;

    assert_eq!(
        function_shape(check_source(branch_source).expect("source should check")),
        (
            Some(0),
            vec!["suspends".to_string()],
            Some(vec![]),
            Type::None
        )
    );
}

#[test]
fn runtime_errors_on_structured_concurrency_sibling_branch_binding_access() {
    let source = r#"
First()<suspends>:int = 1
Run()<suspends>:int =
    Values := sync:
        X := First()
        X
    Values(1)
Task:task(int) = spawn{Run()}
Task.Await()
"#;

    let error = run_source(source).expect_err("source should runtime error");

    assert!(error.to_string().contains("undefined name `X`"));
}

#[test]
fn evaluates_official_sync_expression_runtime_tuple_result() {
    let source = r#"
var Result:int = 0
ComputeScore()<suspends>:int = 42
ComputeLabel()<suspends>:string = "done"
Run()<suspends><transacts>:void =
    Values := sync:
        ComputeScore()
        ComputeLabel()
    set Result = Values(0)
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_sync_tuple_subexpression_suspension_runtime() {
    let source = r#"
var Result:int = 0
Wait(Value:int)<suspends>:int =
    Sleep(0.0)
    Value
Run()<suspends><transacts>:void =
    Values := sync:
        (Wait(40), 1)
        Wait(1)
    Pair := Values(0)
    set Result = Pair(0) + Pair(1) + Values(1)
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_sync_var_initializer_suspension_runtime() {
    let source = r#"
var Result:int = 0
Wait(Value:int)<suspends>:int =
    Sleep(0.0)
    Value
Run()<suspends><transacts>:void =
    Values := sync:
        var First:int = Wait(40)
        Wait(2)
    set Result = Values(0) + Values(1)
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_sync_expression_starts_later_signal_branch() {
    let source = r#"
var Result:int = 0
Ready:event() = event(){}
Waiter()<suspends>:int =
    Ready.Await()
    40
Signaler()<suspends><transacts>:int =
    Ready.Signal()
    2
Run()<suspends><transacts>:void =
    Values := sync:
        Waiter()
        Signaler()
    set Result = Values(0) + Values(1)
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_race_expression_runtime_cancels_losing_immediate_branch() {
    let source = r#"
var Total:int = 0
Fast()<suspends><transacts>:int =
    set Total += 1
    1
Slow()<suspends><transacts>:int =
    set Total += 10
    2
Run()<suspends><transacts>:int =
    Winner := race:
        Fast()
        Slow()
    Winner + Total
var Result:int = 0
Start()<suspends><transacts>:void =
    set Result = Run()
spawn{Start()}
Result
"#;

    assert_eq!(eval(source), Value::Int(2));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_race_expression_event_signal_can_resume_winner() {
    let source = r#"
var Result:int = 0
Ready:event() = event(){}
Waiter()<suspends>:int =
    Ready.Await()
    40
Signaler()<suspends><transacts>:int =
    Sleep(0.0)
    Ready.Signal()
    2
Run()<suspends><transacts>:void =
    Winner := race:
        Waiter()
        Signaler()
    set Result = Winner + 2
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_race_cancels_losing_sync_child_tasks() {
    let source = r#"
var Trace:int = 0
SlowChild()<suspends><transacts>:int =
    Sleep(0.001)
    set Trace = Trace * 10 + 9
    9
LosingSync()<suspends>:int =
    Values := sync:
        SlowChild()
        SlowChild()
    Values(0)
Winner()<suspends><transacts>:int =
    Sleep(0.0)
    set Trace = Trace * 10 + 1
    40
Run()<suspends><transacts>:void =
    WinnerValue := race:
        LosingSync()
        Winner()
    set Trace = Trace * 100 + WinnerValue
spawn{Run()}
AfterRace:int = Trace
AfterCanceledTimers:int = Trace
AfterCanceledTimers
"#;

    assert_eq!(eval(source), Value::Int(140));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_race_cancels_losing_nested_race_child_tasks() {
    let source = r#"
var Trace:int = 0
NestedSlow()<suspends><transacts>:int =
    Sleep(0.001)
    set Trace = Trace * 10 + 9
    9
NestedNever()<suspends>:int =
    Sleep(Inf)
    7
LosingRace()<suspends>:int =
    race:
        NestedSlow()
        NestedNever()
Winner()<suspends><transacts>:int =
    Sleep(0.0)
    set Trace = Trace * 10 + 1
    40
Run()<suspends><transacts>:void =
    WinnerValue := race:
        LosingRace()
        Winner()
    set Trace = Trace * 100 + WinnerValue
spawn{Run()}
AfterRace:int = Trace
AfterCanceledTimers:int = Trace
AfterCanceledTimers
"#;

    assert_eq!(eval(source), Value::Int(140));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_race_cancellation_runs_losing_branch_defer() {
    let source = r#"
var Trace:int = 0
Loser()<suspends><transacts>:int =
    defer:
        set Trace = Trace * 10 + 9
    Sleep(0.001)
    set Trace = Trace * 10 + 8
    2
Winner()<suspends><transacts>:int =
    Sleep(0.0)
    set Trace = Trace * 10 + 1
    40
Run()<suspends><transacts>:void =
    WinnerValue := race:
        Loser()
        Winner()
    set Trace = Trace * 100 + WinnerValue
spawn{Run()}
AfterRace:int = Trace
AfterCanceledTimers:int = Trace
AfterCanceledTimers
"#;

    assert_eq!(eval(source), Value::Int(1940));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_race_cancellation_runs_losing_sync_child_defers() {
    let source = r#"
var Trace:int = 0
SlowChild()<suspends><transacts>:int =
    defer:
        set Trace = Trace * 10 + 9
    Sleep(0.001)
    set Trace = Trace * 10 + 8
    8
LosingSync()<suspends><transacts>:int =
    Values := sync:
        SlowChild()
        SlowChild()
    Values(0)
Winner()<suspends><transacts>:int =
    Sleep(0.0)
    set Trace = Trace * 10 + 1
    40
Run()<suspends><transacts>:void =
    WinnerValue := race:
        LosingSync()
        Winner()
    set Trace = Trace * 100 + WinnerValue
spawn{Run()}
AfterRace:int = Trace
AfterCanceledTimers:int = Trace
AfterCanceledTimers
"#;

    assert_eq!(eval(source), Value::Int(19940));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_rush_expression_runtime_continues_losing_immediate_branch() {
    let source = r#"
var Total:int = 0
Fast()<suspends><transacts>:int =
    set Total += 1
    1
Slow()<suspends><transacts>:int =
    set Total += 10
    2
Run()<suspends><transacts>:int =
    Winner := rush:
        Fast()
        Slow()
    Winner + Total
var Result:int = 0
Start()<suspends><transacts>:void =
    set Result = Run()
spawn{Start()}
Result
"#;

    assert_eq!(eval(source), Value::Int(12));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_rush_cancels_suspended_loser_when_scope_completes() {
    let source = r#"
var Trace:int = 0
Fast()<suspends><transacts>:int =
    Sleep(-1.0)
    set Trace = Trace * 10 + 1
    40
Slow()<suspends><transacts>:int =
    defer:
        set Trace = Trace * 10 + 9
    Sleep(0.0)
    set Trace = 999
    2
Run()<suspends><transacts>:void =
    Winner := rush:
        Fast()
        Slow()
    set Trace = Trace * 10 + Winner
spawn{Run()}
AfterRun:int = Trace
AfterCanceledTimers:int = Trace
AfterCanceledTimers
"#;

    assert_eq!(eval(source), Value::Int(509));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_rush_expression_event_signal_can_resume_winner_and_continue_loser() {
    let source = r#"
var Trace:int = 0
Ready:event() = event(){}
Waiter()<suspends><transacts>:int =
    Ready.Await()
    set Trace = Trace * 10 + 1
    40
Signaler()<suspends><transacts>:int =
    Sleep(0.0)
    Ready.Signal()
    set Trace = Trace * 10 + 2
    2
Run()<suspends><transacts>:void =
    Winner := rush:
        Waiter()
        Signaler()
    set Trace = Trace * 10 + Winner
spawn{Run()}
Trace
"#;

    assert_eq!(eval(source), Value::Int(502));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_branch_expression_runtime_returns_void_after_starting_body() {
    let source = r#"
var Total:int = 0
Work()<suspends><transacts>:void =
    set Total += 40
Run()<suspends><transacts>:int =
    branch:
        Work()
    Total + 2
var Result:int = 0
Start()<suspends><transacts>:void =
    set Result = Run()
spawn{Start()}
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_branch_event_signal_resumes_started_body() {
    let source = r#"
var Result:int = 0
Ready:event() = event(){}
Work()<suspends><transacts>:void =
    Ready.Await()
    set Result = Result * 10 + 2
Run()<suspends><transacts>:void =
    branch:
        Work()
    set Result = Result * 10 + 1
    Ready.Signal()
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Int(12));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_branch_unsignaled_event_does_not_block_following_expression() {
    let source = r#"
var Result:int = 0
Ready:event() = event(){}
Work()<suspends><transacts>:void =
    Ready.Await()
    set Result = 99
Run()<suspends><transacts>:void =
    branch:
        Work()
    set Result = 42
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_branch_cancellation_runs_scoped_defer() {
    let source = r#"
var Trace:int = 0
Ready:event() = event(){}
Work()<suspends><transacts>:void =
    defer:
        set Trace = Trace * 10 + 9
    Ready.Await()
    set Trace = 99
Run()<suspends><transacts>:void =
    branch:
        Work()
    set Trace = Trace * 10 + 1
spawn{Run()}
Trace
"#;

    assert_eq!(eval(source), Value::Int(19));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_suspended_call_argument_after_multiple_yields() {
    let source = r#"
var Result:int = 0
AddOne(Value:int):int = Value + 1
Worker()<suspends>:int =
    Sleep(0.0)
    Sleep(0.0)
    41
Run()<suspends><transacts>:void =
    set Result = AddOne(Worker())
spawn{Run()}
AfterFirst:int = Result
AfterSecond:int = Result
AfterSecond
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_suspended_binary_operand_after_multiple_yields() {
    let source = r#"
var Result:int = 0
Worker()<suspends>:int =
    Sleep(0.0)
    Sleep(0.0)
    41
Run()<suspends><transacts>:void =
    set Result = Worker() + 1
spawn{Run()}
AfterFirst:int = Result
AfterSecond:int = Result
AfterSecond
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_suspended_compound_assignment_rhs_after_multiple_yields() {
    let source = r#"
var Result:int = 1
Worker()<suspends>:int =
    Sleep(0.0)
    Sleep(0.0)
    41
Run()<suspends><transacts>:void =
    set Result += Worker()
spawn{Run()}
AfterFirst:int = Result
AfterSecond:int = Result
AfterSecond
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_task_await_inside_call_argument_runtime() {
    let source = r#"
var Result:int = 0
AddOne(Value:int):int = Value + 1
Worker()<suspends>:int =
    Sleep(0.001)
    41
Run()<suspends><transacts>:void =
    Task:task(int) = spawn{Worker()}
    set Result = AddOne(Task.Await())
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_task_await_inside_array_tuple_and_map_runtime() {
    let source = r#"
var Result:int = 0
Worker()<suspends>:int =
    Sleep(0.001)
    40
Run()<suspends><transacts>:void =
    Task:task(int) = spawn{Worker()}
    Values:[]int = array{Task.Await(), 2}
    if:
        First := Values[0]
        Second := Values[1]
        Scores:[string]int = map{"answer" => First}
        Answer := Scores["answer"]
    then:
        Pair:tuple(int, int) = (Answer, Second)
        set Result = Pair(0) + Pair(1)
    else:
        set Result = 0
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_task_await_inside_map_key_runtime() {
    let source = r#"
var Result:int = 0
Worker()<suspends>:string =
    Sleep(0.001)
    "answer"
Run()<suspends><transacts>:void =
    Task:task(string) = spawn{Worker()}
    Scores:[string]int = map{Task.Await() => 42}
    if (Score := Scores["answer"]):
        set Result = Score
    else:
        set Result = 0
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_task_await_inside_bracket_argument_runtime() {
    let source = r#"
var Result:int = 0
Worker()<suspends>:int =
    Sleep(0.001)
    1
Run()<suspends><transacts>:void =
    Task:task(int) = spawn{Worker()}
    Values:[]int = array{0, 42}
    Index:int = Task.Await()
    if (Value := Values[Index]):
        set Result = Value
    else:
        set Result = 0
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_task_await_as_index_collection_runtime() {
    let source = r#"
var Result:int = 0
Worker()<suspends>:[]int =
    Sleep(0.001)
    array{0, 42}
Run()<suspends><transacts>:void =
    Task:task([]int) = spawn{Worker()}
    Values:[]int = Task.Await()
    if (Value := Values[1]):
        set Result = Value
    else:
        set Result = 0
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_task_await_as_member_receiver_runtime() {
    let source = r#"
var Result:int = 0
Worker()<suspends>:[]int =
    Sleep(0.001)
    array{40, 2}
Run()<suspends><transacts>:void =
    Task:task([]int) = spawn{Worker()}
    set Result = Task.Await().Length + 40
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_task_await_inside_interpolated_string_runtime() {
    let source = r#"
var Result:string = ""
Worker()<suspends>:int =
    Sleep(0.001)
    42
Run()<suspends><transacts>:void =
    Task:task(int) = spawn{Worker()}
    set Result = "value {Task.Await()}"
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::String("value 42".to_string()));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::String
    );
}

#[test]
fn evaluates_official_task_await_as_case_subject_runtime() {
    let source = r#"
var Result:int = 0
Worker()<suspends>:int =
    Sleep(0.001)
    2
Run()<suspends><transacts>:void =
    Task:task(int) = spawn{Worker()}
    set Result = case (Task.Await()):
        1 => 0
        2 => 40
        _ => 0
    set Result += 2
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_task_await_inside_case_arm_runtime() {
    let source = r#"
var Result:int = 0
Worker()<suspends>:int =
    Sleep(0.001)
    40
Run()<suspends><transacts>:void =
    Task:task(int) = spawn{Worker()}
    set Result = case (1):
        1 => Task.Await() + 2
        _ => 0
spawn{Run()}
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_suspended_for_body_continues_iterations_runtime() {
    let source = r#"
var Result:int = 0
Delayed(Outer:int, Inner:int)<suspends>:int =
    Sleep(0.0)
    Outer * 10 + Inner
Run()<suspends><transacts>:void =
    Values:[]int = for (Outer := 1..2, Inner := 1..2):
        Delayed(Outer, Inner)
    if:
        First := Values[0]
        Second := Values[1]
        Third := Values[2]
        Fourth := Values[3]
    then:
        set Result = First + Second + Third + Fourth
    else:
        set Result = 0
spawn{Run()}
Tick1:int = Result
Tick2:int = Result
Tick3:int = Result
Tick4:int = Result
Result
"#;

    assert_eq!(eval(source), Value::Int(66));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_suspended_for_generator_iterable_runtime() {
    let source = r#"
var Result:int = 0
Items()<suspends>:[]int =
    Sleep(0.0)
    array{10, 20, 12}
Run()<suspends><transacts>:void =
    Values:[]int = for (Item : Items()):
        Item
    if:
        First := Values[0]
        Second := Values[1]
        Third := Values[2]
    then:
        set Result = First + Second + Third
    else:
        set Result = 0
spawn{Run()}
Tick:int = Result
Tick2:int = Result
Tick3:int = Result
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_suspended_for_let_clause_runtime() {
    let source = r#"
var Result:int = 0
Base()<suspends><transacts>:int =
    Sleep(0.0)
    40
Run()<suspends><transacts>:void =
    Values:[]int = for (Offset := 1..2, Start := Base()):
        Start + Offset
    if:
        First := Values[0]
        Second := Values[1]
    then:
        set Result = First + Second - 40
    else:
        set Result = 0
spawn{Run()}
Tick:int = Result
Result
"#;

    assert_eq!(eval(source), Value::Int(43));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_suspended_for_failable_binding_runtime() {
    let source = r#"
var Result:int = 0
Maybe(Value:int)<suspends><transacts>:?int =
    Sleep(0.0)
    if (Value = 1). option{40} else. if (Value = 2). option{2} else. option{}
Run()<suspends><transacts>:void =
    Values:[]int = for (I := 1..3, Value := Maybe(I)?):
        Value
    if:
        First := Values[0]
        Second := Values[1]
    then:
        set Result = First + Second
    else:
        set Result = 0
spawn{Run()}
Tick1:int = Result
Tick2:int = Result
Tick3:int = Result
Tick4:int = Result
Tick5:int = Result
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_suspended_for_failable_filter_runtime() {
    let source = r#"
var Result:int = 0
Keep(Value:int)<suspends><transacts>:logic =
    Sleep(0.0)
    if (Value = 2). false else. true
Run()<suspends><transacts>:void =
    Values:[]int = for (I := 1..3, Keep(I)?):
        I * 10
    if:
        First := Values[0]
        Second := Values[1]
    then:
        set Result = First + Second + Values.Length
    else:
        set Result = 0
spawn{Run()}
Tick1:int = Result
Tick2:int = Result
Tick3:int = Result
Tick4:int = Result
Tick5:int = Result
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_suspended_if_failure_binding_runtime() {
    let source = r#"
var Result:int = 0
Maybe(Value:int)<suspends><transacts>:?int =
    Sleep(0.0)
    if (Value = 1). option{42} else. option{}
Run()<suspends><transacts>:void =
    if (Value := Maybe(1)?):
        set Result = Value
    else:
        set Result = 0
spawn{Run()}
Tick1:int = Result
Tick2:int = Result
Tick3:int = Result
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_suspended_option_failure_context_runtime() {
    let source = r#"
var Result:int = 0
Maybe(Value:int)<suspends><transacts>:?int =
    Sleep(0.0)
    if (Value = 1). option{40} else. option{}
Run()<suspends><transacts>:void =
    Captured:?int = option{Maybe(1)?}
    Missing:?int = option{Maybe(2)?}
    if:
        Value := Captured?
        not Missing?
    then:
        set Result = Value + 2
    else:
        set Result = 0
spawn{Run()}
Tick1:int = Result
Tick2:int = Result
Tick3:int = Result
Tick4:int = Result
Tick5:int = Result
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_suspended_failure_decision_expression_runtime() {
    let source = r#"
var Result:int = 0
Ready(Value:int)<suspends><transacts>:logic =
    Sleep(0.0)
    if (Value <> 0). true else. false
Run()<suspends><transacts>:void =
    First:int = if (Ready(0)? or Ready(1)?). 20 else. 0
    Second:int = if (not Ready(0)?). 2 else. 0
    Third:int = if (Ready(1)? and Ready(1)?). 20 else. 0
    set Result = First + Second + Third
spawn{Run()}
Tick1:int = Result
Tick2:int = Result
Tick3:int = Result
Tick4:int = Result
Tick5:int = Result
Tick6:int = Result
Tick7:int = Result
Tick8:int = Result
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_suspended_decides_bracket_call_runtime() {
    let source = r#"
var Result:int = 0
Maybe(Value:int)<suspends><transacts>:?int =
    Sleep(0.0)
    if (Value = 1). option{42} else. option{}
Pick(Value:int)<decides><transacts><suspends>:int = Maybe(Value)?
Run()<suspends><transacts>:void =
    if (Value := Pick[1]):
        Missing:int = if (Pick[2]). 100 else. 0
        set Result = Value + Missing
    else:
        set Result = 0
spawn{Run()}
Tick1:int = Result
Tick2:int = Result
Tick3:int = Result
Tick4:int = Result
Tick5:int = Result
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_suspended_decides_method_bracket_call_runtime() {
    let source = r#"
var Result:int = 0
chooser := class:
    Maybe(Value:int)<suspends><transacts>:?int =
        Sleep(0.0)
        if (Value = 1). option{40} else. option{}
    Pick(Value:int)<decides><transacts><suspends>:int = Maybe(Value)?
Run()<suspends><transacts>:void =
    Choice := chooser{}
    if (Value := Choice.Pick[1]):
        Missing:int = if (Choice.Pick[2]). 100 else. 2
        set Result = Value + Missing
    else:
        set Result = 0
spawn{Run()}
Tick1:int = Result
Tick2:int = Result
Tick3:int = Result
Tick4:int = Result
Tick5:int = Result
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn evaluates_official_suspended_set_expression_failure_context_runtime() {
    let source = r#"
var Result:int = 0
Maybe(Value:int)<suspends><transacts>:?int =
    Sleep(0.0)
    if (Value = 1). option{40} else. option{}
Run()<suspends><transacts>:void =
    var Total:int = 0
    if (set Total = Maybe(1)?):
        if (set Total += option{2}?):
            set Result = Total
        else:
            set Result = 0
    else:
        set Result = 0
spawn{Run()}
Tick1:int = Result
Tick2:int = Result
Tick3:int = Result
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rolls_back_official_suspended_set_expression_failure_context_runtime() {
    let source = r#"
var Result:int = 0
Maybe(Value:int)<suspends><transacts>:?int =
    Sleep(0.0)
    if (Value = 1). option{40} else. option{}
Run()<suspends><transacts>:void =
    var Total:int = 0
    if (set Total = Maybe(1)?, Maybe(2)?):
        set Result = 0
    else:
        set Result = Total + 42
spawn{Run()}
Tick1:int = Result
Tick2:int = Result
Tick3:int = Result
Tick4:int = Result
Result
"#;

    assert_eq!(eval(source), Value::Int(42));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}

#[test]
fn rejects_official_event_parametric_type_wrong_arity() {
    let error =
        check_source("Value:event(int, string) = external {}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parametric type `event` expected 0 or 1 type arguments")
    );
}

#[test]
fn rejects_official_task_parametric_type_wrong_arity() {
    let error = check_source("Value:task() = external {}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parametric type `task` expected 1 type arguments")
    );
}

#[test]
fn rejects_official_generator_parametric_type_wrong_arity() {
    let error =
        check_source("Value:generator(int, string) = external {}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parametric type `generator` expected 0 or 1 type arguments")
    );
}

#[test]
fn rejects_official_modifier_parametric_type_wrong_arity() {
    let error = check_source("Value:modifier() = external {}").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parametric type `modifier` expected 1 type arguments")
    );
}

#[test]
fn rejects_official_modifier_stack_parametric_type_wrong_arity() {
    let error = check_source("Value:modifier_stack(int, int) = external {}")
        .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("parametric type `modifier_stack` expected 1 type arguments")
    );
}

#[test]
fn rejects_official_modifier_stack_add_modifier_type_mismatch() {
    let error = check_source(
        r#"
Stack:modifier_stack(int) = external {}
Modifier:modifier(string) = external {}
Stack.AddModifier(Modifier, 0)
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("expected `modifier(int)`"));
}

#[test]
fn rejects_official_modifier_evaluate_type_mismatch() {
    let error = check_source(
        r#"
Modifier:modifier(int) = external {}
Modifier.Evaluate("bad")
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("expected `int`"));
}

#[test]
fn rejects_official_modifier_stack_unknown_member() {
    let error = check_source(
        r#"
Stack:modifier_stack(int) = external {}
Stack.RemoveModifier()
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("has no member `RemoveModifier`"));
}

#[test]
fn rejects_official_generator_pair_iteration() {
    let error = check_source(
        r#"
Values:generator(int) = external {}
for (Index -> Value : Values):
    Value
"#,
    )
    .expect_err("source should fail");

    assert!(error.to_string().contains("pair iteration"));
}

#[test]
fn evaluates_external_official_generator_iteration_as_empty_runtime_generator() {
    let source = r#"
Values:generator(int) = external {}
Collected:[]int = for (Value : Values):
    Value
Collected.Length
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
    assert_eq!(eval(source), Value::Int(0));
}

#[test]
fn evaluates_external_parameterless_generator_iteration_as_empty_runtime_generator() {
    let source = r#"
Values:generator() = external {}
Collected:[]any = for (Value : Values):
    Value
Collected.Length
"#;

    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
    assert_eq!(eval(source), Value::Int(0));
}

#[test]
fn rejects_type_alias_conflicting_with_official_task_parametric_type() {
    let error = check_source("task := int").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("type alias `task` conflicts with builtin type name")
    );
}

#[test]
fn rejects_type_alias_conflicting_with_official_generator_parametric_type() {
    let error = check_source("generator := int").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("type alias `generator` conflicts with builtin type name")
    );
}

#[test]
fn rejects_type_alias_conflicting_with_official_modifier_parametric_type() {
    let error = check_source("modifier := int").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("type alias `modifier` conflicts with builtin type name")
    );
}

#[test]
fn rejects_type_alias_conflicting_with_official_modifier_stack_parametric_type() {
    let error = check_source("modifier_stack := int").expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("type alias `modifier_stack` conflicts with builtin type name")
    );
}

#[test]
fn rejects_data_member_default_spawn_expression() {
    let error = check_source(
        r#"
Wait()<suspends>:int = 42

bad := class:
    Task:task(int) = spawn{Wait()}
"#,
    )
    .expect_err("source should fail");

    assert!(
        error
            .to_string()
            .contains("data-member default value cannot contain `spawn` expressions")
    );
}

#[test]
fn checks_spawn_inside_defer() {
    let source = r#"
Wait()<suspends>:void = {}
Run()<suspends>:void =
    defer:
        spawn{Wait()}
"#;

    assert!(check_source(source).is_ok());
}

#[test]
fn evaluates_for_multiple_generator_clauses() {
    let source = r#"
Pairs:[]int = for (X := 1..2, Y := 1..3):
    X * 10 + Y
if (Value := Pairs[5]). Pairs.Length + Value else. 0
"#;

    assert_eq!(eval(source), Value::Int(29));
    assert_eq!(
        check_source(source).expect("source should check"),
        Type::Int
    );
}
