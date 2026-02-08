# Built-in Functions

FiddlerScript provides a set of built-in functions for common operations.

**Note:** Most built-in functions can be called using either function syntax `func(value, args)` or method syntax `value.func(args)`. Both are equivalent. Method syntax is useful for chaining operations.

## Output

### `print(args...)`

Print values to stdout.

**Arguments:** Any number of values
**Returns:** `null`

```fiddlerscript
print("Hello");                    // Hello
print("Count:", 42);               // Count: 42
print("a", "b", "c");              // a b c
```

## String and Bytes Operations

### `len(value)` / `value.len()`

Get the length of a string, bytes, array, or dictionary.

**Arguments:** A string, bytes, array, or dictionary value
**Returns:** Integer (length)

```fiddlerscript
// Function syntax
let s = "hello";
print(len(s));          // 5

// Method syntax
print(s.len());         // 5
print([1, 2, 3].len()); // 3
```

### `str(value)`

Convert any value to a string.

**Arguments:** Any value
**Returns:** String representation

```fiddlerscript
let n = 42;
let s = str(n);         // "42"

let b = true;
let bs = str(b);        // "true"
```

### `bytes(value)`

Convert any value to bytes.

**Arguments:** Any value
**Returns:** Bytes representation

```fiddlerscript
let b = bytes("hello");     // String to bytes
let n = bytes(42);          // "42" as bytes
let f = bytes(true);        // "true" as bytes
```

### `bytes_to_string(bytes)` / `bytes.bytes_to_string()`

Convert bytes to a UTF-8 string.

**Arguments:** Bytes or string value
**Returns:** String

```fiddlerscript
// Function syntax
let text = bytes_to_string(data);

// Method syntax
let text = data.bytes_to_string();
```

Invalid UTF-8 sequences are replaced with the Unicode replacement character.

## Type Conversion

### `int(value)`

Convert a value to an integer.

**Arguments:** String, integer, float, boolean, bytes, or null
**Returns:** Integer

```fiddlerscript
let n = int("42");      // 42
let b = int(true);      // 1
let f = int(false);     // 0
let z = int(null);      // 0
let t = int(3.99);      // 3 (truncates float)
let neg = int(-2.7);    // -2 (truncates toward zero)
```

Strings must contain valid integer representations, or a runtime error occurs.

### `float(value)`

Convert a value to a float.

**Arguments:** String, integer, float, or boolean
**Returns:** Float

```fiddlerscript
let f = float(42);          // 42.0
let pi = float("3.14159");  // 3.14159
let one = float(true);      // 1.0
let zero = float(false);    // 0.0
```

## Environment Variables

### `getenv(name)`

Get an environment variable by name.

**Arguments:** String (variable name)
**Returns:** String value or `null` if not found

```fiddlerscript
let home = getenv("HOME");
if (home) {
    print("Home directory:", home);
} else {
    print("HOME not set");
}
```

**Note:** Environment variables are also loaded as script variables by default, so you can access them directly:

```fiddlerscript
print(HOME);      // Same as getenv("HOME")
print(PATH);      // Same as getenv("PATH")
```

### `drop(value)`

Returns `null`. This is a convenience function for clearing variables or signaling that a value should be discarded.

**Arguments:** Any value (the value is ignored)
**Returns:** `null`

```fiddlerscript
let data = "temporary";
data = drop(data);        // data is now null

// Useful for clearing variables
let temp = some_computation();
// ... use temp ...
temp = drop(temp);        // Clear temp
```

**Note:** In the context of the FiddlerScript processor, setting `this` to `null` (either directly or via `drop()`) will filter out the message from the pipeline.

## JSON Processing

### `parse_json(data)`

Parse JSON from bytes or a string.

**Arguments:** Bytes or string containing valid JSON
**Returns:** Parsed value (dictionary, array, string, integer, boolean, or null)

```fiddlerscript
// Parse a JSON string
let s = parse_json("\"hello\"");      // "hello"

// Parse a JSON number
let n = parse_json("42");             // 42

// Parse a JSON boolean
let b = parse_json("true");           // true

// Parse a JSON null
let z = parse_json("null");           // null

// Parse a JSON object (returns dictionary)
let obj = parse_json("{\"name\": \"Alice\", \"age\": 30}");
let name = get(obj, "name");          // "Alice"

// Parse a JSON array (returns array)
let arr = parse_json("[1, 2, 3]");
let first = get(arr, 0);              // 1
```

**Supported JSON types:**
- `null` -> `null`
- `true`/`false` -> boolean
- numbers -> integer (floats are truncated)
- strings -> string
- arrays -> array
- objects -> dictionary

