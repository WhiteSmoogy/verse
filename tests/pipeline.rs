use verse_rs::{
    Constant, Desugarer, IRGenerator, Instruction, Opcode, SemanticAnalyzer, Type, Value, VerseVm,
    compile_source, parse_source,
};

#[test]
fn explicit_compiler_pipeline_runs_through_vm() {
    let source = r#"
Add(X:int, Y:int):int =
    X + Y

Add(20, 22)
"#;

    let parsed = parse_source(source).expect("source should parse");
    let desugared = Desugarer::new().desugar_program(&parsed);
    let semantic = SemanticAnalyzer::new()
        .analyze_desugared_program(desugared)
        .expect("source should analyze");
    assert_eq!(semantic.value_type, Type::Int);

    let ir = IRGenerator::new()
        .generate(semantic)
        .expect("IR generation should produce bytecode");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    assert_eq!(ir.bytecode_program().functions().len(), 1);
    assert!(
        ir.bytecode_program()
            .entry_chunk()
            .instructions()
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::Call)
    );
    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_runs_function_return_and_if_without_legacy_eval() {
    let source = r#"
Pick(Value:int):int =
    if (Value > 10):
        return Value
    else:
        return 10

Pick(42)
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    assert!(
        ir.bytecode_program()
            .chunks()
            .iter()
            .flat_map(|chunk| chunk.instructions())
            .any(|instruction| instruction.opcode() == Opcode::GtFastFail)
    );

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_runs_mutable_assignment_without_legacy_eval() {
    let source = r#"
var Total:int = 0
set Total += 40
set Total += 2
Total
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    assert!(
        ir.bytecode_program()
            .entry_chunk()
            .instructions()
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::Add)
    );
    let opcodes = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::RefGet));
    assert!(opcodes.contains(&Opcode::RefSet));
    assert!(
        ir.bytecode_program()
            .entry_chunk()
            .constants()
            .iter()
            .any(|constant| matches!(constant, Constant::GlobalRef(name) if name == "Total"))
    );

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_runs_var_expression_without_legacy_eval() {
    let source = r#"
Seed := var Total:int = 40
set Total += 2
Seed + (Total - 40)
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let opcodes = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::NewRef));
    assert!(opcodes.contains(&Opcode::RefSet));
    assert!(opcodes.contains(&Opcode::RefGet));

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_runs_failable_var_expression_without_legacy_eval() {
    let source = r#"
Hit := if (var Pick:int = array{40}[0]). Pick else. 0
Miss := if (var Other:int = array{40}[9]). Other else. 2
Hit + Miss
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let opcodes = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::ArrayIndexFastFail));
    assert!(opcodes.contains(&Opcode::NewRef));
    assert!(opcodes.contains(&Opcode::RefSet));

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_runs_call_set_without_legacy_eval() {
    let source = r#"
var Values:[]int = array{1, 2}
var Scores:[string]int = map{}
Result := if:
    set Values[0] = 40
    set Scores["ada"] = 2
    Value := Values[0]
    Score := Scores["ada"]
then:
    Value + Score
else:
    0
Result
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    assert!(
        ir.bytecode_program()
            .entry_chunk()
            .instructions()
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::CallSet)
    );

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_call_set_fails_in_failure_context_without_legacy_eval() {
    let source = r#"
var Values:[]int = array{1}
Hit := if:
    set Values[0] = 40
    Value := Values[0]
then:
    Value
else:
    0
Miss := if:
    set Values[9] = 99
then:
    0
else:
    2
Hit + Miss
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    assert!(
        ir.bytecode_program()
            .entry_chunk()
            .instructions()
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::CallSet)
    );

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_rolls_back_ref_set_when_failure_context_fails_without_legacy_eval() {
    let source = r#"
var BreakTime:logic = false
if:
    1 > 0
    set BreakTime = true
    0 > 1
then:
    0
else:
    if (BreakTime?):
        1
    else:
        42
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let opcodes = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::BeginFailureContext));
    assert!(opcodes.contains(&Opcode::RefSet));

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_rolls_back_call_set_when_failure_context_fails_without_legacy_eval() {
    let source = r#"
var Values:[]int = array{1}
if:
    set Values[0] = 99
    set Values[9] = 0
then:
    0
else:
    if:
        Value := Values[0]
    then:
        Value + 41
    else:
        0
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let opcodes = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::BeginFailureContext));
    assert!(opcodes.contains(&Opcode::CallSet));

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_runs_ordinary_named_argument_without_legacy_eval() {
    let source = r#"
BuyMousetrap(CoinsPerMousetrap:int):int = CoinsPerMousetrap + 32
BuyMousetrap(CoinsPerMousetrap := 10)
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    assert!(
        ir.bytecode_program()
            .entry_chunk()
            .instructions()
            .iter()
            .any(|instruction| matches!(
                instruction,
                Instruction::Call {
                    arguments,
                    named_arguments,
                    ..
                } if arguments.len() == 1 && named_arguments.is_empty()
            ))
    );

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_reorders_ordinary_named_arguments_without_legacy_eval() {
    let source = r#"
Difference(Left:int, Right:int):int = Left - Right
Difference(Right := 8, Left := 50)
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    assert!(
        ir.bytecode_program()
            .entry_chunk()
            .instructions()
            .iter()
            .any(|instruction| matches!(
                instruction,
                Instruction::Call {
                    arguments,
                    named_arguments,
                    ..
                } if arguments.len() == 2 && named_arguments.is_empty()
            ))
    );

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_runs_collection_literals_without_legacy_eval() {
    let source = r#"
Values := array{10, 20, 12}
Pair := (1, 2)
Scores := map{"ada" => 40, "grace" => 2}
42
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );

    let instructions = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .collect::<Vec<_>>();
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::NewArray)
    );
    assert!(
        ir.bytecode_program()
            .entry_chunk()
            .constants()
            .iter()
            .any(|constant| matches!(constant, Constant::Tuple(_)))
    );
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::NewMap)
    );
    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_runs_additive_collection_and_text_values_without_legacy_eval() {
    let text_source = r#"
Text := "For" + array{'t', 'y'}
if (Text = "Forty"):
    42
else:
    0
"#;

    let text_ir = compile_source(text_source).expect("source should compile");
    assert!(
        !text_ir
            .bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    assert!(
        text_ir
            .bytecode_program()
            .entry_chunk()
            .instructions()
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::Add)
    );
    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&text_ir).expect("IR program should run"),
        Value::Int(42)
    );

    let array_source = r#"
