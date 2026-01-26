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

// Array operations (function or method syntax)
let first = arr.get(0);             // Get element at index
let arr3 = arr.push(4);             // Add element (returns new array)
let size = arr.len();               // Get length

// Equivalent function syntax
let first = get(arr, 0);
let arr3 = push(arr, 4);
let size = len(arr);
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
let d2 = d.set("name", "Alice");

// Dictionary operations (method syntax)
let name = person.get("name");      // Get value by key
let k = person.keys();              // Get array of keys (in insertion order)

// Equivalent function syntax
let name = get(person, "name");
let k = keys(person);
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

### Method Call Syntax

FiddlerScript supports method call syntax for built-in functions. Instead of passing a value as the first argument, you can call the function as a method on that value:

```fiddlerscript
// Function syntax (traditional)
let length = len("hello");
let lower = lowercase("HELLO");

// Method syntax (equivalent)
let length = "hello".len();
let lower = "HELLO".lowercase();
```

Both syntaxes are equivalent and can be used interchangeably. Method syntax is particularly useful for chaining operations:

```fiddlerscript
// Method chaining
let result = "  HELLO WORLD  "
    .trim()
    .lowercase()
    .split(" ");
// result is ["hello", "world"]

// Array method chaining
let arr = [1, 2, 3]
    .push(4)
    .push(5);
// arr is [1, 2, 3, 4, 5]

let length = arr.len();  // 5
```

**Available methods:**

| Type | Methods |
|------|---------|
| String | `len()`, `lowercase()`, `uppercase()`, `capitalize()`, `trim()`, `trim_prefix(s)`, `trim_suffix(s)`, `has_prefix(s)`, `has_suffix(s)`, `split(delim)`, `reverse()`, `lines()` |
| Array | `len()`, `get(index)`, `set(index, value)`, `push(value)`, `delete(index)`, `reverse()` |
| Dictionary | `len()`, `get(key)`, `set(key, value)`, `delete(key)`, `keys()` |
| Bytes | `len()`, `bytes_to_string()` |

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
