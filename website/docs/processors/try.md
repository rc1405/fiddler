# try
Attempt to run a given processor and either continue processing or try alternative processors upon failure

=== "Required"
    ```yml
    processors:
        - try: 
            processor: 
              noop: {}
    ```

=== "Full"
    ```yml
    processors:
        - try: 
            processor: 
              noop: {}
            catch:
              - noop: {}
    ```


## Fields
### `processor`
The fiddler processor to use    
Type: `object`  
Required: `true`  

### `catch`
An array of fiddler processors to run if the initial processor fails    
Type: `array`  
Required: `false`  