array{10, 20} + array{30} + (40, 42)
"#;

    let array_ir = compile_source(array_source).expect("source should compile");
    assert!(
        !array_ir
            .bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let mut vm = VerseVm::new();
    let value = vm.run_ir_program(&array_ir).expect("IR program should run");
    let Value::Array(items) = value else {
        panic!("expected array result, got {value}");
    };
    assert_eq!(
        *items.borrow(),
        vec![
            Value::Int(10),
            Value::Int(20),
            Value::Int(30),
            Value::Int(40),
            Value::Int(42)
        ]
    );
}

#[test]
fn bytecode_vm_runs_length_members_without_legacy_eval() {
    let source = r#"
Values:[]int = array{10, 20, 30}
Scores:[string]int = map{"alice" => 10, "bob" => 20}
Text := "abc"
Values.Length() * 100 + Scores.Length() * 10 + Text.Length() + Values.Length
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );

    let instructions = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .collect::<Vec<_>>();
    assert!(
        instructions
            .iter()
            .filter(|instruction| instruction.opcode() == Opcode::Length)
            .count()
            >= 4
    );

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(326)
    );
}

#[test]
fn bytecode_vm_runs_failure_bind_array_index_without_legacy_eval() {
    let source = r#"
Values := array{42, 99}
Hit := if (Value := Values[0]):
    Value
else:
    0
Miss := if (Value := Values[9]):
    Value
else:
    42
Hit + Miss - 42
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );

    let instructions = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .collect::<Vec<_>>();
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::ArrayIndexFastFail)
    );
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::BeginFailureContext)
    );

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_runs_option_query_without_legacy_eval() {
    let source = r#"
Filled:?int = option{42}
Empty:?int = option{}
First := if (Value := Filled?):
    Value
else:
    0
Second := if (Value := Empty?):
    Value
else:
    0
First + Second
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );

    let instructions = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .collect::<Vec<_>>();
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::NewOption)
    );
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::QueryFastFail)
    );

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_wraps_failable_option_expression_without_legacy_eval() {
    let source = r#"
Values := array{42}
Found:?int = option{Values[0]}
Missing:?int = option{Values[9]}
Hit := if (Value := Found?):
    Value
else:
    0
Miss := if (Value := Missing?):
    Value
else:
    42
Hit + Miss - 42
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );

    let instructions = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .collect::<Vec<_>>();
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::NewOption)
    );
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::ArrayIndexFastFail)
    );

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_runs_decision_expressions_without_legacy_eval() {
    let source = r#"
Both := if (5 > 0 and 30 >= 20):
    20
else:
    0
Either := if (0 > 0 or 2 = 2):
    20
else:
    0
Negated := if (not (0 > 0)):
    2
else:
    0
Both + Either + Negated
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );

    let instructions = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .collect::<Vec<_>>();
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::GtFastFail)
    );
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::EqFastFail)
    );
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::BeginFailureContext)
    );

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_runs_wildcard_case_without_legacy_eval() {
    let source = r#"
Value:int = case (2):
    1 => 10
    2 => 40
    _ => 0
Value + 2
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let opcodes = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::EqFastFail));
    assert!(opcodes.contains(&Opcode::BeginFailureContext));

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_runs_scalar_case_patterns_without_legacy_eval() {
    let source = r#"
LogicValue:int = case (false):
    true => 0
    false => 10
    _ => 0
StringValue:int = case ("harvest"):
    "battle" => 0
    "harvest" => 20
    _ => 0
CharValue:int = case ('B'):
    'A' => 0
    'B' => 12
    _ => 0
LogicValue + StringValue + CharValue
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let eq_fast_fail_count = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .filter(|instruction| instruction.opcode() == Opcode::EqFastFail)
        .count();
    assert!(eq_fast_fail_count >= 6);

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_captures_partial_case_failure_without_legacy_eval() {
    let source = r#"
Matched:?int = option{
    case (2):
        2 => 40
}
Missing:?int = option{
    case (3):
        2 => 0
}
if (Value := Matched?, not Missing?). Value + 2 else. 0
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let opcodes = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::EqFastFail));
    assert!(opcodes.contains(&Opcode::NewOption));

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_propagates_partial_case_failure_from_decides_function_without_legacy_eval() {
    let source = r#"
Pick(Value:int)<decides><transacts>:int =
    case (Value):
        7 => 42

Hit := if (Result := Pick[7]). Result else. 0
Miss := if (Result := Pick[8]). Result else. 1
Hit + Miss
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    assert!(
        ir.bytecode_program()
            .chunks()
            .iter()
            .flat_map(|chunk| chunk.instructions())
            .any(|instruction| instruction.opcode() == Opcode::EqFastFail)
    );

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(43)
    );
}

