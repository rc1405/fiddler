# aws_s3
Process data from a S3 Bucket

```yml
input:
    aws_s3:
        bucket: "my-bucket"
        prefix: "myprefix/"
        queue:
            queue_url: "https://some_queue_url"
            endpoint_url: "https://some_unique_endpoint"
            credentials:
                access_key_id: "AccessKey"
                secret_access_key: "SecretKey"
                session_token: "SessionToken"
            region: "us-west-2"
        credentials:
            access_key_id: "AccessKey"
            secret_access_key: "SecretKey"
            session_token: "SessionToken"
        delete_after_read: false
        endpoint_url: "https://some_unique_endpoint"
        read_lines: true
        force_path_style_urls: false
        region: us-east-1
```


## Metadata
All SQS Message attributes are inserted into metadata of the message

## Fields
### `bucket`
The S3 Bucket to process.  Note: if `queue` is provided, the bucket provided in the S3 Event Record will be used  
**Type:** `string`  
Required: `true`  

### `prefix`
The prefix of the objects from the s3 bucket to process.  Note: ignored if `queue` is used  
Type: `string`  
Required: `false`  

### `queue`
The [aws_sqs](./aws_sqs.md) object to utilize.  Expectes S3 Notifications directly to SQS
Type: `object`  
Required: `false`  
Properties:  
&nbsp;&nbsp;&nbsp;&nbsp;`queue_url`  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;AWS SQS Queue URL to utilize  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Type: `string`  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Required: `true`  

&nbsp;&nbsp;&nbsp;&nbsp;`endpoint_url`  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Custom AWS SQS Endpoint URL  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Type: `string`  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Required: `false`  

&nbsp;&nbsp;&nbsp;&nbsp;`credentials`  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;AWS Credentials Object to utilize.  If no credentials object is provided, fiddler will utilize standard SDK locations to pull in credentials.  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Type: `object`  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Properties:  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;`access_key_id`  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;AWS Access Key ID  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Type: `string`  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Required: `true`  
<br>
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;`secret_access_key`  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;AWS Secret Access Key  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Type: `string`  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Required: `true`  
<br>
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;`session_token`  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;AWS Session Token  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Type: `string`  
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Required: `false`  
<br>
&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Required: `false`

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

### `delete_after_read`  
Delete the object after it has been fully processed  
Type: `bolean`  
Default: `false`
Required: `false` 

### `force_path_style_urls`
Force Path Style S3 URLS  
Type: `boolean`  
Required: `false` 

### `region`
AWS Region of the bucket
Type: `string`  
Required: `false` 

## Credentials
Required IAM permissions to operate:  
- s3:GetObject  
- s3:ListBucket  

Required IAM permissions to operate if using the Queue:  
- sqs:ReceiveMessage  
- sqs:DeleteMessage  
- sqs:ChangeMessageVisibility  

Required IAM permissions to operate if using `delete_objects`  
- s3:DeleteObject  

If using KMS, the following kms permissions may be needed on the relvant keys:  
- kms:Decrypt