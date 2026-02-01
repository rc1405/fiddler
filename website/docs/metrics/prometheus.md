# prometheus
Expose metrics via a Prometheus-compatible endpoint using the `metrics` crate.

!!! note "Feature Flag Required"
    This metrics backend requires the `prometheus` feature to be enabled when building fiddler.
    ```bash
    cargo build --features prometheus
    ```

=== "Basic"
    ```yml
    metrics:
      prometheus: {}
    ```

=== "Full Example"
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

## Exposed Metrics

The following Prometheus metrics are exposed:

### Counters
| Metric Name | Description |
|-------------|-------------|
| `fiddler_messages_received_total` | Total messages received from input |
| `fiddler_messages_completed_total` | Messages successfully processed through all outputs |
| `fiddler_messages_process_errors_total` | Messages that encountered processing errors |
| `fiddler_messages_output_errors_total` | Messages that encountered output errors |
| `fiddler_streams_started_total` | Number of streams started |
| `fiddler_streams_completed_total` | Number of streams completed |
| `fiddler_duplicates_rejected_total` | Duplicate messages rejected |
| `fiddler_stale_entries_removed_total` | Stale entries cleaned up |
| `fiddler_input_bytes_total` | Total bytes received from input |
| `fiddler_output_bytes_total` | Total bytes written to output |

### Gauges
| Metric Name | Description |
|-------------|-------------|
| `fiddler_messages_in_flight` | Current number of messages being processed |
| `fiddler_throughput_per_second` | Current throughput in messages per second |
| `fiddler_bytes_per_second` | Current throughput in bytes per second |
| `fiddler_latency_avg_ms` | Average message processing latency in milliseconds |
| `fiddler_latency_min_ms` | Minimum message processing latency in milliseconds |
| `fiddler_latency_max_ms` | Maximum message processing latency in milliseconds |

## Scraping Metrics

The Prometheus exporter uses the default configuration from `metrics-exporter-prometheus`. Metrics are typically available at `http://localhost:9000/metrics` (default port).

### Example Prometheus Configuration

```yml
scrape_configs:
  - job_name: 'fiddler'
    static_configs:
      - targets: ['localhost:9000']
```

## Fields
Currently, the prometheus backend accepts an empty configuration object. Future versions may support additional configuration options such as:

- Custom port binding
- Custom endpoint path
- Default labels

### Example Future Configuration (Not Yet Implemented)
```yml
metrics:
  prometheus:
    port: 9090
    path: /metrics
    labels:
      environment: production
      service: my-pipeline
```
