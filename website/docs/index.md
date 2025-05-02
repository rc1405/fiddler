# Fiddler

Fiddler is a stream processor built in rust that uses a yaml based configuration file syntax to map together inputs, processors, and outputs into a stateless data pipeline.

## Configuration 

```
input:
    stdin: {}
pipeline:
  max_in_flight: 10
  processors:
    - python: 
        string: true
        code: |
            import json
            msg = json.loads(root)
            msg['Python'] = 'rocks'
            root = json.dumps(msg)
output:
  switch:
    - check:
        condition: '\"Hello World\" > `5`'
        output:
          validate: 
            expected: []
    - stdout: {}
```

The main inline message manipulation is provided by Python, and requires the Python shared library.

To install the Python shared library on Ubuntu:
`sudo apt install python3-dev`
To install the Python shared library on RPM based distributions (e.g. Fedora, Red Hat, SuSE), install the `python3-devel` package.

Any third party packages utilized in Python will have to be installed on the OS running fiddler.  

The message format in and out is simple, the source of the event is stored in the local variable named `root`; and the output taken from executing the code is expected to be named `root`.  By default `root` is bytes of the message, unless the argument `string: true` is proved, in which case `root` is converted to a string.
<br>
<br>
Conditional execution of steps for processing, or selection of outputs can be obtained through the use of `check` and `switch` plugins.  For processing, it looks like:
<br>
```
- switch:
    - check: 
        condition: '\"Hello World\" <= `5`'
        processors:
            - python: 
                string: true
                code: |
                    import json
                    new_string = f\"python: {root}\"
                    root = new_string
            - echo: {}
    - label: my_cool_mapping
        echo: {}
```
<br>
The condition format utilized [jmespath](https://jmespath.org/specification.html) syntax for evaluation.  As such, this can only be utilized with JSON documents.
