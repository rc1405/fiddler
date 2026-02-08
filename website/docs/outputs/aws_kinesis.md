# aws_kinesis

Put records to an AWS Kinesis Data Stream. This output batches messages and publishes them efficiently using the PutRecords API.

=== "Basic"
    ```yml
    output:
      aws_kinesis:
        stream_name: "my-stream"
    ```

=== "With Partition Key"
    ```yml
    output:
      aws_kinesis:
        stream_name: "events"
        partition_key: "customer_id"
        region: "us-west-2"
    ```

=== "With Credentials"
    ```yml
    output:
      aws_kinesis:
        stream_name: "my-stream"
        region: "us-east-1"
        credentials:
          access_key_id: "${AWS_ACCESS_KEY_ID}"
          secret_access_key: "${AWS_SECRET_ACCESS_KEY}"
    ```

=== "With Batching"
    ```yml
    output:
      aws_kinesis:
        stream_name: "high-volume"
        batch:
          size: 500
          duration: "5s"
    ```

## Fields

### `stream_name`

The name of the Kinesis stream to write to.

Type: `string`
Required: `true`

### `partition_key`

The partition key for shard distribution.

Type: `string`
Required: `false`
Default: Random UUID per record

The partition key determines which shard receives each record. Using a consistent partition key ensures related records go to the same shard and maintain ordering.

### `region`

AWS region where the stream is located.

Type: `string`
Required: `false`

Uses the AWS SDK's default region resolution if not specified.

### `endpoint_url`

Custom endpoint URL for the Kinesis API.

Type: `string`
Required: `false`

Useful for local development with LocalStack.

### `credentials`

Explicit AWS credentials. If not provided, uses the standard AWS credential chain.

Type: `object`
Required: `false`

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `access_key_id` | string | Yes | AWS access key ID |
| `secret_access_key` | string | Yes | AWS secret access key |
| `session_token` | string | No | AWS session token (for temporary credentials) |

### `batch`

Batching policy for grouping records before sending.

Type: `object`
Required: `false`

| Field | Type | Description |
|-------|------|-------------|
| `size` | integer | Maximum records per batch (max: 500) |
| `duration` | string | Maximum time before flush (default: "5s") |

**Note**: Kinesis PutRecords API has a hard limit of 500 records per call. The batch size is automatically capped at 500.

## How It Works

1. Messages are buffered according to the batch policy
2. When a batch is ready, records are published using PutRecords
3. Each record uses the configured partition key (or random UUID)
4. Partial failures are logged but not retried automatically
5. Automatic reconnection on connection failures

## AWS Authentication

The Kinesis output supports two authentication methods:

### Explicit Credentials

```yml
output:
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
        "kinesis:PutRecord",
        "kinesis:PutRecords"
      ],
      "Resource": "arn:aws:kinesis:*:*:stream/my-stream"
    }
  ]
}
```

## Examples

### Basic Stream Publishing

```yml
input:
  http_server:
    port: 8080

processors:
  - fiddlerscript:
      code: |
        let data = parse_json(this);
        data = set(data, "ingested_at", now());
        this = bytes(str(data));

output:
  aws_kinesis:
    stream_name: "events"
```

### Ordered Processing with Partition Key

Use a consistent partition key to maintain ordering for related records:

```yml
input:
  kafka:
    brokers: ["kafka:9092"]
    topics: ["orders"]

output:
  aws_kinesis:
    stream_name: "order-events"
    partition_key: "order_id"  # All events for same order go to same shard
```

### High-Throughput Configuration

```yml
input:
  amqp:
    url: "amqp://localhost"
    queue: "high_volume"
    prefetch_count: 1000

output:
  aws_kinesis:
    stream_name: "analytics"
    batch:
      size: 500  # Maximum allowed
      duration: "1s"
```

### Local Development with LocalStack

```yml
input:
  stdin: {}

output:
  aws_kinesis:
    stream_name: "test-stream"
    region: "us-east-1"
    endpoint_url: "http://localhost:4566"
```

### Cross-Account Access

```yml
output:
  aws_kinesis:
    stream_name: "events"
    region: "eu-west-1"
    credentials:
      access_key_id: "${CROSS_ACCOUNT_ACCESS_KEY}"
      secret_access_key: "${CROSS_ACCOUNT_SECRET_KEY}"
```

## Partition Key Strategies

| Strategy | Use Case |
|----------|----------|
| Random UUID (default) | Even distribution, no ordering guarantee |
| Static value | All records to same shard |
| Entity ID | Ordering within entity (user, order, etc.) |
| Timestamp | Time-based distribution |

## Kinesis Limits

| Limit | Value |
|-------|-------|
| Records per PutRecords | 500 |
| Record size | 1 MB |
| PutRecords payload | 5 MB |
| Write throughput per shard | 1 MB/s or 1,000 records/s |

## Error Handling

- **Partial failures**: Logged with count of failed records
- **Connection failures**: Automatic retry
- **Throttling**: SDK handles backoff automatically
- **Oversized records**: Rejected with error

## Performance Considerations

- **Batch size**: Use 500 for maximum throughput
- **Partition key**: Distribute evenly to avoid hot shards
- **Record size**: Keep records small for better batching
- **Flush interval**: Shorter intervals reduce latency at the cost of more API calls

## See Also

- [aws_kinesis input](../inputs/aws_kinesis.md) - Consume records from Kinesis streams
- [aws_sqs](aws_sqs.md) - AWS SQS output
- [AWS Kinesis Documentation](https://docs.aws.amazon.com/kinesis/)
