Values:[]int = array{10, 20, 30, 20}

Slice := if (Value := Values.Slice[1, 3]). Value else. array{}
Inserted := if (Value := Values.Insert[2, array{25, 26}]). Value else. Values
ElementRemoved := if (Value := Values.RemoveElement[1]). Value else. Values
Replaced := Values.ReplaceAllElements[20, 7]
PatternReplaced := if (Value := Values.ReplaceAll[(20, 30), array{99}]). Value else. Values

print("slice-first=" + str(if (First := Slice[0]). First else. 0))
print("first-20=" + str(if (Index := Values.Find[20]). Index else. -1))
print("inserted-len=" + str(Inserted.Length))
print("remove-element=" + str(if (Value := ElementRemoved[1]). Value else. 0))
print("replaced-last=" + str(if (Value := Replaced[3]). Value else. 0))
print("pattern=" + str(PatternReplaced))
print("missing-find=" + str(if (Index := Values.Find[99]). Index else. -1))
print("bad-slice=" + str(if (Bad := Values.Slice[3, 1]). Bad.Length else. 0))
