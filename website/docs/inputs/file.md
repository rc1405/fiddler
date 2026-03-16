# file
Read file off of the local filesystem

=== "Required"
    ```yml
    input:
        file:
            filename: path_to_file
    ```

=== "Full"
    ```yml
    input:
        file:
            filename: path_to_file
            codec: Tail
            position_filename: path_to_position_file
    ```

## Fields
### `filename`
Path to the file to consume  
Type: `string`  
Required: `true`  

### `codec`
Enum to outline the type of file reader to implement  
Type: `string`  
Accepted values:   
&nbsp;&nbsp;&nbsp;&nbsp;`Lines`: Read the file line by line [default]  
&nbsp;&nbsp;&nbsp;&nbsp;`ToEnd`: Read the file in its entirity  
&nbsp;&nbsp;&nbsp;&nbsp;`Tail`: Read the file line by line, waiting for new data to be written  

### `position_filename`
Filename to track the position of tailed files
Type: `string`
Required: with `codec`: `Tail`

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

=== "With Retry"
    ```yml
    input:
      retry:
        max_retries: 3
        initial_wait: "1s"
        backoff: "exponential"
      file:
        filename: path_to_file
    ```