# transform

Transform JSON messages by extracting fields using JMESPath expressions and mapping them to new field names. This processor creates a completely new JSON structure containing only the mapped fields.

=== "Basic"
    ```yml
    processors:
      - transform:
          mappings:
            - source: "name"
              target: "user_name"
            - source: "email"
              target: "user_email"
    ```

=== "Nested Extraction"
    ```yml
    processors:
      - transform:
          mappings:
            - source: "user.profile.email"
              target: "email"
            - source: "user.name"
              target: "name"
    ```

=== "Array Projection"
    ```yml
    processors:
      - transform:
          mappings:
            - source: "users[*].name"
              target: "all_names"
    ```

## Fields

### `mappings`

An array of source-to-target field mappings. Each mapping extracts a value using a JMESPath expression and assigns it to a new field name.

Type: `array`
Required: `true`

#### Mapping Object

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `source` | string | Yes | JMESPath expression to extract the value |
| `target` | string | Yes | Field name in the output JSON |

### `label`

Optional label for identifying this processor in logs and metrics.

Type: `string`
Required: `false`

## How It Works

1. The processor parses the input message bytes as JSON
2. For each mapping, the JMESPath `source` expression is evaluated against the input
3. The extracted value is assigned to the `target` field name
4. A new JSON object is created containing only the mapped fields
5. The original message structure is completely replaced

**Important**: The transform processor creates an entirely new JSON object. Fields not explicitly mapped are discarded.

## JMESPath Expressions

