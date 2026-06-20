Origin:tuple(int, int) = (0, 0)
East:tuple(int, int) = (1, 0)

Grid:[tuple(int, int)]string = map{
    Origin => "origin",
    East => "east",
}

print(Grid[Origin])
print(Grid[East])
print("east-x=" + str(East(0)))

Add(A:int, B:int):int = A + B
print("sum=" + str(Add((40, 2))))

AddPair(Pair:tuple(int, int)):int = Pair(0) + Pair(1)
print("pair-sum=" + str(AddPair(3, 5)))

Sum(Values:[]int)<transacts>:int = {
    var Total:int = 0
    for (Value : Values) {
        set Total += Value
    }
    Total
}

print("values=" + str(Sum(1, 2, 3)))
print("tuple-values=" + str(Sum((4, 5))))
