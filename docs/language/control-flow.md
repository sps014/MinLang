# Control Flow

## if / else

Parentheses around the condition are required. The body must be a block:

```minlang
if (score >= 90) {
    print("A\n");
} else if (score >= 70) {
    print("B\n");
} else {
    print("F\n");
}
```

There is no ternary operator; use an `if` block instead.

## while

Runs the body repeatedly as long as the condition is `true`:

```minlang
let i = 0;
while (i < 10) {
    print(i);
    i = i + 1;
}
```

## for

Three-part loop: initializer, condition, increment. All three parts are optional:

```minlang
for (let i = 0; i < 5; i = i + 1) {
    print(i);
}
```

The initializer runs once. The condition is checked before each iteration. The increment runs after each body execution.

## break and continue

`break` exits the nearest enclosing loop immediately:

```minlang
let i = 0;
while (true) {
    if (i >= 5) {
        break;
    }
    print(i);
    i = i + 1;
}
```

`continue` skips the rest of the current iteration and goes back to the condition check:

```minlang
for (let i = 0; i < 10; i = i + 1) {
    if (i % 2 == 0) {
        continue;   // skip even numbers
    }
    print(i);
}
```

Both `break` and `continue` produce a compile error if used outside a loop.
