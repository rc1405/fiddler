# FiddlerScript

A minimal C-style scripting language with a Rust-based interpreter.

## Features

- **Variables**: `let x = 10;` for integers, `let name = "hello";` for strings, and bytes support
- **Control Flow**: `if-else` statements and `for` loops with C-style syntax
- **Functions**: User-defined functions with `fn name(params) { ... }` syntax
- **Built-ins**: `print()`, `len()`, `str()`, `int()`, `getenv()`, `parse_json()`, `bytes_to_string()`, `bytes()`
- **Comments**: Single-line comments with `//`
- **Environment Variables**: All OS environment variables available as script variables
- **External Integration**: Set/get variables from Rust code, including bytes data

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
fiddler-script = { path = "../fiddler-script" }
```

### Basic Example

```rust
use fiddler_script::Interpreter;

fn main() {
    let source = r#"
        let x = 10;
        let y = 20;
        print(x + y);
    "#;

    let mut interpreter = Interpreter::new();
    interpreter.run(source).expect("Failed to run script");
}
```

### Injecting Variables

You can inject variables into the interpreter before running a script:

```rust
use fiddler_script::{Interpreter, Value};

fn main() {
    let mut interpreter = Interpreter::new();

    // Set variables of different types
    interpreter.set_variable_int("count", 42);
    interpreter.set_variable_string("name", "Alice");
    interpreter.set_variable_bytes("data", b"raw bytes".to_vec());
    interpreter.set_variable_value("flag", Value::Boolean(true));

    interpreter.run(r#"
        print("Count:", count);
        print("Name:", name);
        print("Data length:", len(data));
    "#).unwrap();
}
```

### Retrieving Variables

After running a script, you can retrieve variable values:

```rust
use fiddler_script::Interpreter;

fn main() {
    let mut interpreter = Interpreter::new();
    interpreter.run("let result = 10 * 5;").unwrap();

    // Get as Value type
    if let Some(value) = interpreter.get_value("result") {
        println!("Result: {}", value);
    }

    // Get as bytes
    if let Some(bytes) = interpreter.get_bytes("result") {
        println!("As bytes: {:?}", bytes);
    }

    // Check if variable exists
    if interpreter.has_variable("result") {
        println!("Variable exists!");
    }
}
```

### Custom Built-in Functions

```rust
use fiddler_script::{Interpreter, Value, RuntimeError};
use std::collections::HashMap;

fn main() {
    let mut builtins: HashMap<String, fn(Vec<Value>) -> Result<Value, RuntimeError>> = HashMap::new();

    builtins.insert("double".to_string(), |args| {
        if let Some(Value::Integer(n)) = args.first() {
            Ok(Value::Integer(n * 2))
        } else {
            Err(RuntimeError::InvalidArgument("Expected integer".to_string()))
        }
    });

    let mut interpreter = Interpreter::with_builtins(builtins);
    interpreter.run("print(double(21));").unwrap(); // Prints: 42
}
```

## Language Syntax

### Variables

```fiddlerscript
let x = 10;
let name = "hello";
let flag = true;
```

### Arithmetic

```fiddlerscript
let sum = 1 + 2;
let diff = 5 - 3;
let product = 4 * 5;
let quotient = 10 / 2;
let remainder = 7 % 3;
```

### Comparison and Logical Operators

```fiddlerscript
let a = 5 > 3;      // true
let b = 5 == 5;     // true
let c = 5 != 3;     // true
let d = true && false;  // false
let e = true || false;  // true
let f = !true;      // false
```

### Control Flow

```fiddlerscript
// If-else
if (x > 0) {
    print("positive");
} else if (x < 0) {
    print("negative");
} else {
    print("zero");
}

// For loop
for (let i = 0; i < 10; i = i + 1) {
    print(i);
}
```

### Functions

```fiddlerscript
fn add(a, b) {
    return a + b;
}

let result = add(5, 3);
print(result);  // 8
```

### Recursion

```fiddlerscript
fn factorial(n) {
    if (n <= 1) {
        return 1;
    }
    return n * factorial(n - 1);
}

print(factorial(5));  // 120
```

## Built-in Functions

| Function | Description |
|----------|-------------|
| `print(args...)` | Print values to stdout |
| `len(value)` | Get the length of a string or bytes |
| `str(value)` | Convert a value to a string |
| `int(value)` | Convert a value to an integer |
| `getenv(name)` | Get an environment variable by name (returns null if not found) |
| `parse_json(bytes)` | Parse JSON from bytes or string |
| `bytes_to_string(bytes)` | Convert bytes to a UTF-8 string |
| `bytes(value)` | Convert any value to bytes |

## Environment Variables

By default, the interpreter loads all OS environment variables as script variables:

```fiddlerscript
// Access environment variables directly
print(HOME);       // Prints home directory
print(PATH);       // Prints PATH

// Or use getenv() for dynamic access
let key = "HOME";
print(getenv(key));
```

Use `Interpreter::new_without_env()` to create an interpreter without loading environment variables.

## Working with Bytes

Bytes are useful for handling binary data and JSON:

```fiddlerscript
// Parse JSON data
let json = parse_json(json_data);  // json_data injected from Rust

// Convert bytes to string
let text = bytes_to_string(raw_bytes);

// Convert to bytes
let b = bytes("hello");  // String to bytes
let n = bytes(42);       // Integer to bytes (as string representation)
```

## License

MIT
