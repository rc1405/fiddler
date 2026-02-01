# Input
The source of messages for fiddler are described as inputs.  There may only be one input per configuration item.  Input key names must be uniquely registered across all available fiddler inputs.

```yml
input:
  label: standard in
  stdin: {}
```

## Available Inputs

| Input | Description | Feature Flag |
|-------|-------------|--------------|
| [file](./file.md) | Read from local files | - |
| [http_server](./http_server.md) | Receive data via HTTP POST requests | `http_server` |
| [stdin](./stdin.md) | Read from standard input | - |
| [aws_s3](./aws_s3.md) | Read from AWS S3 buckets | `aws` |
| [aws_sqs](./aws_sqs.md) | Read from AWS SQS queues | `aws` |