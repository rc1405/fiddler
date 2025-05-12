# compress
Compress the message being processed

=== "Required"
    ```yml
    processors:
        - compress: {}
    ```

=== "Full"
    ```yml
    processors:
        - compress: 
            algorithm: Gzip
    ```

## Fields
### `algorithm`
The compression algorithm to use.  [Default: Gzip]    
Type: `string`  
Required: `false`  
Accepted values:   
&nbsp;&nbsp;&nbsp;&nbsp;`Gzip`: compress the message using gzip.  
&nbsp;&nbsp;&nbsp;&nbsp;`Zip`: compress the message using deflate  
&nbsp;&nbsp;&nbsp;&nbsp;`Zlib`: compress the message using zlib  