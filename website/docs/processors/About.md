# Processors
Processors in fiddler handle all of the data transformation operations on the messages received from [Inputs](../inputs/About.md).  Each Processor receives a single Message and is expected to return one to many messages as a result.  Each result is forward along to processors later in the array; or to the desired output.

```yml
processors:
  - label: lines
    lines: {}
  - label: noop
    noop: {}
  - label: pretty print
    python:
      string: true
      code: |
        import json
        root = json.dumps(json.loads(root), indent=4)
```