# elasticsearch
Receive messages from an AWS SQS Queue

```yml
output:
    elasticsearch:
        url: https://127.0.0.1:9200
        username: elastic
        password: changeme
        cloud_id: someId
        index: my_index
        cert_validation: Default
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

### `cert_validation`
Enum to identify which type of certification validation is utilized  
Type: `string`  
AcceptedValues:  
&nbsp;&nbsp;&nbsp;&nbsp;`Default`: default mechanisms utilized by elasticsearch's SDK [default]  
&nbsp;&nbsp;&nbsp;&nbsp;`None`: Do not perform certification validation    
Required: `false`  