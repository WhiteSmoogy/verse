using { /Verse.org/Verse }

<# Function examples use official Verse comment syntax. #>
Label(Message:string, ?Level:int = 1, ?Color:string = "white"):string = {
    Message + ":" + str(Level) + ":" + Color
}

Scale(?Value:int, ?Factor:int = 2):int = Value * Factor

Offset(Value:int, ?Amount:int = Value + 1):int = Value + Amount

Double(X:int):int = X * 2
PublicDouble<public>(X:int):int = X * 2
PureDouble(X:int)<computes>:int = X * 2
Pick(Value:int)<decides><transacts>:int = Value
Values:[]int = array{42}
PickIndex(Index:int)<decides><transacts>:int = Values[Index]
OnBegin<override>()<suspends>:void = none
FortyTwo():int = 42
Apply(F:type{_(:int):int}, Value:int):int = F(Value)
var Handler:?type{_():int} = false

MakeAdder(X:int) = {
    Add(Y:int):int = X + Y
    Add
}

print(Label("Start"))
print(Label("Warn", ?Level := 2))
print(Label("Err", ?Color := "red", ?Level := 3))
print("scale=" + str(Scale(?Value := 5) + Scale(?Factor := 3, ?Value := 5)))
print("offset=" + str(Offset(10) + Offset(10, ?Amount := 5)))
print("apply=" + str(Apply(Double, 21)))
print("public=" + str(PublicDouble(21)))
print("effect=" + str(PureDouble(21)))
print("pick=" + str(if (Value := Pick[42]). Value else. 0))
print("pick-fail=" + str(if (Value := PickIndex[1]). Value else. 0))
set Handler = option{FortyTwo}
print("handler=" + str(if (Function := Handler?). Function() else. 0))
print("adder=" + str(MakeAdder(40)(2)))
BlockValue:int = block:
    Base:int = 40
    Base + 2
print("block=" + str(BlockValue))
Remainder:int = if (Value := Mod[-1, 3]). Value else. 0
Quot:int = if (Value := Quotient[10, -3]). Value else. 0
print("math=" + str(Remainder) + ":" + str(Quot) + ":" + str(Clamp(12, 10, 0)) + ":" + str(Lerp(10, 20, 0.25)))
print("rounding=" + str(if (CeilValue := Ceil[1.2], FloorValue := Floor[1.8], RoundValue := Round[2.5], IntValue := Int[-3.7]). Abs(-5) + Min(10, 3) + Max(7, 9) + CeilValue + FloorValue + RoundValue + IntValue else. 0))
print("float-math=" + str(if (SqrtValue := Sqrt(81), PowValue := Pow(2, 5), LogValue := Log(2, 8), RoundedSqrt := Round[SqrtValue], RoundedPow := Round[PowValue], RoundedLog := Round[LogValue]). RoundedSqrt + RoundedPow + RoundedLog + Sgn(-3) else. 0))
print("finite=" + str(if (Value := Inf.IsFinite[]). Value else. 42))
print("almost=" + str(if (IsAlmostEqual[1.0, 1.01, 0.02]). 42 else. 0))