#[test]
fn bytecode_vm_binds_decision_expression_success_values_without_legacy_eval() {
    let source = r#"
AndValue := if (Value := 1 = 1 and 40 = 40):
    Value
else:
    0
OrValue := if (Value := 0 > 1 or 2 = 2):
    Value
else:
    0
AndValue + OrValue
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );

    let instructions = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .collect::<Vec<_>>();
    assert!(
        instructions
            .iter()
            .filter(|instruction| instruction.opcode() == Opcode::EqFastFail)
            .count()
            >= 3
    );
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::Jump)
    );

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_captures_division_and_mod_failure_without_legacy_eval() {
    let source = r#"
Division := if (84 / 0):
    0
else:
    40
Remainder := if (84 % 0):
    0
else:
    2
Captured:?rational = option{84 / 0}
Ignored := if (Captured?):
    99
else:
    0
Division + Remainder + Ignored
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );

    let instructions = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .collect::<Vec<_>>();
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::Div)
    );
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::Mod)
    );
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::NewOption)
    );

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_preserves_rational_division_without_legacy_eval() {
    let ir = compile_source(
        r#"
if (Value := 1 / 2):
    Value
else:
    0
"#,
    )
    .expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    assert!(
        ir.bytecode_program()
            .entry_chunk()
            .instructions()
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::Div)
    );

    let mut vm = VerseVm::new();
    let value = vm.run_ir_program(&ir).expect("IR program should run");
    assert!(matches!(value, Value::Rational(_)));
    assert_eq!(value.to_string(), "1/2");

    let chained = compile_source(
        r#"
if (Half := 1 / 2, Quarter := Half / 2):
    Quarter
else:
    0
"#,
    )
    .expect("source should compile");
    assert!(
        !chained
            .bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let mut vm = VerseVm::new();
    let value = vm.run_ir_program(&chained).expect("IR program should run");
    assert!(matches!(value, Value::Rational(_)));
    assert_eq!(value.to_string(), "1/4");
}

