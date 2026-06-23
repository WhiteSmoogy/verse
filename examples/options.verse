FindScore(Name:string)<computes>:?int = if (Name = "alice") {
    option{42}
} else {
    option{}
}

profile := struct:
    Name : string

Alice:?int = FindScore("alice")
var Bob:?int = false
MaybeProfile:?profile = option{profile{Name := "Ava"}}
NoProfile:?profile = false
Values:[]int = array{42}
Captured:?int = option{Values[1]}
Ready:?logic = option{true?}
Blocked:?logic = option{false?}

set Bob = option{7}

print("alice=" + str(if (Score := Alice?). Score else. 0))
print("bob=" + str(if (Score := Bob?). Score else. 0))
print("missing=" + str(if (Missing := FindScore("bob")?). Missing else. 0))
print("profile=" + if (Name := MaybeProfile?.Name). Name else. "none")
print("none=" + if (Name := NoProfile?.Name). Name else. "none")
print("captured=" + str(if (Value := Captured?). Value else. 0))
print("ready=" + str(if (Value := Ready?). Value else. false))
print("blocked=" + str(if (Value := Blocked?). Value else. false))
Selected:int = if:
    Score := Alice?
    Score = 42
then:
    Score
else:
    0
print("selected=" + str(Selected))
