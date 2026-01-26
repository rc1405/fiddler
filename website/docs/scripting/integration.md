# Integration Guide

This guide covers how to use FiddlerScript from Rust code.

## Creating an Interpreter

### Standard Interpreter

The standard interpreter loads all OS environment variables as script variables:

```rust
use fiddler_script::Interpreter;

let mut interpreter = Interpreter::new();
```

### Interpreter Without Environment Variables

For a clean environment without OS variables:

```rust
use fiddler_script::Interpreter;

let mut interpreter = Interpreter::new_without_env();
```

### Interpreter with Custom Built-ins

Add custom built-in functions:

```rust
use fiddler_script::{Interpreter, Value, RuntimeError};
use std::collections::HashMap;

let mut builtins: HashMap<String, fn(Vec<Value>) -> Result<Value, RuntimeError>> = HashMap::new();

builtins.insert("double".to_string(), |args| {
    if let Some(Value::Integer(n)) = args.first() {
        Ok(Value::Integer(n * 2))
    } else {
        Err(RuntimeError::InvalidArgument("Expected integer".to_string()))
    }
});

let mut interpreter = Interpreter::with_builtins(builtins);
```

## Running Scripts

### Basic Execution

```rust
use fiddler_script::Interpreter;

let mut interpreter = Interpreter::new();
let result = interpreter.run("let x = 10 + 5; x;");

match result {
    Ok(value) => println!("Result: {}", value),
    Err(e) => eprintln!("Error: {}", e),
}
```

### Error Handling

FiddlerScript provides three error types wrapped in `FiddlerError`:

```rust
use fiddler_script::{Interpreter, FiddlerError, LexError, ParseError, RuntimeError};

let mut interpreter = Interpreter::new();
let result = interpreter.run(source);

match result {
    Ok(value) => println!("Success: {}", value),
    Err(FiddlerError::Lex(e)) => eprintln!("Lexer error: {}", e),
    Err(FiddlerError::Parse(e)) => eprintln!("Parser error: {}", e),
    Err(FiddlerError::Runtime(e)) => eprintln!("Runtime error: {}", e),
}
```

## Injecting Variables

### Setting Variables Before Execution

```rust
use fiddler_script::{Interpreter, Value};

let mut interpreter = Interpreter::new();

// Set different types
interpreter.set_variable_int("count", 42);
interpreter.set_variable_string("name", "Alice");
interpreter.set_variable_bytes("data", b"raw bytes".to_vec());
interpreter.set_variable_value("flag", Value::Boolean(true));

interpreter.run(r#"
    print("Count:", count);
    print("Name:", name);
    print("Data length:", len(data));
    print("Flag:", flag);
"#).unwrap();
```

### Variable Setters

| Method | Description |
|--------|-------------|
| `set_variable_value(name, Value)` | Set any Value type |
| `set_variable_int(name, i64)` | Set an integer |
| `set_variable_string(name, impl Into<String>)` | Set a string |
| `set_variable_bytes(name, Vec<u8>)` | Set bytes |

## Retrieving Variables

### Getting Values After Execution

```rust
use fiddler_script::{Interpreter, Value};

let mut interpreter = Interpreter::new();
interpreter.run("let result = 10 * 5;").unwrap();

// Get as Value type
if let Some(value) = interpreter.get_value("result") {
    match value {
        Value::Integer(n) => println!("Integer: {}", n),
        Value::String(s) => println!("String: {}", s),
        Value::Boolean(b) => println!("Boolean: {}", b),
        Value::Bytes(bytes) => println!("Bytes: {} bytes", bytes.len()),
        Value::Null => println!("Null"),
    }
}

// Get as bytes (converts any value to bytes)
if let Some(bytes) = interpreter.get_bytes("result") {
    println!("As bytes: {:?}", bytes);
}

// Check if variable exists
if interpreter.has_variable("result") {
    println!("Variable exists");
}
```

### Variable Getters

| Method | Returns | Description |
|--------|---------|-------------|
| `get_value(name)` | `Option<Value>` | Get variable as Value type |
| `get_bytes(name)` | `Option<Vec<u8>>` | Get variable as bytes |
| `has_variable(name)` | `bool` | Check if variable exists |

## The Value Type

The `Value` enum represents all FiddlerScript values:

```rust
pub enum Value {
    Integer(i64),
    String(String),
    Boolean(bool),
    Bytes(Vec<u8>),
    Null,
}
```

### Converting Values to Bytes

All values can be converted to bytes:

```rust
use fiddler_script::Value;

let v = Value::Integer(42);
let bytes = v.to_bytes();  // "42" as bytes

let v = Value::String("hello".to_string());
let bytes = v.to_bytes();  // "hello" as bytes

let v = Value::Boolean(true);
let bytes = v.to_bytes();  // "true" as bytes
```

## Complete Example

```rust
use fiddler_script::{Interpreter, Value};

fn process_message(message: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut interpreter = Interpreter::new();

    // Inject the message
    interpreter.set_variable_bytes("input", message.to_vec());
    interpreter.set_variable_int("timestamp", chrono::Utc::now().timestamp());

    // Run the processing script
    let script = r#"
        let text = bytes_to_string(input);
        let processed = text + " [processed at " + str(timestamp) + "]";
        let output = bytes(processed);
    "#;

    interpreter.run(script)?;

    // Get the result
    match interpreter.get_bytes("output") {
        Some(bytes) => Ok(bytes),
        None => Err("No output produced".into()),
    }
}
```

## Reusing the Interpreter

The interpreter maintains state between `run()` calls, so you can define functions once and use them later:

```rust
use fiddler_script::Interpreter;

let mut interpreter = Interpreter::new();

// Define helper functions
interpreter.run(r#"
    fn double(x) { return x * 2; }
    fn triple(x) { return x * 3; }
"#).unwrap();

// Use them in subsequent calls
let result = interpreter.run("double(21);").unwrap();
println!("{}", result);  // 42

let result = interpreter.run("triple(14);").unwrap();
println!("{}", result);  // 42
```

## Thread Safety

The `Interpreter` is not thread-safe. Each thread should have its own interpreter instance:

```rust
use fiddler_script::Interpreter;
use std::thread;

let handles: Vec<_> = (0..4).map(|i| {
    thread::spawn(move || {
        let mut interpreter = Interpreter::new();
        interpreter.set_variable_int("thread_id", i);
        interpreter.run("print(\"Thread\", thread_id);").unwrap();
    })
}).collect();

for handle in handles {
    handle.join().unwrap();
}
```