#[test]
fn bytecode_vm_negates_rational_values_without_legacy_eval() {
    let source = r#"
if (Half := 1 / 2):
    -Half
else:
    0
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let opcodes = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::Div));
    assert!(opcodes.contains(&Opcode::Neg));

    let mut vm = VerseVm::new();
    let value = vm.run_ir_program(&ir).expect("IR program should run");
    assert!(matches!(value, Value::Rational(_)));
    assert_eq!(value.to_string(), "-1/2");
}

#[test]
fn bytecode_vm_runs_failure_sequence_condition_without_legacy_eval() {
    let source = r#"
Values := array{42}
Hit := if (Value := Values[0], Value = 42):
    Value
else:
    0
Miss := if (Value := Values[9], Value = 42):
    0
else:
    42
Hit + Miss - 42
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );

    let instructions = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .collect::<Vec<_>>();
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::ArrayIndexFastFail)
    );
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::EqFastFail)
    );

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_runs_if_condition_block_without_legacy_eval() {
    let source = r#"
Values := array{42}
Hit := if:
    Value := Values[0]
    Value = 42
then:
    Value
else:
    0
Miss := if:
    Value := Values[9]
    Value = 42
then:
    0
else:
    42
Hit + Miss - 42
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );

    let instructions = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .collect::<Vec<_>>();
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::ArrayIndexFastFail)
    );
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::EqFastFail)
    );

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_runs_loop_break_without_legacy_eval() {
    let source = r#"
var I:int = 0
var Total:int = 0
loop:
    if (I = 4):
        break
    set I += 1
    set Total += I
Total
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );

    let instructions = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .collect::<Vec<_>>();
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::EqFastFail)
    );
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::Jump)
    );

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(10)
    );
}

#[test]
fn bytecode_vm_runs_simple_range_for_without_legacy_eval() {
    let source = r#"
for (I := 1..4):
    I * 2
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    assert!(
        ir.bytecode_program()
            .entry_chunk()
            .instructions()
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::ArrayAdd)
    );

    let mut vm = VerseVm::new();
    let value = vm.run_ir_program(&ir).expect("IR program should run");
    let Value::Array(items) = value else {
        panic!("expected array result, got {value}");
    };
    assert_eq!(
        *items.borrow(),
        vec![Value::Int(2), Value::Int(4), Value::Int(6), Value::Int(8)]
    );
}

#[test]
fn bytecode_vm_runs_multiple_range_for_clauses_without_legacy_eval() {
    let source = r#"
Pairs:[]int = for (X := 1..2, Y := 1..3):
    X * 10 + Y
if (Value := Pairs[5]). Pairs.Length + Value else. 0
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let lte_fast_fail_count = ir
        .bytecode_program()
        .chunks()
        .iter()
        .flat_map(|chunk| chunk.instructions())
        .filter(|instruction| instruction.opcode() == Opcode::LteFastFail)
        .count();
    assert!(lte_fast_fail_count >= 2);

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(29)
    );
}

