# Metrics
Metrics provide observability into the fiddler runtime by exposing statistics about message processing. Metrics configuration is optional - if not provided, metrics are disabled with zero overhead.

```yml
metrics:
  prometheus: {}
```

## Available Metrics
The following metrics are tracked and exposed by all metrics backends:

| Metric | Type | Description |
|--------|------|-------------|
| `total_received` | Counter | Total messages received from input |
| `total_completed` | Counter | Messages successfully processed through all outputs |
| `total_process_errors` | Counter | Messages that encountered processing errors |
| `total_output_errors` | Counter | Messages that encountered output errors |
| `streams_started` | Counter | Number of streams started |
| `streams_completed` | Counter | Number of streams completed |
| `duplicates_rejected` | Counter | Duplicate messages rejected |
| `stale_entries_removed` | Counter | Stale entries cleaned up from state tracker |
| `in_flight` | Gauge | Current number of messages being processed |
| `throughput_per_sec` | Gauge | Current throughput in messages per second |

## Configuration
Metrics are configured at the top level of the pipeline configuration:

```yml
label: My Pipeline
metrics:
  prometheus: {}
input:
  stdin: {}
processors:
  - noop: {}
output:
  stdout: {}
```

## Available Backends
- [prometheus](./prometheus.md) - Prometheus-compatible metrics endpoint (requires `prometheus` feature)
