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

=== "Filter Messages"
    ```yml
    processors:
      - fiddlerscript:
          code: |
            let data = parse_json(this);
            let status = jmespath(data, "status");
            if (status == "ignore") {
                this = null;  // Filter out this message
            }
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

## Filtering Messages

To filter out (drop) a message from the pipeline, set `this` to `null` or an empty array:

```fiddlerscript
// Filter using null
let data = parse_json(this);
if (jmespath(data, "skip") == true) {
    this = null;  // Message will be filtered
}

// Or use drop() function
if (should_filter) {
    this = drop(this);
}

// Or use empty array
if (no_output_needed) {
    this = array();  // Also filters the message
}
```

Filtered messages are tracked in the `total_filtered` metric and treated as successful completions.

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
| `delete(collection, key)` | Remove element by index/key |
| `keys(dict)` | Get array of dictionary keys |
| `len(value)` | Get length of string/bytes/array/dict |

### String Operations
| Function | Description |
|----------|-------------|
| `capitalize(str)` | Capitalize first character |
| `lowercase(str)` | Convert to lowercase |
| `uppercase(str)` | Convert to uppercase |
| `trim(str)` | Remove leading/trailing whitespace |
| `trim_prefix(str, prefix)` | Remove prefix if present |
| `trim_suffix(str, suffix)` | Remove suffix if present |
| `has_prefix(str, prefix)` | Check if starts with prefix |
| `has_suffix(str, suffix)` | Check if ends with suffix |
| `split(str, delim)` | Split by delimiter into array |
| `reverse(str)` | Reverse string |
| `lines(value)` | Split string/bytes into lines |

### Compression
| Function | Description |
|----------|-------------|
| `gzip_compress(data)` | Compress with gzip |
| `gzip_decompress(data)` | Decompress gzip data |
| `zlib_compress(data)` | Compress with zlib |
| `zlib_decompress(data)` | Decompress zlib data |
| `deflate_compress(data)` | Compress with deflate |
| `deflate_decompress(data)` | Decompress deflate data |

### Encoding
| Function | Description |
|----------|-------------|
| `base64_encode(data)` | Encode as base64 string |
| `base64_decode(data)` | Decode from base64 to bytes |

### JSON
| Function | Description |
|----------|-------------|
| `parse_json(bytes)` | Parse JSON bytes into values |
| `jmespath(data, expr)` | Query data using JMESPath expressions |

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
| `drop(value)` | Returns null (useful for filtering messages) |

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

### Extract Nested Fields with JMESPath
```yml
processors:
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        // Extract deeply nested field
        let email = jmespath(data, "user.profile.contact.email");
        // Extract all names from an array
        let names = jmespath(data, "users[*].name");
        this = bytes(str(names));
```

### Filter Messages by Content
```yml
processors:
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        let level = jmespath(data, "level");
        // Only keep error and warning messages
        if (level != "error" && level != "warning") {
            this = null;  // Filter out info/debug messages
        }
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

### Decompress and Process
```yml
processors:
  - fiddlerscript:
      code: |
        // Decompress gzip data
        let decompressed = gzip_decompress(this);
        let text = bytes_to_string(decompressed);

        // Process and recompress
        let processed = uppercase(text);
        this = gzip_compress(processed);
```

### Base64 Decode and Parse
```yml
processors:
  - fiddlerscript:
      code: |
        // Decode base64 payload
        let decoded = base64_decode(this);
        let json = parse_json(decoded);
        let message = get(json, "message");
        this = bytes(message);
```

### Split Lines into Messages
```yml
processors:
  - fiddlerscript:
      code: |
        // Split multi-line input into separate messages
        let all_lines = lines(this);
        let messages = array();
        for (let i = 0; i < len(all_lines); i = i + 1) {
            let line = get(all_lines, i);
            let trimmed = trim(line);
            if (len(trimmed) > 0) {
                messages = push(messages, bytes(trimmed));
            }
        }
        this = messages;
```

### String Manipulation
```yml
processors:
  - fiddlerscript:
      code: |
        let text = bytes_to_string(this);

        // Remove prefix and normalize
        if (has_prefix(text, "LOG:")) {
            text = trim_prefix(text, "LOG:");
            text = trim(text);
            text = uppercase(text);
        }

        this = bytes(text);
```

### CSV Line Parsing
```yml
processors:
  - fiddlerscript:
      code: |
        let text = bytes_to_string(this);
        let fields = split(text, ",");

        // Create JSON from CSV fields
        let json = "{";
        json = json + "\"name\": \"" + trim(get(fields, 0)) + "\", ";
        json = json + "\"value\": \"" + trim(get(fields, 1)) + "\"";
        json = json + "}";

        this = bytes(json);
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
| Compression | gzip, zlib, deflate | Any library |
| Encoding | Base64 | Any encoding |
| String methods | Built-in | Full Python |

Use FiddlerScript for:
- Simple to moderate transformations
- JSON parsing and extraction
- Message splitting
- Compression/decompression (gzip, zlib, deflate)
- Base64 encoding/decoding
- String manipulation (trim, split, case conversion)
- When you want no external dependencies

Use Python for:
- Complex data processing
- When you need external libraries
- Regular expressions
- Advanced numeric operations
