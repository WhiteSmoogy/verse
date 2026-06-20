Values:[]int = array{10, 20, 30, 20}

Slice := Values.Slice[1, 3]
Inserted := Values.Insert[2, array{25, 26}]
ElementRemoved := Values.RemoveElement[1]
Replaced := Values.ReplaceAllElements[20, 7]
PatternReplaced := Values.ReplaceAll[(20, 30), array{99}]

print("slice-first=" + str(Slice[0]))
print("first-20=" + str(Values.Find[20]))
print("inserted-len=" + str(Inserted.Length))
print("remove-element=" + str(ElementRemoved[1]))
print("replaced-last=" + str(Replaced[3]))
print("pattern=" + str(PatternReplaced))
print("missing-find=" + str(if (Index := Values.Find[99]). Index else. -1))
print("bad-slice=" + str(if (Bad := Values.Slice[3, 1]). Bad.Length else. 0))
