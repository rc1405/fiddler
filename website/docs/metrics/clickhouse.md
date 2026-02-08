# clickhouse
Send pipeline metrics to ClickHouse for time-series storage and analysis.

!!! note "Feature Flag Required"
    This metrics backend requires the `clickhouse` feature to be enabled when building fiddler.
    ```bash
    cargo build --features clickhouse
    ```

=== "Basic"
    ```yml
    metrics:
      clickhouse:
        url: "http://localhost:8123"
    ```

=== "Full"
    ```yml
    metrics:
      interval: 60
      clickhouse:
        url: "http://localhost:8123"
        database: fiddler
        table: metrics
        username: default
        password: secret
        include:
          - total_received
          - total_completed
          - throughput_per_sec
        exclude:
          - stale_entries_removed
        dimensions:
          - name: environment
            value: production
          - name: pipeline
            value: main
        batch_size: 100
        flush_interval_ms: 5000
        create_table: true
        ttl_days: 30
    ```

## Fields

### `url`
ClickHouse HTTP interface URL.
Type: `string`
Required: `true`

### `database`
Target database name for metrics.
Type: `string`
Default: `"fiddler"`
Required: `false`

### `table`
Target table name for metrics.
Type: `string`
Default: `"metrics"`
Required: `false`

### `username`
Username for authentication.
Type: `string`
Required: `false`

### `password`
Password for authentication.
Type: `string`
Required: `false`

### `include`
List of metric names to include. If not specified, all metrics are included.
Type: `array[string]`
Required: `false`

### `exclude`
List of metric names to exclude. Applied after `include` filter.
Type: `array[string]`
Required: `false`

### `dimensions`
Additional dimension columns added to all metric rows. Useful for identifying pipelines or environments.
Type: `array`
Required: `false`

#### Dimension Object
| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Dimension column name |
| `value` | string | Constant value for all rows |

### `batch_size`
Number of metric entries to batch before sending to ClickHouse.
Type: `integer`
Default: `100`
Required: `false`

### `flush_interval_ms`
Maximum time in milliseconds between flushes.
Type: `integer`
Default: `5000`
Required: `false`

### `create_table`
Automatically create the metrics table if it doesn't exist.
Type: `boolean`
Default: `true`
Required: `false`

### `ttl_days`
Time-to-live for metrics in days. Set to `0` to disable automatic expiration.
Type: `integer`
Default: `0`
Required: `false`

## Available Metrics

The following metrics can be tracked:

| Metric Name | Type | Description |
|-------------|------|-------------|
| `total_received` | counter | Total messages received from input |
| `total_completed` | counter | Messages successfully processed |
| `total_filtered` | counter | Messages intentionally filtered/dropped by processors |
| `total_process_errors` | counter | Messages with processing errors |
| `total_output_errors` | counter | Messages with output errors |
| `streams_started` | counter | Streams initiated |
| `streams_completed` | counter | Streams finished |
| `duplicates_rejected` | counter | Duplicate messages rejected |
| `stale_entries_removed` | counter | Stale entries cleaned up |
| `in_flight` | gauge | Messages currently being processed |
| `throughput_per_sec` | gauge | Processing throughput (messages/sec) |
| `input_bytes` | counter | Total input bytes received |
| `output_bytes` | counter | Total output bytes written |
| `bytes_per_sec` | gauge | Throughput in bytes per second |
| `latency_avg_ms` | gauge | Average message processing latency in milliseconds |
| `latency_min_ms` | gauge | Minimum message processing latency in milliseconds |
| `latency_max_ms` | gauge | Maximum message processing latency in milliseconds |

## Table Schema

When `create_table: true`, the following table is created:

```sql
CREATE TABLE IF NOT EXISTS fiddler.metrics (
    timestamp DateTime64(3),
    metric_name String,
    metric_value Float64
    [, dimension1 String, dimension2 String, ...]
) ENGINE = MergeTree()
ORDER BY (timestamp, metric_name)
[TTL toDateTime(timestamp) + INTERVAL n DAY]
```

