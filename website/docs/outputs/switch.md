# switch
Switch accepts an array of valid outputs and is expected to be utlized with the `check` output.  Switch will attempt each provided output; while failing to the next provided the error is returned is `ConditionalCheckfailed`.  Other encountered errors will be surfaced.

=== "Required"
  ```yml
  output:
    switch: []
  ```

## `check`
=== "Required"
  ```yml
  output:
    switch:
      - check:
          condition: '\"Hello World\" > `5`'
          output: 
            stdout: {}"
  ```

### Fields
#### `condition`
Condition utilized for the execution of the output step utilizing [jmespath](https://jmespath.org/specification.html) syntax for evaluation.  As such, this can only be utilized with JSON documents.  
Type: `string`  
Required: `true`  

#### `output`
Valid fiddler output module
Type: `object`
Required: `true`

## `retry`

Retry policy for failed writes. When present, the runtime retries failed writes with backoff.

Type: `object`
Required: `false`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `max_retries` | integer | 3 | Maximum retry attempts |
| `initial_wait` | string | "1s" | Wait before first retry |
| `max_wait` | string | "30s" | Maximum wait cap |
| `backoff` | string | "exponential" | Strategy: `constant`, `linear`, or `exponential` |  