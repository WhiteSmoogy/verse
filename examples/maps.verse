var Scores:[string]int = map{
    "alice" => 10,
    "bob" => 20,
}

set Scores["alice"] += 5
set Scores["carol"] = 25

var Total:int = 0
for (Name -> Score : Scores) {
    print(Name + "=" + str(Score))
    set Total += Score
}

print("alice=" + str(Scores["alice"]))
print("missing=" + str(if (Score := Scores["missing"]). Score else. 0))
print("entries=" + str(Scores.Length))
print("total=" + str(Total))

Names:[]string = array{"alice", "missing", "carol"}
Selected:[]int = for (Name : Names, Score := Scores[Name]):
    Score
print("selected=" + str(Selected.Length + Selected[0] + Selected[1]))

MoreScores:[string]int = map{"bob" => 21, "dana" => 30}
Combined:[string]int = ConcatenateMaps(Scores, MoreScores)
print("combined-bob=" + str(Combined["bob"]))
print("combined-entries=" + str(Combined.Length))
