# FiddlerScript

FiddlerScript is a minimal C-style scripting language with a Rust-based interpreter, designed for inline message manipulation within Fiddler pipelines.

## Overview

FiddlerScript provides a lightweight alternative to Python for simple data transformations. It supports:

- **Data Types**: integers, strings, booleans, bytes, arrays, and dictionaries
- **Literal Syntax**: array literals `[1, 2, 3]` and dictionary literals `{"key": value}`
- **Control Flow**: `if-else` statements and `for` loops
- **Functions**: user-defined functions with recursion support
- **Built-ins**: `print()`, `len()`, `str()`, `int()`, `getenv()`, `parse_json()`, compression, base64, and more
- **Comments**: single-line comments with `//`

## Quick Example

```fiddlerscript
// Process a JSON message
let data = parse_json(this);
let name = get(data, "name");

if (name) {
    print("Processing user:", name);
} else {
    print("Unknown user");
}

// Create a response
let response = {"status": "ok", "processed": true};
this = bytes(response);
```

```fiddlerscript
// Split message into multiple outputs
let lines = lines(this);
let results = [];
for (let i = 0; i < len(lines); i = i + 1) {
    let line = get(lines, i);
    if (len(line) > 0) {
        results = push(results, bytes(line));
    }
}
this = results;
```

## Environment Variables

By default, FiddlerScript loads all OS environment variables as script variables, making them directly accessible:

```fiddlerscript
// Access environment variables directly
print(HOME);
print(USER);

// Or use getenv() for dynamic access
let key = "PATH";
print(getenv(key));
```

## Integration with Fiddler

FiddlerScript is designed to integrate seamlessly with Fiddler's message processing pipeline. Variables can be injected from Rust code before script execution, and results can be retrieved after execution.

```rust
use fiddler_script::{Interpreter, Value};

let mut interpreter = Interpreter::new();

// Inject message data
interpreter.set_variable_bytes("message", msg_bytes);
interpreter.set_variable_string("source", "input-queue");

// Run the script
interpreter.run(script_source)?;

// Retrieve results
if let Some(result) = interpreter.get_value("output") {
    // Process the result
}
```

## Documentation

- [Syntax Guide](syntax.md) - Language syntax and constructs
- [Built-in Functions](builtins.md) - Reference for all built-in functions
- [Integration Guide](integration.md) - Using FiddlerScript from Rust
