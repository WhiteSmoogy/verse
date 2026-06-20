damage_type := enum{Physical, Fire}

character := struct{}

vector2 := struct:
    X : int = 0
    Y : int = 0

damage_info := struct:
    Amount : int = 0
    Type : damage_type = damage_type.Physical
    Source : ?character = false
    IsCritical : logic = false

Origin := vector2 . X:=0
PlayerPos:vector2 = vector2:
    let:
        Base:int = 40
        Offset:int = 2
    X := Base
    Y := Offset
Hit := damage_info{Amount := 42; Type := damage_type.Fire}

print("pos=" + str(PlayerPos.X + PlayerPos.Y))
print("origin=" + str(if (Origin = vector2{}) { 1 } else { 0 }))
print("hit=" + str(Hit.Amount))
