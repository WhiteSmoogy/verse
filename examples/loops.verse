var total:int = 0

var i:int = 1
loop {
    if (i > 10) {
        break
    }
    if (i = 8). break
    if (i <> 3) {
        set total += i
    }
    set i += 1
}

var xs:[]int = array{total, total + 1, total + 2}
Update0 := if (set xs[0] *= 2). 0 else. 0
Update1 := if (set xs[0] = Quotient[xs[0], 2]). 0 else. 0
Update2 := if (set xs[0] -= 0). 0 else. 0
Update3 := if (set xs[1] = 99). 0 else. 0
labels := for (Letter : "ab") {
    Letter
}
doubled := for (Value : array{1, 2, 3}):
    Value * 2
dotted := for (Value := 1..3). Value * 3
filtered := for (Value := 1..5, Value <> 3, Doubled := Value * 2):
    Doubled
blocked := for:
    Value := 1..3
    Doubled := Value * 2
do:
    Doubled

print(xs)
print("len=" + str(xs.Length))
print("labels=" + (if (A := labels[0]). ToString(A) else. "") + (if (B := labels[1]). ToString(B) else. "") + str(labels.Length))
print("doubled=" + str(if (A := doubled[0], B := doubled[1], C := doubled[2]). A + B + C else. 0))
print("dotted=" + str(if (A := dotted[0], B := dotted[1], C := dotted[2]). A + B + C else. 0))
print("filtered=" + str(filtered.Length + if (Value := filtered[2]). Value else. 0))
print("blocked=" + str(if (A := blocked[0], B := blocked[1], C := blocked[2]). A + B + C else. 0))