#[test]
fn bytecode_vm_runs_array_value_for_without_legacy_eval() {
    let source = r#"
Values:[]int = array{20, 21}
Mapped := for (Value : Values):
    Value + 1
if:
    First := Mapped[0]
    Second := Mapped[1]
then:
    First + Second
else:
    0
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let opcodes = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::Length));
    assert!(opcodes.contains(&Opcode::Call));
    assert!(opcodes.contains(&Opcode::ArrayAdd));

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(43)
    );
}

#[test]
fn bytecode_vm_runs_array_pair_for_without_legacy_eval() {
    let source = r#"
Values:[]int = array{20, 21}
Mapped := for (Index -> Value : Values):
    Index + Value
if:
    First := Mapped[0]
    Second := Mapped[1]
then:
    First + Second
else:
    0
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let opcodes = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::Length));
    assert!(opcodes.contains(&Opcode::Call));
    assert!(opcodes.contains(&Opcode::ArrayAdd));

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_runs_map_value_for_without_legacy_eval() {
    let source = r#"
Scores:[string]int = map{"alice" => 20, "bob" => 21}
Mapped := for (Score : Scores):
    Score + 1
if:
    First := Mapped[0]
    Second := Mapped[1]
then:
    First + Second
else:
    0
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let opcodes = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::Length));
    assert!(opcodes.contains(&Opcode::MapValue));
    assert!(opcodes.contains(&Opcode::ArrayAdd));

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(43)
    );
}

#[test]
fn bytecode_vm_runs_map_pair_for_without_legacy_eval() {
    let source = r#"
Scores:[int]int = map{1 => 20, 2 => 19}
Mapped := for (Rank -> Score : Scores):
    Rank + Score
if:
    First := Mapped[0]
    Second := Mapped[1]
then:
    First + Second
else:
    0
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let opcodes = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::MapKey));
    assert!(opcodes.contains(&Opcode::MapValue));
    assert!(opcodes.contains(&Opcode::ArrayAdd));

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_uses_semantic_facts_for_array_parameter_for_without_legacy_eval() {
    let source = r#"
Bump(Values:[]int):[]int =
    for (Value : Values):
        Value + 1

Mapped := Bump(array{20, 21})
if:
    First := Mapped[0]
    Second := Mapped[1]
then:
    First + Second
else:
    0
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let opcodes = ir
        .bytecode_program()
        .chunks()
        .iter()
        .flat_map(|chunk| chunk.instructions())
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::Length));
    assert!(opcodes.contains(&Opcode::Call));
    assert!(opcodes.contains(&Opcode::ArrayAdd));

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(43)
    );
}

#[test]
fn bytecode_vm_uses_semantic_facts_for_map_parameter_for_without_legacy_eval() {
    let source = r#"
Bump(Scores:[string]int):[]int =
    for (Score : Scores):
        Score + 1

Mapped := Bump(map{"alice" => 20, "bob" => 21})
if:
    First := Mapped[0]
    Second := Mapped[1]
then:
    First + Second
else:
    0
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let opcodes = ir
        .bytecode_program()
        .chunks()
        .iter()
        .flat_map(|chunk| chunk.instructions())
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::Length));
    assert!(opcodes.contains(&Opcode::MapValue));
    assert!(opcodes.contains(&Opcode::ArrayAdd));

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(43)
    );
}

#[test]
fn bytecode_vm_runs_for_filter_and_intermediate_binding_without_legacy_eval() {
    let source = r#"
Values:[]int = for (X := 1..5, X <> 3, Y := X * 2):
    Y
if (Value := Values[2]). Values.Length + Value else. 0
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let opcodes = ir
        .bytecode_program()
        .chunks()
        .iter()
        .flat_map(|chunk| chunk.instructions())
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::BeginFailureContext));
    assert!(opcodes.contains(&Opcode::NeqFastFail));
    assert!(opcodes.contains(&Opcode::QueryFastFail));
    assert!(opcodes.contains(&Opcode::ArrayAdd));

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(12)
    );
}

