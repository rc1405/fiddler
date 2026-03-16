# stdout
Emit process messages to stdout

=== "Required"
    ```yml
    output:
        stdout: {}
    ```

=== "With Retry"
    ```yml
    output:
      retry:
        max_retries: 5
        initial_wait: "2s"
        backoff: "exponential"
      stdout: {}
    ```

## Fields

### `retry`

Retry policy for failed writes. When present, the runtime retries failed writes with backoff.

Type: `object`
Required: `false`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `max_retries` | integer | 3 | Maximum retry attempts |
| `initial_wait` | string | "1s" | Wait before first retry |
| `max_wait` | string | "30s" | Maximum wait cap |
| `backoff` | string | "exponential" | Strategy: `constant`, `linear`, or `exponential` |