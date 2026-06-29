use verse_rs::{NativeError, NativeResult, Value, native_api, run_source_with_native_apis};

native_api! {
    pub mod host_native;
    trait HostNative;
    path "/Game.com/Host";
    source r#"
using { /Verse.org/Native }

Add<native><public>(Left:int, Right:int)<transacts>:int
RequirePositive<native><public>(Value:int)<decides><transacts>:int
Label<native><public>(Value:string)<transacts>:string
"#;
}

struct Host;

impl host_native::HostNative for Host {
    fn add(
        &self,
        _ctx: verse_rs::native::NativeCallContext,
        left: i128,
        right: i128,
    ) -> NativeResult<i128> {
        Ok(left + right)
    }

    fn require_positive(
        &self,
        _ctx: verse_rs::native::NativeCallContext,
        value: i128,
    ) -> NativeResult<i128> {
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
