# clickhouse
Send events to ClickHouse database using batch inserts.

!!! note "Feature Flag Required"
    This output requires the `clickhouse` feature to be enabled when building fiddler.
    ```bash
    cargo build --features clickhouse
    ```

=== "Basic"
    ```yml
    output:
      clickhouse:
        url: "http://localhost:8123"
        table: events
    ```

=== "Full"
    ```yml
    output:
      clickhouse:
        url: "http://localhost:8123"
        database: default
        table: events
        username: default
        password: secret
        batch:
          size: 1000
          duration: 10s
        create_table: true
        columns:
          - name: timestamp
            type: DateTime64(3)
          - name: event_type
            type: String
          - name: data
            type: String
    ```

## Fields

### `url`
ClickHouse HTTP interface URL.
Type: `string`
Required: `true`

### `database`
Target database name.
Type: `string`
Default: `"default"`
Required: `false`

### `table`
Target table name.
Type: `string`
Required: `true`

### `username`
Username for authentication.
Type: `string`
Required: `false`

### `password`
Password for authentication.
Type: `string`
Required: `false`

### `batch`
Batching policy for writes.
Type: `object`
Required: `false`

#### `batch.size`
Maximum number of messages per batch.
Type: `integer`
Default: `500`

#### `batch.duration`
Maximum time to wait before flushing a batch.
Type: `duration`
Default: `10s`

### `create_table`
Automatically create the table if it doesn't exist. Requires `columns` to be defined.
Type: `boolean`
Default: `false`
Required: `false`

### `columns`
Column definitions for automatic table creation.
Type: `array`
Required: `false` (required if `create_table: true`)

#### Column Object
| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Column name |
| `type` | string | ClickHouse data type (e.g., `String`, `DateTime64(3)`, `UInt64`) |

## Message Format

Messages must be valid JSON objects. Each JSON object becomes one row in ClickHouse. JSON keys must match the column names in your table.

**Example Message:**
```json
{"timestamp": "2024-01-15T10:30:00Z", "event_type": "click", "user_id": 123}
```

## Table Creation

When `create_table: true`, fiddler creates a table with the MergeTree engine:

```sql
CREATE TABLE IF NOT EXISTS database.table (
    timestamp DateTime64(3),
    event_type String,
    data String
) ENGINE = MergeTree()
ORDER BY timestamp
```

!!! warning "Column Order"
    The `ORDER BY` clause uses the first column defined. Ensure your first column is suitable for ordering (typically a timestamp).

## Connection Settings

The ClickHouse output uses the following HTTP client settings:

| Setting | Value |
|---------|-------|
| Max idle connections per host | 2 |
| Idle timeout | 90 seconds |
| Request timeout | 60 seconds |
| Connection timeout | 10 seconds |

## Security

### SQL Injection Prevention
All identifiers (database, table, column names) are validated to prevent SQL injection:

- Must start with a letter or underscore
- Can only contain alphanumeric characters and underscores
- Cannot be empty

### Authentication
Supports HTTP Basic Authentication when `username` and `password` are provided.

## Full Pipeline Example

```yml
label: Events to ClickHouse
input:
  http_server:
    port: 8080
    path: "/events"
processors:
  - fiddlerscript:
      source: |
        root.timestamp = timestamp()
        root.event_type = this.type
        root.payload = encode_json(this)
output:
  clickhouse:
    url: "http://clickhouse:8123"
    database: analytics
    table: events
    username: fiddler
    password: "${CLICKHOUSE_PASSWORD}"
    batch:
      size: 1000
      duration: 5s
    create_table: true
    columns:
      - name: timestamp
        type: DateTime64(3)
      - name: event_type
        type: String
      - name: payload
        type: String
```

## Performance Tuning

### Batch Size
Larger batch sizes improve throughput but increase memory usage and latency:

| Batch Size | Use Case |
|------------|----------|
| 100-500 | Low-latency requirements |
| 500-1000 | Balanced (default) |
| 1000-5000 | High-throughput bulk loading |

### Batch Duration
Controls maximum wait time before flushing:

- Lower values (1-5s): Better for real-time data
- Higher values (10-30s): Better for batch processing
