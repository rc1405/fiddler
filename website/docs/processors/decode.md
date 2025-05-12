# decode
Decode the message

=== "Required"
    ```yml
    processors:
        - decode: {}
    ```

=== "Full"
    ```yml
    processors:
        - decode: 
            algorithm: Base64
    ```


## Fields
### `algorithm`
The decoding algorithm to use.  [Default: Base64]    
Type: `string`  
Required: `false`  
Accepted values:   
&nbsp;&nbsp;&nbsp;&nbsp;`Base64`: decode the message as base64  