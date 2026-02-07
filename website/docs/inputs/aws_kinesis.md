# aws_kinesis

Consume records from an AWS Kinesis Data Stream. This input reads from a specific shard and processes records as they arrive.

=== "Basic"
    ```yml
    input:
      aws_kinesis:
        stream_name: "my-stream"
    ```

=== "With Shard"
    ```yml
    input:
      aws_kinesis:
        stream_name: "events"
        shard_id: "shardId-000000000000"
        shard_iterator_type: "TRIM_HORIZON"
    ```

=== "With Credentials"
    ```yml
    input:
      aws_kinesis:
        stream_name: "my-stream"
        region: "us-east-1"
        credentials:
          access_key_id: "${AWS_ACCESS_KEY_ID}"
          secret_access_key: "${AWS_SECRET_ACCESS_KEY}"
    ```

=== "LocalStack"
    ```yml
    input:
      aws_kinesis:
        stream_name: "local-stream"
        region: "us-east-1"
        endpoint_url: "http://localhost:4566"
    ```

## Fields

### `stream_name`

The name of the Kinesis stream to consume from.

Type: `string`
Required: `true`

### `shard_id`

The specific shard ID to consume from.

Type: `string`
Required: `false`

If not specified, the input automatically discovers and uses the first available shard.

### `shard_iterator_type`

Determines where in the stream to start reading.

Type: `string`
Required: `false`
Default: `"LATEST"`

| Value | Description |
|-------|-------------|
| `LATEST` | Start with the most recent records |
| `TRIM_HORIZON` | Start from the oldest available records |
| `AT_TIMESTAMP` | Start from a specific timestamp |

### `batch_size`

Maximum number of records to retrieve per GetRecords API call.

Type: `integer`
Required: `false`
Default: `100`
Maximum: `10000`

### `region`

AWS region where the stream is located.

Type: `string`
Required: `false`

Uses the AWS SDK's default region resolution if not specified (environment variables, config files, instance metadata).

### `endpoint_url`

Custom endpoint URL for the Kinesis API.

Type: `string`
Required: `false`

Useful for local development with LocalStack or other Kinesis-compatible services.

### `credentials`

Explicit AWS credentials. If not provided, uses the standard AWS credential chain.

Type: `object`
Required: `false`

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `access_key_id` | string | Yes | AWS access key ID |
| `secret_access_key` | string | Yes | AWS secret access key |
| `session_token` | string | No | AWS session token (for temporary credentials) |

## How It Works

1. The input connects to AWS Kinesis using SDK credentials
2. If no shard ID is specified, it discovers available shards and uses the first one
3. A shard iterator is obtained based on `shard_iterator_type`
4. Records are polled in batches using GetRecords
5. When caught up with the stream, polling continues with brief pauses

## AWS Authentication

The Kinesis input supports two authentication methods:

### Explicit Credentials

```yml
input:
  aws_kinesis:
    stream_name: "events"
    credentials:
      access_key_id: "${AWS_ACCESS_KEY_ID}"
      secret_access_key: "${AWS_SECRET_ACCESS_KEY}"
```

### AWS SDK Credential Chain (default)

When no credentials are specified, the standard AWS credential chain is used:

1. Environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`)
2. Shared credentials file (`~/.aws/credentials`)
3. IAM role for EC2/ECS/Lambda
4. Web identity token (EKS)

### Required IAM Permissions

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "kinesis:GetShardIterator",
        "kinesis:GetRecords",
        "kinesis:ListShards",
        "kinesis:DescribeStream"
      ],
      "Resource": "arn:aws:kinesis:*:*:stream/my-stream"
    }
  ]
}
```

## Examples

### Basic Stream Processing

```yml
input:
  aws_kinesis:
    stream_name: "events"
    shard_iterator_type: "LATEST"

processors:
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        data = set(data, "processed_at", now());
        this = bytes(str(data));

output:
  stdout: {}
```

### Process Historical Data

```yml
input:
  aws_kinesis:
    stream_name: "events"
    shard_iterator_type: "TRIM_HORIZON"
    batch_size: 500

output:
  clickhouse:
    url: "http://localhost:8123"
    table: "events"
    batch:
      size: 1000
      duration: "10s"
```

### Multi-Shard Processing

For streams with multiple shards, run separate Fiddler instances:

```yml
# Instance 1
input:
  aws_kinesis:
    stream_name: "high-volume-stream"
    shard_id: "shardId-000000000000"
```

```yml
# Instance 2
input:
  aws_kinesis:
    stream_name: "high-volume-stream"
    shard_id: "shardId-000000000001"
```

### Local Development with LocalStack

```yml
input:
  aws_kinesis:
    stream_name: "test-stream"
    region: "us-east-1"
    endpoint_url: "http://localhost:4566"
    shard_iterator_type: "TRIM_HORIZON"

output:
  stdout: {}
```

### Cross-Account Access

```yml
input:
  aws_kinesis:
    stream_name: "events"
    region: "us-west-2"
    credentials:
      access_key_id: "${CROSS_ACCOUNT_ACCESS_KEY}"
      secret_access_key: "${CROSS_ACCOUNT_SECRET_KEY}"
```

## Iterator Types

| Type | Use Case |
|------|----------|
| `LATEST` | Real-time processing of new data only |
| `TRIM_HORIZON` | Processing historical data, reprocessing |
| `AT_TIMESTAMP` | Replaying from a specific point in time |

## Error Handling

- **Connection failures**: Automatic retry with 1-second backoff
- **Expired iterator**: Automatic refresh
- **Throttling**: Respects Kinesis rate limits with backoff
- **Empty responses**: Brief pause when caught up to avoid excessive API calls

## Performance Considerations

- **Batch size**: Larger batches reduce API calls but increase latency
- **Shard count**: Each shard has a read limit of 2 MB/s or 5 transactions/s
- **Iterator expiration**: Iterators expire after 5 minutes of inactivity

## See Also

- [aws_kinesis output](../outputs/aws_kinesis.md) - Put records to Kinesis streams
- [aws_sqs](aws_sqs.md) - AWS SQS input
- [AWS Kinesis Documentation](https://docs.aws.amazon.com/kinesis/)
