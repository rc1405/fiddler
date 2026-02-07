# filter

Filter messages based on a JMESPath condition evaluated against JSON data. Messages that match the condition pass through; messages that don't match are dropped from the pipeline.

=== "Basic"
`yml
    processors:
      - filter:
          condition: "status == 'active'"
    `

=== "Numeric Comparison"
``yml
    processors:
      - filter:
          condition: "age >= `18`"
    ``

=== "Complex Condition"
``yml
    processors:
      - filter:
          condition: "type == 'order' && total > `100` && status != 'cancelled'"
    ``

## Fields

### `condition`

A JMESPath expression that must evaluate to a boolean value. Messages where the condition evaluates to `true` pass through; messages where it evaluates to `false` are dropped.

Type: `string`
Required: `true`

### `label`

Optional label for identifying this processor in logs and metrics.

Type: `string`
Required: `false`

## How It Works

1. The processor parses the message bytes as JSON
2. The JMESPath condition is evaluated against the JSON data
3. If the result is `true`, the message continues through the pipeline
4. If the result is `false`, the message is dropped (marked as a conditional check failure)
5. If the condition doesn't return a boolean, a processing error occurs

## JMESPath Expressions

The filter processor uses [JMESPath](https://jmespath.org/) syntax for conditions. JMESPath is a query language for JSON that supports:

- Field access: `status`, `user.name`, `items[0]`
- Comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`
- Logical operators: `&&`, `||`, `!`
- Built-in functions: `length()`, `contains()`, `starts_with()`, `ends_with()`, etc.

### Literal Values

In JMESPath, literal values are enclosed in backticks:

- Numbers: `` `100` ``, `` `3.14` ``
- Booleans: `` `true` ``, `` `false` ``
- Strings can use single quotes: `'active'`
- Null: `` `null` ``

## Examples

### Filter by Field Value

Keep only messages with a specific status:

```yml
processors:
  - filter:
      condition: "status == 'active'"
```

### Filter by Numeric Range

Keep messages where the value is within a range:

```yml
processors:
  - filter:
      condition: "price >= `10` && price <= `100`"
```

### Filter by Nested Field

Access nested JSON fields:

```yml
processors:
  - filter:
      condition: "user.verified == `true`"
```

### Filter by Array Contents

Check if an array contains a specific value:

```yml
processors:
  - filter:
      condition: "contains(tags, 'important')"
```

### Filter by Array Length

Keep messages with non-empty arrays:

```yml
processors:
  - filter:
      condition: "length(items) > `0`"
```

### Filter by Null Check

Keep messages where a field is not null:

```yml
processors:
  - filter:
      condition: "error != null"
```

Or keep messages where a field is null:

```yml
processors:
  - filter:
      condition: "error == null"
```

### Filter by String Prefix

Keep messages where a field starts with a prefix:

```yml
processors:
  - filter:
      condition: "starts_with(name, 'prod-')"
```

### Complex Multi-Condition Filter

Combine multiple conditions:

```yml
processors:
  - filter:
      condition: "type == 'order' && total > `100` && status != 'cancelled'"
```

### Filter Log Levels

Keep only error and warning logs:

```yml
processors:
  - filter:
      condition: "level == 'error' || level == 'warning'"
```

### Filter by Type Check

Keep messages that have a specific field type:

```yml
processors:
  - filter:
      condition: "type(data) == 'object'"
```

## Error Handling

### Invalid JSON

If the message bytes are not valid JSON, a processing error is returned and the message is marked as failed.

### Non-Boolean Result

If the JMESPath expression doesn't return a boolean value, a processing error is returned:

```yml
# This will error because 'name' returns a string, not a boolean
processors:
  - filter:
      condition: "name" # Wrong - returns string value
```

Correct usage:

```yml
processors:
  - filter:
      condition: "name != null" # Correct - returns boolean
```

### Invalid JMESPath Expression

If the JMESPath expression is invalid, an error is returned during configuration validation.

## Common JMESPath Functions

| Function                   | Description                                      | Example                       |
| -------------------------- | ------------------------------------------------ | ----------------------------- |
| `length(expr)`             | Returns the length of a string, array, or object | `length(items) > `0``         |
| `contains(arr, val)`       | Check if array contains value                    | `contains(tags, 'important')` |
| `starts_with(str, prefix)` | Check if string starts with prefix               | `starts_with(name, 'prod-')`  |
| `ends_with(str, suffix)`   | Check if string ends with suffix                 | `ends_with(file, '.json')`    |
| `type(expr)`               | Returns the type of a value                      | `type(data) == 'array'`       |
| `not_null(expr...)`        | Returns first non-null value                     | `not_null(name, 'unknown')`   |
| `to_string(expr)`          | Convert to string                                | -                             |
| `to_number(expr)`          | Convert to number                                | -                             |

For the complete list of JMESPath functions, see the [JMESPath specification](https://jmespath.org/specification.html#built-in-functions).

## Comparison with Other Filtering Methods

| Method                      | Use Case                                                         |
| --------------------------- | ---------------------------------------------------------------- |
| `filter` processor          | Simple boolean conditions on JSON fields                         |
| `switch` processor          | Route messages to different processing paths based on conditions |
| FiddlerScript `this = null` | Complex filtering logic with full scripting capabilities         |

## Full Pipeline Example

```yml
label: Order Processing Pipeline
input:
  http_server:
    port: 8080
processors:
  # Only process valid orders
  - filter:
      condition: "type == 'order' && items != null && length(items) > `0`"
  # Additional processing...
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        let total = jmespath(data, "sum(items[*].price)");
        // ... process order
output:
  stdout: {}
```
