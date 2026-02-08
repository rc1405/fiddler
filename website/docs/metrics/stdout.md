# stdout
Output metrics as JSON to stdout for debugging and local development.

=== "Basic"
    ```yml
    metrics:
      stdout: {}
    ```

=== "Pretty Print"
    ```yml
    metrics:
      interval: 60
      stdout:
        pretty: true
    ```

## Fields

### `pretty`
Enable pretty-printed JSON output with indentation.
Type: `boolean`
Default: `false`
Required: `false`

## Output Format

### Standard Output (pretty: false)
```json
{"total_received":1000,"total_completed":995,"total_filtered":3,"total_process_errors":2,"total_output_errors":0,"streams_started":0,"streams_completed":0,"duplicates_rejected":0,"stale_entries_removed":0,"in_flight":5,"throughput_per_sec":123.45,"input_bytes":102400,"output_bytes":98304,"bytes_per_sec":9830.4,"latency_avg_ms":5.5,"latency_min_ms":1.2,"latency_max_ms":15.8}
```

### Pretty Output (pretty: true)
```json
{
  "total_received": 1000,
  "total_completed": 995,
  "total_filtered": 3,
  "total_process_errors": 2,
  "total_output_errors": 0,
  "streams_started": 0,
  "streams_completed": 0,
  "duplicates_rejected": 0,
  "stale_entries_removed": 0,
  "in_flight": 5,
  "throughput_per_sec": 123.45,
  "input_bytes": 102400,
  "output_bytes": 98304,
  "bytes_per_sec": 9830.4,
  "latency_avg_ms": 5.5,
  "latency_min_ms": 1.2,
  "latency_max_ms": 15.8
}
```

## Available Metrics

| Field | Type | Description |
|-------|------|-------------|
| `total_received` | integer | Total messages received from input |
| `total_completed` | integer | Messages successfully processed |
| `total_filtered` | integer | Messages intentionally filtered/dropped by processors |
| `total_process_errors` | integer | Messages with processing errors |
| `total_output_errors` | integer | Messages with output errors |
| `streams_started` | integer | Streams initiated |
| `streams_completed` | integer | Streams finished |
| `duplicates_rejected` | integer | Duplicate messages rejected |
| `stale_entries_removed` | integer | Stale entries cleaned up |
| `in_flight` | integer | Messages currently being processed |
| `throughput_per_sec` | float | Processing throughput (messages/sec) |
| `input_bytes` | integer | Total input bytes received |
| `output_bytes` | integer | Total output bytes written |
| `bytes_per_sec` | float | Throughput in bytes per second |
| `latency_avg_ms` | float | Average message processing latency in milliseconds |
| `latency_min_ms` | float | Minimum message processing latency in milliseconds |
| `latency_max_ms` | float | Maximum message processing latency in milliseconds |

## Use Cases

### Development and Debugging
Monitor pipeline behavior during development:
```yml
label: Debug Pipeline
input:
  file:
    filename: test_data.jsonl
    codec: Lines
processors:
  - fiddlerscript:
      source: |
        root = this
output:
  stdout: {}
metrics:
  interval: 5
  stdout:
    pretty: true
```

### Piping to Log Aggregator
Send metrics to an external system:
```bash
fiddler run config.yml 2>&1 | grep '"total_received"' | your-log-aggregator
```

### CI/CD Testing
Verify pipeline metrics in automated tests:
```bash
fiddler run config.yml 2>&1 | jq '.throughput_per_sec'
```

## Notes

- Metrics are written to stdout, interleaved with any `stdout` output module data
- For production use, consider `prometheus` or `clickhouse` metrics backends
- The `interval` setting in the parent `metrics` block controls how often metrics are printed (default: 300 seconds)
