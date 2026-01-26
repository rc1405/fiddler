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

## Summary Table

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
| `array(args...)` | Any values | Array | Create array |
| `push(arr, val)` | Array, any | Array | Append to array |
| `get(coll, key)` | Collection, key | Any | Get by index/key |
| `set(coll, key, val)` | Collection, key, any | Collection | Set by index/key |
| `dict()` | None | Dictionary | Create empty dict |
| `keys(dict)` | Dictionary | Array | Get dictionary keys |
| `is_array(val)` | Any | Boolean | Check if array |
| `is_dict(val)` | Any | Boolean | Check if dictionary |
