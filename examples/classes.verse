player := class:
    Name : string
    var Score : int = 0
    var Ready : int = 0

    block:
        set Self.Ready = 1

    AddScore(Points:int)<transacts>:void =
        set Self.Score += Points

    Label():string =
        Self.Name + "=" + str(Self.Score) + ":" + str(Self.Ready)

ranked_player := class(player):
    Rank : int = 1

    AddScore<override>(Points:int)<transacts>:void =
        (super:)AddScore(Points)

    RankedLabel():string =
        Self.Name + ":" + str(Self.Rank) + ":" + str(Self.Score) + ":" + str(Self.Ready)

    ParentLabel():string =
        super{Name := Self.Name, Score := Self.Score}.Label()

token := class<unique>:
    Label : string

list_node := class:
    Data : any
    var Next : ?list_node = false

int_node := class(list_node):
    Data<override> : int
    var Next<override> : ?int_node = false

MakePlayer<constructor>(InName:string, InitialScore:int):player =
    player:
        Name := InName
        Score := InitialScore

Hero:player = MakePlayer("Ava", 0)
Alias := Hero
Champion:ranked_player = ranked_player{Name := "Mira", Score := 20, Rank := 7}
FirstToken := token{Label := "same"}
SecondToken := token{Label := "same"}
TokenScores:[token]int = map{FirstToken => 20, SecondToken => 22}
Head := int_node{Data := 40}
Tail := int_node{Data := 2}

set Alias.Score = 10
Hero.AddScore(32)
Champion.AddScore(22)
set Head.Next = option{Tail}

print(Hero.Label())
print(Champion.RankedLabel())
print(Champion.ParentLabel())
print("unique=" + str(if (FirstToken <> SecondToken). TokenScores[FirstToken] + TokenScores[SecondToken] else. 0))
print("override=" + str(Head.Data + if (Next := Head.Next?). Next.Data else. 0))
