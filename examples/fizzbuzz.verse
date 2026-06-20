label(n:int):string = if (n % 15 = 0) {
    "FizzBuzz"
} else if (n % 3 = 0) {
    "Fizz"
} else if (n % 5 = 0) {
    "Buzz"
} else {
    str(n)
}

var n:int = 1
loop {
    if (n > 20) {
        break
    }
    print(label(n))
    set n += 1
}
