# Output
The sinks of fiddler are described as outputs.  There may only be one output per configuration item.  Output key names must be uniquely registered across all available fiddler outputs.

```yml
output:
  label: standard out
  stdout: {}
```

## Available Outputs

| Output | Description | Feature Flag |
|--------|-------------|--------------|
| [clickhouse](./clickhouse.md) | Send to ClickHouse database | `clickhouse` |
| [drop](./drop.md) | Discard messages | - |
| [elasticsearch](./elasticsearch.md) | Send to Elasticsearch | `elasticsearch` |
| [stdout](./stdout.md) | Write to standard output | - |
| [switch](./switch.md) | Route to different outputs based on conditions | - |
| [aws_sqs](./aws_sqs.md) | Send to AWS SQS queues | `aws` |