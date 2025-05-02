# aws_sqs
Send messages to an AWS SQS Queue

```yml
output:
    aws_sqs:
        queue_url: "https://some_queue_url"
        endpoint_url: "https://some_unique_endpoint"
        credentials:
            access_key_id: "AccessKey"
            secret_access_key: "SecretKey"
            session_token: "SessionToken"
        region: "us-west-2"
```

## Fields
### `queue_url`
AWS SQS Queue URL to utilize  
Type: `string`  
Required: `true`  

### `endpoint_url`
Custom AWS SQS Endpoint URL  
Type: `string`  
Required: `false`  

### `credentials`
AWS Credentials Object to utilize.  If no credentials object is provided, fiddler will utilize standard SDK locations to pull in credentials.
Type: `object`  
Properties:  
&nbsp;&nbsp;&nbsp;&nbsp;`access_key_id`  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;AWS Access Key ID  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Type: `string`  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Required: `true`  
<br>
&nbsp;&nbsp;&nbsp;&nbsp;`secret_access_key`  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;AWS Secret Access Key  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Type: `string`  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Required: `true`  
<br>
&nbsp;&nbsp;&nbsp;&nbsp;`session_token`  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;AWS Session Token  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Type: `string`  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Required: `false`  
<br>
Required: `false`

## Credentials
Required IAM permissions to operate:  
- sqs:SendMessage