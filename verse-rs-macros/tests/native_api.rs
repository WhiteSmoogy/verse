use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use verse_rs::{
    NativeError, NativeResult, Type, Value, check_project_file_with_native_apis, native::NativeInt,
    run_project_file_with_native_apis, run_source_with_native_apis,
};
use verse_rs_macros::native_api;

native_api! {
    pub mod host_native;
    trait HostNative;
    path "/Game.com/Host";
    source r#"
using { /Verse.org/Native }

Point<native><public> := struct<computes>:
    X<public>:int
    Y<public>:int

LabelKind<native><public> := enum:
    Plain
    Loud

Add<native><public>(Left:int, Right:int)<transacts>:int
RequirePositive<native><public>(Value:int)<decides><transacts>:int
Label<native><public>(Value:string)<transacts>:string
Sum<native><public>(Values:[]int)<transacts>:int
Lookup<native><public>(Values:[string]int, Key:string)<decides><transacts>:int
MovePoint<native><public>(Value:Point, Delta:int)<transacts>:Point
Describe<native><public>(Kind:LabelKind, Value:string)<transacts>:string
"#;
}

native_api! {
    pub mod text_native;
    trait TextNative;
    path "/Game.com/Text";
    source r#"
using { /Verse.org/Native }

Wrap<native><public>(Value:int)<transacts>:string
"#;
}

struct Host;

impl host_native::HostNative for Host {
    fn add(
        &self,
        _ctx: verse_rs::native::NativeCallContext,
        left: NativeInt,
        right: NativeInt,
    ) -> NativeResult<NativeInt> {
        Ok(left + right)
    }

    fn require_positive(
        &self,
        _ctx: verse_rs::native::NativeCallContext,
        value: NativeInt,
    ) -> NativeResult<NativeInt> {
        if value > 0 {
            Ok(value)
        } else {
            Err(NativeError::failure("value must be positive"))
        }
    }

    fn label(
        &self,
        _ctx: verse_rs::native::NativeCallContext,
        value: String,
    ) -> NativeResult<String> {
        Ok(format!("host:{value}"))
    }

    fn sum(
        &self,
        _ctx: verse_rs::native::NativeCallContext,
        values: Vec<NativeInt>,
    ) -> NativeResult<NativeInt> {
        Ok(values.into_iter().sum())
    }

    fn lookup(
        &self,
        _ctx: verse_rs::native::NativeCallContext,
        values: HashMap<String, NativeInt>,
        key: String,
    ) -> NativeResult<NativeInt> {
        values
            .get(&key)
            .copied()
            .ok_or_else(|| NativeError::failure("missing key"))
    }

    fn move_point(
        &self,
        _ctx: verse_rs::native::NativeCallContext,
        value: host_native::Point,
        delta: NativeInt,
    ) -> NativeResult<host_native::Point> {
        Ok(host_native::Point {
            x: value.x + delta,
            y: value.y + delta,
        })
    }

    fn describe(
        &self,
        _ctx: verse_rs::native::NativeCallContext,
        kind: host_native::LabelKind,
        value: String,
    ) -> NativeResult<String> {
        let prefix = match kind {
            host_native::LabelKind::Plain => "plain",
            host_native::LabelKind::Loud => "loud",
        };
        Ok(format!("{prefix}:{value}"))
    }
}

struct TextHost;

impl text_native::TextNative for TextHost {
    fn wrap(
        &self,
        _ctx: verse_rs::native::NativeCallContext,
        value: NativeInt,
    ) -> NativeResult<String> {
        Ok(format!("value={value}"))
    }
}

#[test]
fn injects_native_api_from_generated_trait() {
    let source = r#"
using { /Game.com/Host }

Add(20, 22)
"#;

    let value = run_source_with_native_apis(source, [host_native::bind(Host)])
        .expect("source should run with host native API");
    assert_eq!(value, Value::Int(42));
}

#[test]
fn maps_injected_native_failure_to_decides_failure() {
    let source = r#"
using { /Game.com/Host }

Good := if (Value := RequirePositive[40]). Value else. 0
Bad := if (Value := RequirePositive[0]). Value else. 2
Good + Bad
"#;

    let value = run_source_with_native_apis(source, [host_native::bind(Host)])
        .expect("source should run with host native API");
    assert_eq!(value, Value::Int(42));
}

#[test]
fn converts_native_strings() {
    let source = r#"
using { /Game.com/Host }

Label("ok")
"#;

    let value = run_source_with_native_apis(source, [host_native::bind(Host)])
        .expect("source should run with host native API");
    assert_eq!(value, Value::String("host:ok".to_string()));
}

#[test]
fn converts_containers_and_native_types() {
    let source = r#"
using { /Game.com/Host }

P := MovePoint(Point{X := 18, Y := 19}, 2)
Found := if (Value := Lookup[map{"answer" => 42}, "answer"]). Value else. 0
Describe(LabelKind.Loud, "{Sum(array{10, 20, 12})}:{Found}:{P.X}:{P.Y}")
"#;

    let value = run_source_with_native_apis(source, [host_native::bind(Host)])
        .expect("source should run with host native API");
    assert_eq!(value, Value::String("loud:42:42:20:21".to_string()));
}

#[test]
fn injects_multiple_native_apis_from_separate_bindings() {
    let source = r#"
using { /Game.com/Host }
using { /Game.com/Text }

Wrap(Add(20, 22))
"#;

    let value = run_source_with_native_apis(
        source,
        [host_native::bind(Host), text_native::bind(TextHost)],
    )
    .expect("source should run with both host native APIs");
    assert_eq!(value, Value::String("value=42".to_string()));
}

#[test]
fn runs_project_file_with_native_apis() {
    let root = temp_project_dir("native_project_file");
    write_project_file(
        &root,
        "Helpers.verse",
        r#"
using { /Game.com/Host }

DoubleWithHost(Value:int)<transacts>:int = Add(Value, Value)
"#,
    );
    write_project_file(
        &root,
        "main.verse",
        r#"
DoubleWithHost(21)
"#,
    );

    let entry = root.join("main.verse");
    assert_eq!(
        check_project_file_with_native_apis(&entry, [host_native::bind(Host)])
            .expect("project should check with host native API"),
        Type::Int
    );
    assert_eq!(
        run_project_file_with_native_apis(&entry, [host_native::bind(Host)])
            .expect("project should run with host native API"),
        Value::Int(42)
    );
}

fn temp_project_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("verse_rs_macros_{name}_{nonce}"));
    fs::create_dir_all(&dir).expect("temp project directory should be created");
    dir
}

fn write_project_file(root: &Path, relative: &str, source: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("project subdirectory should be created");
    }
    fs::write(path, source).expect("project file should be written");
}
