# FiddlerScript

FiddlerScript is a minimal C-style scripting language with a Rust-based interpreter, designed for inline message manipulation within Fiddler pipelines.

## Overview

FiddlerScript provides a lightweight alternative to Python for simple data transformations. It supports:

- **Variables**: integers, strings, booleans, and bytes
- **Control Flow**: `if-else` statements and `for` loops
- **Functions**: user-defined functions with recursion support
- **Built-ins**: `print()`, `len()`, `str()`, `int()`, `getenv()`, `parse_json()`, and more
- **Comments**: single-line comments with `//`

## Quick Example

```fiddlerscript
// Process a message
let count = 0;
for (let i = 0; i < len(message); i = i + 1) {
    count = count + 1;
}

if (count > 100) {
    print("Large message:", count, "bytes");
} else {
    print("Small message");
}
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
