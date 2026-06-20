state := enum:
    Menu
    (state:)Start
    Playing
    Paused

weapon := enum<open>{Sword, Bow}
keyword_enum := enum:
    (keyword_enum:)for
    Regular

var Current:state = state.Menu
set Current = state.Playing

StateID:[state]int = map{
    state.Menu => 0,
    state.Start => 1,
    state.Playing => 1,
    state.Paused => 2,
}

IsPlaying(Value:state):logic = Value = state.Playing
StateName(Value:state):string =
    case (Value):
        state.Menu => "menu"
        state.Start => "start"
        state.Playing => "playing"
        state.Paused => "paused"
        @ignore_unreachable state.Paused => "ignored"

print("current=" + str(Current))
print("is-playing=" + str(IsPlaying(Current)))
print("state-name=" + StateName(Current))
print("paused-id=" + str(StateID[state.Paused]))
print("weapon=" + str(weapon.Sword))
print("keyword=" + str(if (keyword_enum.for = keyword_enum.for) { 1 } else { 0 }))
