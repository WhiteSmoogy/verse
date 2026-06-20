ClampValue(Value:int):int = {
    if (Value < 0) {
        return 0
    }
    if (Value > 100) {
        return 100
    }
    Value
}

print(ClampValue(-5))
print(ClampValue(42))
print(ClampValue(120))

var Cleanup:string = ""
WithCleanup()<transacts>:int = {
    defer:
        set Cleanup = Cleanup + "D"
    return 42
}

print(str(WithCleanup()) + ":" + Cleanup)