### `jmespath(data, expression)`

Query JSON data using [JMESPath](https://jmespath.org/) expressions. JMESPath is a query language for JSON that allows you to extract and transform elements from a JSON document.

**Arguments:**
- `data` - A dictionary or array (typically from `parse_json()`)
- `expression` - A JMESPath expression string

**Returns:** The extracted value, or `null` if not found

```fiddlerscript
// Simple key access
let data = parse_json("{\"name\": \"Alice\", \"age\": 30}");
let name = jmespath(data, "name");           // "Alice"

// Nested key access
let nested = parse_json("{\"user\": {\"profile\": {\"email\": \"alice@example.com\"}}}");
let email = jmespath(nested, "user.profile.email");  // "alice@example.com"

// Array indexing
let arr = parse_json("[\"a\", \"b\", \"c\"]");
let second = jmespath(arr, "[1]");           // "b"

// Array projections
let users = parse_json("[{\"name\": \"Alice\"}, {\"name\": \"Bob\"}]");
let names = jmespath(users, "[*].name");     // ["Alice", "Bob"]

// Filtering
let items = parse_json("[{\"price\": 10}, {\"price\": 25}, {\"price\": 5}]");
let expensive = jmespath(items, "[?price > `15`]");  // [{"price": 25}]
```

**Common JMESPath expressions:**
- `key` - Get a key from an object
- `key1.key2` - Nested key access
- `[0]` - Array index
- `[*]` - All array elements
- `[*].key` - Project key from all array elements
- `[?condition]` - Filter array elements
- `key1 || key2` - Return first non-null value

## Array Operations

### `array(args...)`

Create a new array from arguments.

**Arguments:** Any number of values
**Returns:** Array containing all arguments

```fiddlerscript
let arr = array(1, 2, 3);
let mixed = array("hello", 42, true);
let empty = array();
```

### `push(array, value)`

Add a value to the end of an array.

**Arguments:** An array, a value to add
**Returns:** A new array with the value appended

```fiddlerscript
let arr = array(1, 2);
let arr2 = push(arr, 3);  // array(1, 2, 3)
```

### `get(collection, index)`

Get a value from an array, dictionary, or string.

**Arguments:**
- For arrays: array, integer index
- For strings: string, integer index
- For dictionaries: dictionary, string key

**Returns:** The value at the index/key, or `null` if not found

```fiddlerscript
let arr = array("a", "b", "c");
let first = get(arr, 0);       // "a"

let s = "hello";
let char = get(s, 1);          // "e"

let d = set(dict(), "key", "value");
let v = get(d, "key");         // "value"
```

### `set(collection, index, value)`

Set a value in an array or dictionary.

**Arguments:**
- For arrays: array, integer index, value
- For dictionaries: dictionary, string key, value

**Returns:** A new array/dictionary with the value set

```fiddlerscript
let arr = array(1, 2, 3);
let arr2 = set(arr, 1, 99);  // array(1, 99, 3)

let d = dict();
let d2 = set(d, "name", "Alice");
```

### `is_array(value)`

Check if a value is an array.

**Arguments:** Any value
**Returns:** Boolean

```fiddlerscript
let arr = array(1, 2);
let check = is_array(arr);  // true
let check2 = is_array(42);  // false
```

## Dictionary Operations

### `dict()`

Create an empty dictionary.

**Arguments:** None
**Returns:** Empty dictionary

```fiddlerscript
let d = dict();
let d2 = set(d, "key", "value");
```

### `keys(dictionary)`

Get all keys from a dictionary.

**Arguments:** A dictionary
**Returns:** Array of keys (strings) in insertion order

```fiddlerscript
let d = set(set(dict(), "a", 1), "b", 2);
let k = keys(d);  // array("a", "b") - order is preserved
```

**Note:** Keys are returned in the order they were inserted into the dictionary.

### `is_dict(value)`

Check if a value is a dictionary.

**Arguments:** Any value
**Returns:** Boolean

```fiddlerscript
let d = dict();
let check = is_dict(d);     // true
let check2 = is_dict(42);   // false
```

### `delete(collection, key)`

Remove an element from a dictionary or array.

**Arguments:**
- For dictionaries: dictionary, string key
- For arrays: array, integer index

**Returns:** A new collection with the element removed

```fiddlerscript
let d = set(dict(), "name", "Alice");
let d2 = delete(d, "name");  // Empty dictionary

let arr = array(1, 2, 3);
let arr2 = delete(arr, 1);   // array(1, 3)
```

### `contains(collection, item)` / `collection.contains(item)`

Check if a collection contains a value or key.

**Arguments:**
- For arrays: array, value to search for
- For dictionaries: dictionary, string key to check

**Returns:** Boolean (`true` if found, `false` otherwise)

```fiddlerscript
// Array examples
let numbers = [1, 2, 3, 4, 5];
numbers.contains(3);           // true
numbers.contains(10);          // false
contains(numbers, 2);          // true (function syntax)

let fruits = ["apple", "banana"];
fruits.contains("banana");     // true

// Dictionary examples
let user = {"name": "Alice", "age": 30};
user.contains("name");         // true
user.contains("email");        // false
contains(user, "age");         // true (function syntax)

// In conditionals
if (users.contains("admin")) {
    print("Admin exists");
}
```

## String Operations

All string operations support both function and method syntax.

### `capitalize(string)` / `string.capitalize()`

Capitalize the first character of a string.

**Arguments:** String
**Returns:** String with first character uppercased

```fiddlerscript
let s = "hello".capitalize();  // "Hello"
let s2 = capitalize("HELLO");  // "HELLO"
```

### `lowercase(string)` / `string.lowercase()`

Convert a string to lowercase.

**Arguments:** String
**Returns:** Lowercase string

```fiddlerscript
let s = "Hello World".lowercase();  // "hello world"
```

### `uppercase(string)` / `string.uppercase()`

Convert a string to uppercase.

**Arguments:** String
**Returns:** Uppercase string

```fiddlerscript
let s = "Hello World".uppercase();  // "HELLO WORLD"
```

### `trim(string)` / `string.trim()`

Remove leading and trailing whitespace from a string.

**Arguments:** String
**Returns:** Trimmed string

```fiddlerscript
let s = "  hello  ".trim();  // "hello"
```

### `trim_prefix(string, prefix)`

Remove a prefix from a string if present.

**Arguments:** String, prefix string
**Returns:** String with prefix removed (or original if no match)

```fiddlerscript
let s = trim_prefix("hello world", "hello ");  // "world"
let s2 = trim_prefix("hello", "bye");          // "hello"
```

### `trim_suffix(string, suffix)`

Remove a suffix from a string if present.

**Arguments:** String, suffix string
**Returns:** String with suffix removed (or original if no match)

```fiddlerscript
let s = trim_suffix("hello.txt", ".txt");  // "hello"
let s2 = trim_suffix("hello", ".txt");     // "hello"
```

### `has_prefix(string, prefix)`

Check if a string starts with a prefix.

**Arguments:** String, prefix string
**Returns:** Boolean

```fiddlerscript
let check = has_prefix("hello world", "hello");  // true
let check2 = has_prefix("hello", "bye");         // false
```

### `has_suffix(string, suffix)`

Check if a string ends with a suffix.

**Arguments:** String, suffix string
**Returns:** Boolean

```fiddlerscript
let check = has_suffix("hello.txt", ".txt");  // true
let check2 = has_suffix("hello", ".txt");     // false
```

### `split(string, delimiter)` / `string.split(delimiter)`

Split a string by a delimiter.

**Arguments:** String, delimiter string
**Returns:** Array of strings

```fiddlerscript
let parts = "a,b,c".split(",");  // ["a", "b", "c"]
let words = "hello world".split(" ");  // ["hello", "world"]
```

### `reverse(string)`

Reverse a string.

**Arguments:** String
**Returns:** Reversed string

```fiddlerscript
let s = reverse("hello");  // "olleh"
```

### `lines(value)`

Split a string or bytes into lines.

**Arguments:** String or bytes
**Returns:** Array of strings (one per line)

```fiddlerscript
let text = "line1\nline2\nline3";
let arr = lines(text);  // array("line1", "line2", "line3")
```

Handles both Unix (`\n`) and Windows (`\r\n`) line endings.

## Compression

### `gzip_compress(data, level?)`

Compress data using gzip.

**Arguments:**
- `data` - String or bytes to compress
- `level` (optional) - Compression level 0-9 (default: 6). 0 = no compression, 9 = maximum compression.

**Returns:** Compressed bytes

```fiddlerscript
let compressed = gzip_compress("hello world");
let max_compressed = gzip_compress("hello world", 9);  // Maximum compression
let fast_compressed = gzip_compress("hello world", 1); // Fast compression
```

### `gzip_decompress(data)`

Decompress gzip-compressed data.

**Arguments:** Bytes
**Returns:** Decompressed bytes

```fiddlerscript
let original = gzip_decompress(compressed);
let text = bytes_to_string(original);
```

### `zlib_compress(data, level?)`

Compress data using zlib.

**Arguments:**
- `data` - String or bytes to compress
- `level` (optional) - Compression level 0-9 (default: 6). 0 = no compression, 9 = maximum compression.

**Returns:** Compressed bytes

```fiddlerscript
let compressed = zlib_compress("hello world");
let max_compressed = zlib_compress("hello world", 9);  // Maximum compression
```

### `zlib_decompress(data)`

Decompress zlib-compressed data.

**Arguments:** Bytes
**Returns:** Decompressed bytes

```fiddlerscript
let original = zlib_decompress(compressed);
```

### `deflate_compress(data, level?)`

Compress data using raw deflate.

**Arguments:**
- `data` - String or bytes to compress
- `level` (optional) - Compression level 0-9 (default: 6). 0 = no compression, 9 = maximum compression.

**Returns:** Compressed bytes

```fiddlerscript
let compressed = deflate_compress("hello world");
let max_compressed = deflate_compress("hello world", 9);  // Maximum compression
```

### `deflate_decompress(data)`

Decompress raw deflate-compressed data.

**Arguments:** Bytes
**Returns:** Decompressed bytes

```fiddlerscript
let original = deflate_decompress(compressed);
```

## Encoding

### `base64_encode(data)`

Encode data as base64.

**Arguments:** String or bytes
**Returns:** Base64-encoded string

```fiddlerscript
let encoded = base64_encode("hello");  // "aGVsbG8="
```

### `base64_decode(data)`

Decode base64-encoded data.

**Arguments:** String or bytes
**Returns:** Decoded bytes

```fiddlerscript
let decoded = base64_decode("aGVsbG8=");
let text = bytes_to_string(decoded);  // "hello"
```

## Math Functions

### `abs(value)` / `value.abs()`

Get the absolute value of a number.

**Arguments:** Integer or float
**Returns:** Same type as input (absolute value)

```fiddlerscript
abs(-42);          // 42
abs(-3.14);        // 3.14

let x = -10;
x.abs();           // 10 (method syntax)

let y = -2.5;
y.abs();           // 2.5 (method syntax)
```

### `ceil(value)` / `value.ceil()`

Ceiling function - rounds up to the nearest integer.

**Arguments:** Integer or float
**Returns:** Integer

```fiddlerscript
ceil(42);          // 42 (integers unchanged)
ceil(3.14);        // 4
ceil(-3.14);       // -3

let x = 2.5;
x.ceil();          // 3 (method syntax)
```

### `floor(value)` / `value.floor()`

Floor function - rounds down to the nearest integer.

**Arguments:** Integer or float
**Returns:** Integer

```fiddlerscript
floor(42);         // 42 (integers unchanged)
floor(3.99);       // 3
floor(-3.14);      // -4

let x = 2.5;
x.floor();         // 2 (method syntax)
```

### `round(value)` / `value.round()`

Round function - rounds to the nearest integer.

**Arguments:** Integer or float
**Returns:** Integer

```fiddlerscript
round(42);         // 42 (integers unchanged)
round(3.4);        // 3
round(3.5);        // 4
round(-3.5);       // -4

let x = 2.5;
x.round();         // 3 (method syntax)
```

## Time Functions

### `timestamp()`

Get the current Unix timestamp in seconds.

**Arguments:** None
**Returns:** Integer (seconds since Unix epoch)

```fiddlerscript
let now = timestamp();
print("Current time:", now);  // e.g., 1706284800
```

### `epoch()`

Alias for `timestamp()`. Get the current Unix timestamp in seconds.

**Arguments:** None
**Returns:** Integer (seconds since Unix epoch)

```fiddlerscript
let now = epoch();
```

### `timestamp_millis()`

Get the current Unix timestamp in milliseconds.

**Arguments:** None
**Returns:** Integer (milliseconds since Unix epoch)

```fiddlerscript
let now_ms = timestamp_millis();
print("Milliseconds:", now_ms);  // e.g., 1706284800123

// Measure elapsed time
let start = timestamp_millis();
// ... do work ...
let elapsed = timestamp_millis() - start;
print("Took", elapsed, "ms");
```

### `timestamp_micros()`

Get the current Unix timestamp in microseconds.

**Arguments:** None
**Returns:** Integer (microseconds since Unix epoch)

```fiddlerscript
let now_us = timestamp_micros();
print("Microseconds:", now_us);  // e.g., 1706284800123456
```

### `timestamp_iso8601()`

Get the current time as an ISO 8601 formatted string (RFC 3339).

**Arguments:** None
**Returns:** String in ISO 8601 format

```fiddlerscript
let now = timestamp_iso8601();
print("Current time:", now);  // e.g., "2024-01-26T12:34:56.789+00:00"

// Use in logging
fn log(message) {
    let ts = timestamp_iso8601();
    print("[" + ts + "]", message);
}

log("Application started");
```

## Summary Table

### Core Functions

| Function | Arguments | Returns | Description |
|----------|-----------|---------|-------------|
| `print(args...)` | Any values | `null` | Print to stdout |
| `len(value)` | String, bytes, array, or dict | Integer | Get length |
| `str(value)` | Any | String | Convert to string |
| `int(value)` | Any | Integer | Convert to integer (truncates floats) |
| `float(value)` | Numeric, string, or boolean | Float | Convert to float |
| `bytes(value)` | Any | Bytes | Convert to bytes |
| `bytes_to_string(value)` | Bytes or string | String | Convert bytes to string |
| `getenv(name)` | String | String or null | Get environment variable |
| `drop(value)` | Any | `null` | Returns null (clears variables) |

### JSON

| Function | Arguments | Returns | Description |
|----------|-----------|---------|-------------|
| `parse_json(data)` | Bytes or string | Dict, array, etc. | Parse JSON data |
| `jmespath(data, expr)` | Dict/array, string | Any | Query with JMESPath |

### Collections

| Function | Arguments | Returns | Description |
|----------|-----------|---------|-------------|
| `array(args...)` | Any values | Array | Create array |
| `push(arr, val)` | Array, any | Array | Append to array |
| `get(coll, key)` | Collection, key | Any | Get by index/key |
| `set(coll, key, val)` | Collection, key, any | Collection | Set by index/key |
| `delete(coll, key)` | Collection, key | Collection | Remove by index/key |
| `contains(coll, item)` | Collection, item | Boolean | Check if item/key exists |
| `dict()` | None | Dictionary | Create empty dict |
| `keys(dict)` | Dictionary | Array | Get keys (insertion order) |
| `is_array(val)` | Any | Boolean | Check if array |
| `is_dict(val)` | Any | Boolean | Check if dictionary |

### String Operations

| Function | Arguments | Returns | Description |
|----------|-----------|---------|-------------|
| `capitalize(str)` | String | String | Capitalize first character |
| `lowercase(str)` | String | String | Convert to lowercase |
| `uppercase(str)` | String | String | Convert to uppercase |
| `trim(str)` | String | String | Remove leading/trailing whitespace |
| `trim_prefix(str, prefix)` | String, String | String | Remove prefix if present |
| `trim_suffix(str, suffix)` | String, String | String | Remove suffix if present |
| `has_prefix(str, prefix)` | String, String | Boolean | Check if starts with prefix |
| `has_suffix(str, suffix)` | String, String | Boolean | Check if ends with suffix |
| `split(str, delim)` | String, String | Array | Split by delimiter |
| `reverse(str)` | String | String | Reverse string |
| `lines(value)` | String or bytes | Array | Split into lines |

### Compression

| Function | Arguments | Returns | Description |
|----------|-----------|---------|-------------|
| `gzip_compress(data, level?)` | Data, optional level (0-9) | Bytes | Gzip compress |
| `gzip_decompress(data)` | Bytes | Bytes | Gzip decompress |
| `zlib_compress(data, level?)` | Data, optional level (0-9) | Bytes | Zlib compress |
| `zlib_decompress(data)` | Bytes | Bytes | Zlib decompress |
| `deflate_compress(data, level?)` | Data, optional level (0-9) | Bytes | Deflate compress |
| `deflate_decompress(data)` | Bytes | Bytes | Deflate decompress |

### Encoding

| Function | Arguments | Returns | Description |
|----------|-----------|---------|-------------|
| `base64_encode(data)` | String or bytes | String | Encode as base64 |
| `base64_decode(data)` | String or bytes | Bytes | Decode from base64 |

### Math

| Function | Arguments | Returns | Description |
|----------|-----------|---------|-------------|
| `abs(val)` | Integer or float | Same type as input | Absolute value |
| `ceil(val)` | Integer or float | Integer | Ceiling (rounds up) |
| `floor(val)` | Integer or float | Integer | Floor (rounds down) |
| `round(val)` | Integer or float | Integer | Round to nearest |

### Time

| Function | Arguments | Returns | Description |
|----------|-----------|---------|-------------|
| `timestamp()` | None | Integer | Unix timestamp in seconds |
| `epoch()` | None | Integer | Alias for timestamp() |
| `timestamp_millis()` | None | Integer | Unix timestamp in milliseconds |
| `timestamp_micros()` | None | Integer | Unix timestamp in microseconds |
| `timestamp_iso8601()` | None | String | Current time in ISO 8601 format |
