# decompress
Decompress the message being processed

=== "Required"
    ```yml
    processors:
        - decompress: {}
    ```

=== "Full"
    ```yml
    processors:
        - decompress: 
            algorithm: Gzip
    ```

## Fields
### `algorithm`
The compression algorithm to use.  [Default: Gzip]    
Type: `string`  
Required: `false`  
Accepted values:   
&nbsp;&nbsp;&nbsp;&nbsp;`Gzip`: decode the message as base64  
&nbsp;&nbsp;&nbsp;&nbsp;`Zip`: decode the message as base64  
&nbsp;&nbsp;&nbsp;&nbsp;`Zlib`: decode the message as base64  