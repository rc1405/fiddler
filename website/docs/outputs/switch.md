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