### With Dimensions Example
```sql
CREATE TABLE IF NOT EXISTS fiddler.metrics (
    timestamp DateTime64(3),
    metric_name String,
    metric_value Float64,
    environment String,
    pipeline String
) ENGINE = MergeTree()
ORDER BY (timestamp, metric_name)
TTL toDateTime(timestamp) + INTERVAL 30 DAY
```

## Metric Filtering

### Include Filter
Only track specific metrics:
```yml
metrics:
  clickhouse:
    url: "http://localhost:8123"
    include:
      - total_received
      - total_completed
      - throughput_per_sec
```

### Exclude Filter
Track all metrics except specific ones:
```yml
metrics:
  clickhouse:
    url: "http://localhost:8123"
    exclude:
      - stale_entries_removed
      - duplicates_rejected
```

### Combined Filters
Include is applied first, then exclude:
```yml
metrics:
  clickhouse:
    url: "http://localhost:8123"
    include:
      - total_received
      - total_completed
      - total_process_errors
      - total_output_errors
    exclude:
      - total_output_errors  # Will be excluded
```

## Querying Metrics

### Recent Throughput
```sql
SELECT
    toStartOfMinute(timestamp) AS minute,
    avg(metric_value) AS avg_throughput
FROM fiddler.metrics
WHERE metric_name = 'throughput_per_sec'
  AND timestamp > now() - INTERVAL 1 HOUR
GROUP BY minute
ORDER BY minute
```

### Error Rate Over Time
```sql
SELECT
    toStartOfHour(timestamp) AS hour,
    sumIf(metric_value, metric_name = 'total_process_errors') AS errors,
    sumIf(metric_value, metric_name = 'total_completed') AS completed,
    errors / (errors + completed) * 100 AS error_rate_pct
FROM fiddler.metrics
WHERE timestamp > now() - INTERVAL 24 HOUR
GROUP BY hour
ORDER BY hour
```

### By Pipeline (with dimensions)
```sql
SELECT
    pipeline,
    max(metric_value) AS max_throughput
FROM fiddler.metrics
WHERE metric_name = 'throughput_per_sec'
  AND timestamp > now() - INTERVAL 1 HOUR
GROUP BY pipeline
```

## Connection Settings

| Setting | Value |
|---------|-------|
| Max idle connections per host | 2 |
| Idle timeout | 90 seconds |
| Request timeout | 30 seconds |
| Connection timeout | 10 seconds |
| Channel buffer | 1000 entries |

## Full Pipeline Example

```yml
label: Production Pipeline
input:
  http_server:
    port: 8080
processors:
  - fiddlerscript:
      source: |
        root = this
output:
  clickhouse:
    url: "http://clickhouse:8123"
    database: events
    table: raw_events
metrics:
  interval: 30
  clickhouse:
    url: "http://clickhouse:8123"
    database: observability
    table: pipeline_metrics
    dimensions:
      - name: pipeline
        value: production_ingestion
      - name: environment
        value: prod
    include:
      - total_received
      - total_completed
      - throughput_per_sec
      - in_flight
    batch_size: 50
    flush_interval_ms: 3000
    ttl_days: 90
```

## Performance Considerations

### Buffer Overflow
If metrics are generated faster than they can be sent, a warning is logged:
```
WARN Metrics channel full, dropping metric. Consider increasing batch_size or reducing flush_interval_ms
```

**Solutions:**
- Increase `batch_size` for higher throughput
- Decrease `flush_interval_ms` for more frequent flushes
- Use `include` filter to reduce metric volume

### TTL for Storage Management
Use `ttl_days` to automatically expire old metrics:
```yml
metrics:
  clickhouse:
    url: "http://localhost:8123"
    ttl_days: 30  # Keep 30 days of metrics
```

### Graceful Shutdown
On shutdown, remaining buffered metrics are flushed with a 5-second timeout to ensure data is not lost.
