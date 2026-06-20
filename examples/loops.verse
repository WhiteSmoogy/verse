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
set xs[0] *= 2
set xs[0] = Quotient[xs[0], 2]
set xs[0] -= 0
set xs[1] = 99
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
print("labels=" + ToString(labels[0]) + ToString(labels[1]) + str(labels.Length))
print("doubled=" + str(doubled[0] + doubled[1] + doubled[2]))
print("dotted=" + str(dotted[0] + dotted[1] + dotted[2]))
print("filtered=" + str(filtered.Length + filtered[2]))
print("blocked=" + str(blocked[0] + blocked[1] + blocked[2]))