#[test]
fn bytecode_vm_propagates_decides_function_failure_without_legacy_eval() {
    let source = r#"
Pick(Values:[]int, Index:int)<decides><transacts>:int =
    Values[Index]

Hit := if (Value := Pick[array{40}, 0]):
    Value
else:
    0
Miss := if (Value := Pick[array{40}, 1]):
    Value
else:
    2
Hit + Miss
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let opcodes = ir
        .bytecode_program()
        .chunks()
        .iter()
        .flat_map(|chunk| chunk.instructions())
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::NewFunction));
    assert!(opcodes.contains(&Opcode::Call));
    assert!(opcodes.contains(&Opcode::ArrayIndexFastFail));

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_ignores_compile_time_type_aliases_without_legacy_eval() {
    let source = r#"
score := int
Values:[]score = array{40, 2}
if (First := Values[0], Second := Values[1]). First + Second else. 0
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    assert!(
        ir.bytecode_program()
            .entry_chunk()
            .instructions()
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::NewArray)
    );

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_ignores_compile_time_parametric_types_without_legacy_eval() {
    let source = r#"
box(t:type) := class:
    Items:[]t = array{}

Values:[]int = array{40, 2}
if (First := Values[0], Second := Values[1]). First + Second else. 0
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let opcodes = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::NewArray));
    assert!(!opcodes.contains(&Opcode::NewClass));

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_runs_profile_block_without_legacy_eval() {
    let source = r#"
Result:int = profile("Finding a number"):
    40 + 2
Result
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let opcodes = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::BeginProfileBlock));
    assert!(opcodes.contains(&Opcode::EndProfileBlock));
    assert!(opcodes.contains(&Opcode::Add));

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_runs_failable_profile_block_without_legacy_eval() {
    let source = r#"
Hit := if (profile("Hit"):
    array{10}[0]
). 40 else. 0
Miss := if (profile("Miss"):
    array{10}[9]
). 0 else. 2
Hit + Miss
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let opcodes = ir
        .bytecode_program()
        .entry_chunk()
        .instructions()
        .iter()
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::BeginProfileBlock));
    assert!(opcodes.contains(&Opcode::EndProfileBlock));
    assert!(opcodes.contains(&Opcode::ArrayIndexFastFail));
    assert!(opcodes.contains(&Opcode::BeginFailureContext));

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_wraps_failable_equality_option_without_legacy_eval() {
    let source = r#"
Hit:?int = option{40 = 40}
Miss:?int = option{0 = 1}
First := if (Value := Hit?). Value else. 0
Second := if (Value := Miss?). 100 else. 2
First + Second
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let opcodes = ir
        .bytecode_program()
        .chunks()
        .iter()
        .flat_map(|chunk| chunk.instructions())
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::EqFastFail));
    assert!(opcodes.contains(&Opcode::NewOption));
    assert!(opcodes.contains(&Opcode::BeginFailureContext));

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn bytecode_vm_runs_class_field_load_without_legacy_eval() {
    let source = r#"
counter := class<concrete>:
    Value:int = 0

counter{Value := 42}.Value
"#;

    let ir = compile_source(source).expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    let opcodes = ir
        .bytecode_program()
        .chunks()
        .iter()
        .flat_map(|chunk| chunk.instructions())
        .map(Instruction::opcode)
        .collect::<Vec<_>>();
    assert!(opcodes.contains(&Opcode::NewObject));
    assert!(opcodes.contains(&Opcode::LoadField));

    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}

#[test]
fn compile_source_produces_executable_ir() {
    let ir = compile_source("40 + 2").expect("source should compile");
    assert!(
        !ir.bytecode_program()
            .uses_legacy_compatibility_instruction()
    );
    assert!(
        ir.bytecode_program()
            .entry_chunk()
            .instructions()
            .iter()
            .any(|instruction| instruction.opcode() == Opcode::Add)
    );
    let mut vm = VerseVm::new();
    assert_eq!(
        vm.run_ir_program(&ir).expect("IR program should run"),
        Value::Int(42)
    );
}
