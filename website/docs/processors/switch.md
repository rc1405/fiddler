# switch
Switch accepts an array of valid processors and is expected to be utlized with the `check` output.  Switch will attempt each provided processor; while failing to the next provided the error is returned is `ConditionalCheckfailed`.  Other encountered errors will be surfaced.

=== "Required"
  ```yml
  processors:
    - switch: []
  ```

## `check`
=== "Required"
  ```yml
  processors:
    - switch:
        - check:
            condition: '\"Hello World\" > `5`'
            processors: 
              - noop: {}"
  ```

### Fields
#### `condition`
Condition utilized for the execution of the processing step utilizing [jmespath](https://jmespath.org/specification.html) syntax for evaluation.  As such, this can only be utilized with JSON documents.  
Type: `string`  
Required: `true`  

#### `output`
Valid fiddler output module  
Type: `object`  
Required: `true`  