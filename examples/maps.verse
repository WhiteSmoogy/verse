var Scores:[string]int = map{
    "alice" => 10,
    "bob" => 20,
}

AliceUpdated := if (set Scores["alice"] += 5). 0 else. 0
CarolUpdated := if (set Scores["carol"] = 25). 0 else. 0

var Total:int = 0
for (Name -> Score : Scores) {
    print(Name + "=" + str(Score))
    set Total += Score
}

print("alice=" + str(if (Score := Scores["alice"]). Score else. 0))
print("missing=" + str(if (Score := Scores["missing"]). Score else. 0))
print("entries=" + str(Scores.Length))
print("total=" + str(Total))

Names:[]string = array{"alice", "missing", "carol"}
Selected:[]int = for (Name : Names, Score := Scores[Name]):
    Score
print("selected=" + str(Selected.Length + if (A := Selected[0], B := Selected[1]). A + B else. 0))

MoreScores:[string]int = map{"bob" => 21, "dana" => 30}
Combined:[string]int = ConcatenateMaps(Scores, MoreScores)
print("combined-bob=" + str(if (Score := Combined["bob"]). Score else. 0))
print("combined-entries=" + str(Combined.Length))
