# aws_sqs
Receive messages from an AWS SQS Queue

=== "Required"
    ```yml
    input:
        aws_sqs:
            queue_url: "https://some_queue_url"
    ```

=== "Full"
    ```yml
    input:
        aws_sqs:
            queue_url: "https://some_queue_url"
            endpoint_url: "https://some_unique_endpoint"
            credentials:
                access_key_id: "AccessKey"
                secret_access_key: "SecretKey"
                session_token: "SessionToken"
            region: "us-west-2"
    ```


## Metadata
All SQS Message attributes are inserted into metadata of the message

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
- sqs:ReceiveMessage
- sqs:DeleteMessage
- sqs:ChangeMessageVisibility

## `retry`

Retry policy for failed reads. When present, the runtime retries failed reads with backoff.

Type: `object`
Required: `false`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `max_retries` | integer | 3 | Maximum retry attempts |
| `initial_wait` | string | "1s" | Wait before first retry |
| `max_wait` | string | "30s" | Maximum wait cap |
| `backoff` | string | "exponential" | Strategy: `constant`, `linear`, or `exponential` |

=== "With Retry"
    ```yml
    input:
      retry:
        max_retries: 3
        initial_wait: "1s"
        backoff: "exponential"
      aws_sqs:
        queue_url: "https://some_queue_url"
    ```