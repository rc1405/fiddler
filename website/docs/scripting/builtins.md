# Built-in Functions

FiddlerScript provides a set of built-in functions for common operations.

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

### `len(value)`

Get the length of a string or bytes.

**Arguments:** A string or bytes value
**Returns:** Integer (length)

```fiddlerscript
let s = "hello";
print(len(s));          // 5

let b = bytes("test");
print(len(b));          // 4
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

### `bytes_to_string(bytes)`

Convert bytes to a UTF-8 string.

**Arguments:** Bytes or string value
**Returns:** String

```fiddlerscript
let text = bytes_to_string(data);
print(text);
```

Invalid UTF-8 sequences are replaced with the Unicode replacement character.

## Type Conversion

### `int(value)`

Convert a value to an integer.

**Arguments:** String, integer, boolean, bytes, or null
**Returns:** Integer

```fiddlerscript
let n = int("42");      // 42
let b = int(true);      // 1
let f = int(false);     // 0
let z = int(null);      // 0
```

Strings must contain valid integer representations, or a runtime error occurs.

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

## JSON Processing

### `parse_json(data)`

Parse JSON from bytes or a string.

**Arguments:** Bytes or string containing valid JSON
**Returns:** Parsed value (string, integer, boolean, or null)

```fiddlerscript
// Parse a JSON string
let s = parse_json("\"hello\"");      // "hello"

// Parse a JSON number
let n = parse_json("42");             // 42

// Parse a JSON boolean
let b = parse_json("true");           // true

// Parse a JSON null
let z = parse_json("null");           // null

// Parse a JSON object (returns string representation)
let obj = parse_json("{\"key\": \"value\"}");
```

**Supported JSON types:**
- `null` -> `null`
- `true`/`false` -> boolean
- numbers -> integer (floats are truncated)
- strings -> string
- arrays -> JSON string representation
- objects -> JSON string representation

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
**Returns:** Array of keys (strings)

```fiddlerscript
let d = set(set(dict(), "a", 1), "b", 2);
let k = keys(d);  // array("a", "b")
```

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

## String Operations

### `capitalize(string)`

Capitalize the first character of a string.

**Arguments:** String
**Returns:** String with first character uppercased

```fiddlerscript
let s = capitalize("hello");  // "Hello"
let s2 = capitalize("HELLO"); // "HELLO"
```

### `lowercase(string)`

Convert a string to lowercase.

**Arguments:** String
**Returns:** Lowercase string

```fiddlerscript
let s = lowercase("Hello World");  // "hello world"
```

### `uppercase(string)`

Convert a string to uppercase.

**Arguments:** String
**Returns:** Uppercase string

```fiddlerscript
let s = uppercase("Hello World");  // "HELLO WORLD"
```

### `trim(string)`

Remove leading and trailing whitespace from a string.

**Arguments:** String
**Returns:** Trimmed string

```fiddlerscript
let s = trim("  hello  ");  // "hello"
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

### `split(string, delimiter)`

Split a string by a delimiter.

**Arguments:** String, delimiter string
**Returns:** Array of strings

```fiddlerscript
let parts = split("a,b,c", ",");  // array("a", "b", "c")
let words = split("hello world", " ");  // array("hello", "world")
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

### `gzip_compress(data)`

Compress data using gzip.

**Arguments:** String or bytes
**Returns:** Compressed bytes

```fiddlerscript
let compressed = gzip_compress("hello world");
```

### `gzip_decompress(data)`

Decompress gzip-compressed data.

**Arguments:** Bytes
**Returns:** Decompressed bytes

```fiddlerscript
let original = gzip_decompress(compressed);
let text = bytes_to_string(original);
```

### `zlib_compress(data)`

Compress data using zlib.

**Arguments:** String or bytes
**Returns:** Compressed bytes

```fiddlerscript
let compressed = zlib_compress("hello world");
```

### `zlib_decompress(data)`

Decompress zlib-compressed data.

**Arguments:** Bytes
**Returns:** Decompressed bytes

```fiddlerscript
let original = zlib_decompress(compressed);
```

### `deflate_compress(data)`

Compress data using raw deflate.

**Arguments:** String or bytes
**Returns:** Compressed bytes

```fiddlerscript
let compressed = deflate_compress("hello world");
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

## Summary Table

### Core Functions

| Function | Arguments | Returns | Description |
|----------|-----------|---------|-------------|
| `print(args...)` | Any values | `null` | Print to stdout |
| `len(value)` | String, bytes, array, or dict | Integer | Get length |
| `str(value)` | Any | String | Convert to string |
| `int(value)` | Any | Integer | Convert to integer |
| `bytes(value)` | Any | Bytes | Convert to bytes |
| `bytes_to_string(value)` | Bytes or string | String | Convert bytes to string |
| `getenv(name)` | String | String or null | Get environment variable |
| `parse_json(data)` | Bytes or string | Any | Parse JSON data |

### Collections

| Function | Arguments | Returns | Description |
|----------|-----------|---------|-------------|
| `array(args...)` | Any values | Array | Create array |
| `push(arr, val)` | Array, any | Array | Append to array |
| `get(coll, key)` | Collection, key | Any | Get by index/key |
| `set(coll, key, val)` | Collection, key, any | Collection | Set by index/key |
| `delete(coll, key)` | Collection, key | Collection | Remove by index/key |
| `dict()` | None | Dictionary | Create empty dict |
| `keys(dict)` | Dictionary | Array | Get dictionary keys |
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
| `gzip_compress(data)` | String or bytes | Bytes | Gzip compress |
| `gzip_decompress(data)` | Bytes | Bytes | Gzip decompress |
| `zlib_compress(data)` | String or bytes | Bytes | Zlib compress |
| `zlib_decompress(data)` | Bytes | Bytes | Zlib decompress |
| `deflate_compress(data)` | String or bytes | Bytes | Deflate compress |
| `deflate_decompress(data)` | Bytes | Bytes | Deflate decompress |

### Encoding

| Function | Arguments | Returns | Description |
|----------|-----------|---------|-------------|
| `base64_encode(data)` | String or bytes | String | Encode as base64 |
| `base64_decode(data)` | String or bytes | Bytes | Decode from base64 |
