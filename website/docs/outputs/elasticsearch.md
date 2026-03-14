# elasticsearch
Send events to elasticsearch

=== "Required"
    ```yml
    output:
        elasticsearch:
            url: https://127.0.0.1:9200
            index: my_index
    ```

=== "Full"
    ```yml
    output:
        elasticsearch:
            url: https://127.0.0.1:9200
            username: elastic
            password: changeme
            cloud_id: someId
            index: my_index
    ```

=== "With TLS"
    ```yml
    output:
        elasticsearch:
            url: https://127.0.0.1:9200
            index: my_index
            tls:
              ca: /etc/ssl/ca.crt
              skip_verify: false
    ```
## Fields
### `url`
Elasticsearch URL to utilize  
Type: `string`  
Required: `true`  

### `username`
Elasticsearch username to use  
Type: `string`  
Required: `false`.  

### `password`
Password for the elasticsearch user    
Type: `string`  
Required: `false`  

### `cloud_id`
Elasticsearch CloudID  
Type: `string`  
Required: `false`.  

### `index`
Elasticsearch index to utilize.  default behavior is to append `YYYY-MM-DD` to the index name.  i.e. index of `index` would send to `index-YYYY-MM-DD` where the date chosen is the date of ingest.  
Type: `string`  
Required: `true`  

### `tls`
TLS configuration for custom CA certificates and client certificates.
Type: `object`
Required: `false`

Each string field (`ca`, `cert`, `key`) accepts either a **file path** or **inline PEM content**. If the value starts with `-----BEGIN`, it is treated as inline PEM.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `ca` | string | — | CA certificate for server verification |
| `cert` | string | — | Client certificate for mTLS |
| `key` | string | — | Client private key for mTLS |
| `skip_verify` | boolean | `false` | Skip server certificate verification |