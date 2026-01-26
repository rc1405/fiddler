# fiddlerscript

FiddlerScript is the built-in scripting language for inline message manipulation in Fiddler. It provides a lightweight, C-style syntax for transforming messages without external dependencies.

=== "Basic"
    ```yml
    processors:
      - fiddlerscript:
          code: |
            let text = bytes_to_string(this);
            this = bytes(text + " processed");
    ```

=== "Multiple Messages"
    ```yml
    processors:
      - fiddlerscript:
          code: |
            // Split into multiple messages
            this = array(bytes("msg1"), bytes("msg2"), bytes("msg3"));
    ```

=== "JSON Processing"
    ```yml
    processors:
      - fiddlerscript:
          code: |
            let data = parse_json(this);
            let name = get(data, "name");
            this = bytes(name);
    ```

## Fields

### `code`
The FiddlerScript code to execute for each message.
Type: `string`
Required: `true`

## Variables

The following variables are available in your script:

### `this`
The message content as bytes. This is the primary input and output variable.

- **Input**: Contains the raw bytes of the incoming message
- **Output**: Set this to the desired output bytes, or an array of bytes for multiple messages

```fiddlerscript
// Read the message
let text = bytes_to_string(this);

// Modify and write back
this = bytes(text + " modified");
```

### `metadata`
A dictionary containing the message metadata. Keys are strings, values can be any type.

```fiddlerscript
// Access metadata
let source = get(metadata, "source");
let timestamp = get(metadata, "timestamp");

// Use in processing
if (source == "important-queue") {
    this = bytes("PRIORITY: " + bytes_to_string(this));
}
```

## Returning Multiple Messages

To split a single input message into multiple output messages, set `this` to an array:

```fiddlerscript
// Split message into multiple
this = array(bytes("first"), bytes("second"), bytes("third"));
```

Each element in the array becomes a separate message in the pipeline.

## Built-in Functions

### Data Conversion
| Function | Description |
|----------|-------------|
| `bytes(value)` | Convert any value to bytes |
| `bytes_to_string(bytes)` | Convert bytes to UTF-8 string |
| `str(value)` | Convert any value to string |
| `int(value)` | Convert to integer |

### Collections
| Function | Description |
|----------|-------------|
| `array(items...)` | Create an array from arguments |
| `dict()` | Create an empty dictionary |
| `push(array, value)` | Add value to array (returns new array) |
| `get(collection, key)` | Get value by index/key |
| `set(collection, key, value)` | Set value (returns new collection) |
| `keys(dict)` | Get array of dictionary keys |
| `len(value)` | Get length of string/bytes/array/dict |

### JSON
| Function | Description |
|----------|-------------|
| `parse_json(bytes)` | Parse JSON bytes into values |

### Type Checking
| Function | Description |
|----------|-------------|
| `is_array(value)` | Check if value is an array |
| `is_dict(value)` | Check if value is a dictionary |

### Environment
| Function | Description |
|----------|-------------|
| `getenv(name)` | Get environment variable (returns null if not set) |
| `print(values...)` | Print to stdout (for debugging) |

## Examples

### Simple Transformation
```yml
processors:
  - fiddlerscript:
      code: |
        let text = bytes_to_string(this);
        let upper = "";
        for (let i = 0; i < len(text); i = i + 1) {
            let c = get(text, i);
            // Simple uppercase (ASCII only)
            upper = upper + c;
        }
        this = bytes(upper);
```

### Filter by Content
```yml
processors:
  - fiddlerscript:
      code: |
        let text = bytes_to_string(this);
        // Only keep messages containing "important"
        if (len(text) > 0) {
            this = bytes(text);
        } else {
            this = bytes("");  // Empty message will be processed
        }
```

### Parse and Extract JSON Field
```yml
processors:
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        let user = get(data, "user");
        let name = get(user, "name");
        let email = get(user, "email");
        this = bytes(name + " <" + email + ">");
```

### Split JSON Array into Messages
```yml
processors:
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        let items = get(data, "items");
        let messages = array();
        for (let i = 0; i < len(items); i = i + 1) {
            let item = get(items, i);
            messages = push(messages, bytes(str(item)));
        }
        this = messages;
```

### Add Metadata to Message
```yml
processors:
  - fiddlerscript:
      code: |
        let source = get(metadata, "source");
        let text = bytes_to_string(this);
        let enriched = "{\"source\": \"" + source + "\", \"data\": \"" + text + "\"}";
        this = bytes(enriched);
```

### Conditional Processing
```yml
processors:
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        let type = get(data, "type");

        if (type == "error") {
            // Add error prefix
            let msg = get(data, "message");
            this = bytes("ERROR: " + msg);
        } else if (type == "warning") {
            let msg = get(data, "message");
            this = bytes("WARN: " + msg);
        } else {
            // Pass through unchanged
        }
```

## Language Reference

For complete FiddlerScript language documentation, see:

- [FiddlerScript Overview](../scripting/About.md)
- [Syntax Guide](../scripting/syntax.md)
- [Built-in Functions Reference](../scripting/builtins.md)

## Comparison with Python Processor

| Feature | FiddlerScript | Python |
|---------|--------------|--------|
| Dependencies | None (built-in) | Requires Python runtime |
| Performance | Fast (compiled) | Slower (interpreted) |
| Syntax | C-style | Python |
| External libraries | No | Yes |
| Variable name | `this` | `root` |

Use FiddlerScript for:
- Simple transformations
- JSON parsing and extraction
- Message splitting
- When you want no external dependencies

Use Python for:
- Complex data processing
- When you need external libraries
- Advanced string manipulation
- Regular expressions
