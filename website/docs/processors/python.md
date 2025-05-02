# python
Python is the primary source of inline message manipulation provided by Fiddler.  Any third party packages utilized in Python will have to be installed on the OS running fiddler.  

```yml
processors:
    - python:
        string: true
        code: |
            import json
            new_string = f\"python: {root}\"
            root = new_string

```
## Fields
### `string`
Indicate whether or not fiddler should convert the message to a string before invoking the python code.    
Type: `string`  
Required: `false` [default: `false`]

### `code`
The python code to execute  
Type: `string`  
Required: `true` 

## Usage
The message is passed into the python code using a local variable `root`.  The output taken from the executed code is also taken from the local variable `root`.  By default `root` is bytes of the message, unless the argument `string: true` is proved, in which case `root` is converted to a string.