The transform processor uses [JMESPath](https://jmespath.org/) syntax for source expressions. This enables powerful data extraction and transformation capabilities.

### Field Access

```yml
mappings:
  - source: "name"           # Top-level field
    target: "user_name"
  - source: "user.email"     # Nested field
    target: "email"
  - source: "items[0]"       # Array index
    target: "first_item"
  - source: "items[-1]"      # Negative index (last element)
    target: "last_item"
```

### Array Projections

Extract values from all elements in an array:

```yml
mappings:
  - source: "users[*].name"           # All names
    target: "names"
  - source: "orders[*].total"         # All totals
    target: "order_totals"
```

### Filtering

Filter array elements based on conditions:

```yml
mappings:
  - source: "items[?price > `100`]"           # Items over $100
    target: "expensive_items"
  - source: "users[?active == `true`].name"   # Active user names
    target: "active_users"
```

### Built-in Functions

Use JMESPath functions for transformations:

```yml
mappings:
  - source: "length(items)"           # Count items
    target: "item_count"
  - source: "join(', ', tags)"        # Join array to string
    target: "tags_string"
  - source: "sort(scores)"            # Sort array
    target: "sorted_scores"
```

### Multiselect

Create nested objects in a single mapping:

```yml
mappings:
  - source: "{full_name: name, user_email: email}"
    target: "contact"
```

This produces:
```json
{
  "contact": {
    "full_name": "Alice",
    "user_email": "alice@example.com"
  }
}
```

## Examples

### Rename Fields

Rename fields while keeping the same values:

```yml
processors:
  - transform:
      mappings:
        - source: "firstName"
          target: "first_name"
        - source: "lastName"
          target: "last_name"
        - source: "emailAddress"
          target: "email"
```

Input:
```json
{"firstName": "Alice", "lastName": "Smith", "emailAddress": "alice@example.com"}
```

Output:
```json
{"first_name": "Alice", "last_name": "Smith", "email": "alice@example.com"}
```

### Flatten Nested Structure

Extract nested fields to a flat structure:

```yml
processors:
  - transform:
      mappings:
        - source: "user.profile.name"
          target: "name"
        - source: "user.profile.contact.email"
          target: "email"
        - source: "user.settings.theme"
          target: "theme"
```

Input:
```json
{
  "user": {
    "profile": {
      "name": "Bob",
      "contact": {"email": "bob@example.com"}
    },
    "settings": {"theme": "dark"}
  }
}
```

Output:
```json
{"name": "Bob", "email": "bob@example.com", "theme": "dark"}
```

### Extract Array Elements

Extract specific elements from arrays:

```yml
processors:
  - transform:
      mappings:
        - source: "items[0]"
          target: "first"
        - source: "items[-1]"
          target: "last"
        - source: "items[1:3]"
          target: "middle"
```

### Project Array Fields

Extract a specific field from all objects in an array:

```yml
processors:
  - transform:
      mappings:
        - source: "orders[*].id"
          target: "order_ids"
        - source: "orders[*].total"
          target: "totals"
```

Input:
```json
{
  "orders": [
    {"id": "A1", "total": 100},
    {"id": "A2", "total": 200}
  ]
}
```

Output:
```json
{"order_ids": ["A1", "A2"], "totals": [100, 200]}
```

### Compute Aggregates

Use JMESPath functions for calculations:

```yml
processors:
  - transform:
      mappings:
        - source: "length(items)"
          target: "count"
        - source: "sum(items[*].price)"
          target: "total_price"
        - source: "max(items[*].price)"
          target: "max_price"
        - source: "min(items[*].price)"
          target: "min_price"
```

### Filter and Transform

Filter array elements and extract specific fields:

```yml
processors:
  - transform:
      mappings:
        - source: "products[?inStock == `true`].name"
          target: "available_products"
        - source: "products[?price < `50`] | length(@)"
          target: "affordable_count"
```

### Create Nested Output

Build nested structures using multiselect:

```yml
processors:
  - transform:
      mappings:
        - source: "{name: user.name, email: user.email}"
          target: "contact"
        - source: "{city: address.city, country: address.country}"
          target: "location"
```

Output:
```json
{
  "contact": {"name": "Alice", "email": "alice@example.com"},
  "location": {"city": "NYC", "country": "USA"}
}
```

### Handle Missing Fields

Missing fields result in `null` values:

```yml
processors:
  - transform:
      mappings:
        - source: "required_field"
          target: "required"
        - source: "optional_field"
          target: "optional"
```

If `optional_field` doesn't exist, the output will be:
```json
{"required": "value", "optional": null}
```

## Error Handling

### Invalid JSON

If the message bytes are not valid JSON, a processing error is returned.

### Invalid JMESPath Expression

Invalid JMESPath expressions are caught during configuration validation, preventing startup with malformed expressions.

## Common JMESPath Functions

| Function | Description | Example |
|----------|-------------|---------|
| `length(expr)` | Array/string/object length | `length(items)` |
| `sum(array)` | Sum of numbers | `sum(prices)` |
| `avg(array)` | Average of numbers | `avg(scores)` |
| `min(array)` | Minimum value | `min(prices)` |
| `max(array)` | Maximum value | `max(prices)` |
| `sort(array)` | Sort array | `sort(names)` |
| `reverse(array)` | Reverse array | `reverse(items)` |
| `join(sep, array)` | Join strings | `join(', ', tags)` |
| `keys(object)` | Get object keys | `keys(data)` |
| `values(object)` | Get object values | `values(data)` |
| `contains(arr, val)` | Check membership | `contains(tags, 'urgent')` |
| `starts_with(str, prefix)` | String prefix check | `starts_with(name, 'prod')` |
| `ends_with(str, suffix)` | String suffix check | `ends_with(file, '.json')` |
| `to_string(expr)` | Convert to string | `to_string(count)` |
| `to_number(expr)` | Convert to number | `to_number(value)` |

For the complete list, see the [JMESPath specification](https://jmespath.org/specification.html#built-in-functions).

## Comparison with Other Processors

| Processor | Use Case |
|-----------|----------|
| `transform` | Restructure JSON with field extraction and renaming |
| `filter` | Keep or drop entire messages based on conditions |
| `fiddlerscript` | Complex transformations with full scripting |

## Preserving Original Fields

The transform processor creates a new structure, discarding unmapped fields. To keep original fields alongside new ones, use the `fiddlerscript` processor instead:

```yml
processors:
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        // Add new field while keeping original
        data = set(data, "full_name", get(data, "first") + " " + get(data, "last"));
        this = bytes(str(data));
```

## Full Pipeline Example

```yml
label: Order Processing
input:
  http_server:
    port: 8080
processors:
  # Extract and restructure order data
  - transform:
      mappings:
        - source: "order.id"
          target: "order_id"
        - source: "order.items[*].sku"
          target: "skus"
        - source: "sum(order.items[*].price)"
          target: "total"
        - source: "customer.email"
          target: "email"
        - source: "length(order.items)"
          target: "item_count"
output:
  stdout: {}
```

Input:
```json
{
  "order": {
    "id": "ORD-123",
    "items": [
      {"sku": "PROD-A", "price": 29.99},
      {"sku": "PROD-B", "price": 49.99}
    ]
  },
  "customer": {
    "name": "Alice",
    "email": "alice@example.com"
  }
}
```

Output:
```json
{
  "order_id": "ORD-123",
  "skus": ["PROD-A", "PROD-B"],
  "total": 79.98,
  "email": "alice@example.com",
  "item_count": 2
}
```
