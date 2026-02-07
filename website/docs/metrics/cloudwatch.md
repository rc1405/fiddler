# cloudwatch
Send metrics to AWS CloudWatch for centralized monitoring and alerting.

!!! note "Feature Flag Required"
    This metrics backend requires the `aws` feature to be enabled when building fiddler.
    ```bash
    cargo build --features aws
    ```

=== "Basic"
    ```yml
    metrics:
      cloudwatch: {}
    ```

=== "With Namespace"
    ```yml
    metrics:
      cloudwatch:
        namespace: "MyApplication"
        region: "us-east-1"
    ```

=== "With Filtering"
    ```yml
    metrics:
      cloudwatch:
        namespace: "Fiddler"
        include:
          - total_received
          - total_completed
          - throughput_per_sec
        exclude:
          - stale_entries_removed
    ```

=== "Full Example"
    ```yml
    label: Production Pipeline
    metrics:
      cloudwatch:
        namespace: "Fiddler/Production"
        region: "us-west-2"
        credentials:
          access_key_id: "AKIAIOSFODNN7EXAMPLE"
          secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
        dimensions:
          - name: Environment
            value: production
          - name: Service
            value: data-pipeline
        include:
          - total_received
          - total_completed
          - total_process_errors
          - total_output_errors
          - in_flight
          - throughput_per_sec
    input:
      stdin: {}
    processors:
      - noop: {}
    output:
      stdout: {}
    ```

## Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `namespace` | string | `"Fiddler"` | CloudWatch namespace for organizing metrics |
| `region` | string | *AWS default* | AWS region (uses default provider chain if not set) |
| `credentials` | object | *AWS default* | Explicit AWS credentials (uses default provider chain if not set) |
| `include` | array | *all metrics* | List of metric names to include |
| `exclude` | array | *none* | List of metric names to exclude |
| `dimensions` | array | *none* | Additional dimensions to add to all metrics |

### Credentials Object

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `access_key_id` | string | Yes | AWS access key ID |
| `secret_access_key` | string | Yes | AWS secret access key |
| `session_token` | string | No | AWS session token (for temporary credentials) |

### Dimension Object

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | Yes | Dimension name |
| `value` | string | Yes | Dimension value |

## Available Metrics

The following metrics can be included or excluded:

| Metric Name | Type | Description |
|-------------|------|-------------|
| `total_received` | Count | Total messages received from input |
| `total_completed` | Count | Messages successfully processed through all outputs |
| `total_filtered` | Count | Messages intentionally filtered/dropped by processors |
| `total_process_errors` | Count | Messages that encountered processing errors |
| `total_output_errors` | Count | Messages that encountered output errors |
| `streams_started` | Count | Number of streams started |
| `streams_completed` | Count | Number of streams completed |
| `duplicates_rejected` | Count | Duplicate messages rejected |
| `stale_entries_removed` | Count | Stale entries cleaned up from state tracker |
| `in_flight` | Count | Current number of messages being processed |
| `throughput_per_sec` | Count/Second | Current throughput in messages per second |

## Metric Filtering

By default, all metrics are published to CloudWatch. You can control which metrics are sent using the `include` and `exclude` fields:

- **include**: When specified, only the listed metrics are published. If empty or omitted, all metrics are included.
- **exclude**: Metrics listed here are never published, even if they appear in `include`.

The `exclude` filter is applied after `include`, allowing you to include most metrics while excluding specific ones.

### Example: Only Performance Metrics
```yml
metrics:
  cloudwatch:
    include:
      - throughput_per_sec
      - in_flight
```

### Example: All Except Cleanup Metrics
```yml
metrics:
  cloudwatch:
    exclude:
      - stale_entries_removed
      - duplicates_rejected
```

## AWS Authentication

The CloudWatch backend supports multiple authentication methods:

### 1. Default Provider Chain (Recommended)
When no credentials are specified, the AWS SDK uses its default credential provider chain:

1. Environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`)
2. Shared credentials file (`~/.aws/credentials`)
3. IAM role (when running on EC2, ECS, Lambda, etc.)

```yml
metrics:
  cloudwatch:
    region: "us-east-1"
```

### 2. Explicit Credentials
For development or specific use cases, you can provide credentials directly:

```yml
metrics:
  cloudwatch:
    credentials:
      access_key_id: "AKIAIOSFODNN7EXAMPLE"
      secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
```

!!! warning "Security"
    Avoid committing credentials to version control. Use environment variables or IAM roles for production deployments.

## CloudWatch Dimensions

Dimensions allow you to categorize and filter metrics in CloudWatch. Common use cases include:

- Distinguishing between environments (dev, staging, production)
- Identifying specific services or pipelines
- Grouping by region or availability zone

```yml
metrics:
  cloudwatch:
    dimensions:
      - name: Environment
        value: production
      - name: Pipeline
        value: logs-processor
```

All configured dimensions are attached to every metric published.

## IAM Permissions

The CloudWatch backend requires the following IAM permissions:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "cloudwatch:PutMetricData"
      ],
      "Resource": "*"
    }
  ]
}
```

You can restrict the `Resource` to specific namespaces using conditions:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": "cloudwatch:PutMetricData",
      "Resource": "*",
      "Condition": {
        "StringEquals": {
          "cloudwatch:namespace": "Fiddler"
        }
      }
    }
  ]
}
```

## Performance Considerations

- **Non-blocking**: Metrics are published asynchronously using a background task
- **Buffered**: A bounded channel (100 entries) prevents backpressure from affecting pipeline performance
- **Dropped on overflow**: If the channel is full, metrics are dropped rather than blocking the pipeline

## Viewing Metrics in CloudWatch

1. Open the AWS CloudWatch console
2. Navigate to **Metrics** > **All metrics**
3. Select your namespace (default: "Fiddler")
4. Browse or search for specific metric names

### Example CloudWatch Insights Query

```sql
SELECT AVG(throughput_per_sec), MAX(in_flight)
FROM SCHEMA("Fiddler", Environment, Pipeline)
WHERE Environment = 'production'
GROUP BY Pipeline
ORDER BY AVG(throughput_per_sec) DESC
```

## Troubleshooting

### Metrics Not Appearing

1. Verify the `aws` feature is enabled in your build
2. Check IAM permissions for `cloudwatch:PutMetricData`
3. Confirm the correct region is configured
4. Look for error logs indicating CloudWatch API failures

### High Latency

CloudWatch has API rate limits. If you're publishing many metrics frequently:

1. Consider using the `include` filter to reduce metric count
2. Increase the metrics interval in your pipeline configuration
3. Use dimensions to aggregate related data points
