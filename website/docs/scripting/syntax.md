# FiddlerScript Syntax

This document describes the complete syntax of the FiddlerScript language.

## Variables

Variables are declared using the `let` keyword and can hold integers, strings, booleans, or bytes.

```fiddlerscript
let x = 10;              // Integer
let name = "hello";      // String
let flag = true;         // Boolean
let empty = false;       // Boolean
```

Variables can be reassigned after declaration:

```fiddlerscript
let x = 10;
x = 20;      // Reassignment
x = x + 5;   // Using current value
```

## Data Types

### Integers

64-bit signed integers supporting standard arithmetic operations.

```fiddlerscript
let a = 42;
let b = -10;
let c = 0;
```

### Strings

UTF-8 strings with escape sequence support.

```fiddlerscript
let greeting = "Hello, World!";
let multiline = "Line 1\nLine 2";
let quoted = "She said \"Hi\"";
let path = "C:\\Users\\name";
```

**Supported escape sequences:**
- `\n` - newline
- `\t` - tab
- `\r` - carriage return
- `\\` - backslash
- `\"` - double quote

### Booleans

Boolean values for logical operations.

```fiddlerscript
let yes = true;
let no = false;
```

### Bytes

Raw binary data, typically injected from Rust code.

```fiddlerscript
// Bytes are usually injected from external code
// Convert to string for processing
let text = bytes_to_string(data);
```

### Arrays

Ordered collections of values. Arrays can be created using literal syntax or the `array()` function.

```fiddlerscript
// Array literal syntax
let arr = [1, 2, 3];
let mixed = ["hello", 42, true];
let empty = [];

// Or using array() function
let arr2 = array(1, 2, 3);

// Array operations
let first = get(arr, 0);            // Get element at index
let arr3 = push(arr, 4);            // Add element (returns new array)
let size = len(arr);                // Get length
```

### Dictionaries

Key-value collections with string keys. Dictionaries preserve insertion order when iterating.

```fiddlerscript
// Dictionary literal syntax
let person = {"name": "Alice", "age": 30};
let nested = {"user": {"id": 1, "active": true}};
let empty = {};

// Or using dict() and set() functions
let d = dict();
let d2 = set(d, "name", "Alice");

// Dictionary operations
let name = get(person, "name");     // Get value by key
let k = keys(person);               // Get array of keys (in insertion order)
```

**Note:** Dictionary keys are always strings. When iterating with `keys()`, the order matches the order in which keys were inserted.

### Null

Represents the absence of a value.

```fiddlerscript
let result = getenv("NONEXISTENT");  // Returns null
```

## Operators

### Arithmetic Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `+` | Addition (integers) or concatenation (strings) | `5 + 3` |
| `-` | Subtraction | `5 - 3` |
| `*` | Multiplication | `5 * 3` |
| `/` | Integer division | `5 / 3` |
| `%` | Modulo (remainder) | `5 % 3` |

```fiddlerscript
let sum = 10 + 5;        // 15
let diff = 10 - 5;       // 5
let product = 10 * 5;    // 50
let quotient = 10 / 3;   // 3 (integer division)
let remainder = 10 % 3;  // 1
```

### String Concatenation

The `+` operator concatenates strings:

```fiddlerscript
let greeting = "Hello" + " " + "World";  // "Hello World"
```

### Comparison Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `==` | Equal to | `x == 5` |
| `!=` | Not equal to | `x != 5` |
| `<` | Less than | `x < 5` |
| `<=` | Less than or equal | `x <= 5` |
| `>` | Greater than | `x > 5` |
| `>=` | Greater than or equal | `x >= 5` |

```fiddlerscript
let a = 5 > 3;   // true
let b = 5 == 5;  // true
let c = 5 != 3;  // true
let d = 5 <= 5;  // true
```

### Logical Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `&&` | Logical AND | `a && b` |
| `\|\|` | Logical OR | `a \|\| b` |
| `!` | Logical NOT | `!a` |

```fiddlerscript
let a = true && false;   // false
let b = true || false;   // true
let c = !true;           // false
```

### Unary Operators

| Operator | Description | Example |
|----------|-------------|---------|
| `-` | Negation | `-5` |
| `!` | Logical NOT | `!true` |

```fiddlerscript
let neg = -42;      // -42
let inv = !true;    // false
```

## Control Flow

### If-Else Statements

```fiddlerscript
if (condition) {
    // executed if condition is true
}
```

```fiddlerscript
if (x > 0) {
    print("positive");
} else {
    print("non-positive");
}
```

```fiddlerscript
if (x > 0) {
    print("positive");
} else if (x < 0) {
    print("negative");
} else {
    print("zero");
}
```

### Truthiness

The following values are considered "falsy":
- `false`
- `0` (integer zero)
- `""` (empty string)
- Empty bytes
- `null`

All other values are considered "truthy".

### For Loops

C-style for loops with initialization, condition, and update expressions:

```fiddlerscript
for (let i = 0; i < 10; i = i + 1) {
    print(i);
}
```

All parts are optional:

```fiddlerscript
// No initialization
let i = 0;
for (; i < 5; i = i + 1) {
    print(i);
}

// Infinite loop (use with caution)
for (;;) {
    // ...
}
```

## Functions

### Function Definition

Functions are defined with the `fn` keyword:

```fiddlerscript
fn add(a, b) {
    return a + b;
}
```

### Function Calls

```fiddlerscript
let result = add(5, 3);  // 8
print(result);
```

### Return Statement

Functions return values using the `return` statement:

```fiddlerscript
fn max(a, b) {
    if (a > b) {
        return a;
    }
    return b;
}
```

Functions without an explicit return statement return `null`.

### Recursion

Functions can call themselves recursively:

```fiddlerscript
fn factorial(n) {
    if (n <= 1) {
        return 1;
    }
    return n * factorial(n - 1);
}

print(factorial(5));  // 120
```

```fiddlerscript
fn fibonacci(n) {
    if (n <= 1) {
        return n;
    }
    return fibonacci(n - 1) + fibonacci(n - 2);
}
```

**Note:** FiddlerScript has a maximum recursion depth of 64 nested function calls to prevent stack overflow. Exceeding this limit will result in a runtime error.

## Blocks and Scope

Code blocks create new scopes. Variables declared inside a block are not visible outside:

```fiddlerscript
let x = 10;
if (true) {
    let x = 20;  // This shadows the outer x
    print(x);    // Prints 20
}
print(x);        // Prints 10
```

## Comments

Single-line comments start with `//`:

```fiddlerscript
// This is a comment
let x = 10;  // This is also a comment

// Multi-line comments are written
// using multiple single-line comments
```

## Operator Precedence

From highest to lowest precedence:

1. Unary operators: `!`, `-`
2. Multiplicative: `*`, `/`, `%`
3. Additive: `+`, `-`
4. Comparison: `<`, `<=`, `>`, `>=`
5. Equality: `==`, `!=`
6. Logical AND: `&&`
7. Logical OR: `||`
8. Assignment: `=`

Use parentheses to override precedence:

```fiddlerscript
let a = 2 + 3 * 4;       // 14 (multiplication first)
let b = (2 + 3) * 4;     // 20 (addition first due to parentheses)
```
