# stdin
Read data from stdin by lines and forward to the pipeline

=== "Required"
    ```yml
    input:
        stdin: {}
    ```

=== "With Retry"
    ```yml
    input:
      retry:
        max_retries: 3
        initial_wait: "1s"
        backoff: "exponential"
      stdin: {}
    ```

## Fields

### `retry`

Retry policy for failed reads. When present, the runtime retries failed reads with backoff.

Type: `object`
Required: `false`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `max_retries` | integer | 3 | Maximum retry attempts |
| `initial_wait` | string | "1s" | Wait before first retry |
| `max_wait` | string | "30s" | Maximum wait cap |
| `backoff` | string | "exponential" | Strategy: `constant`, `linear`, or `exponential` |