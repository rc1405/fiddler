# Configuration

## Basic Syntax

```yml
label: Label for Pipeline
num_threads: 3
input:
  label: standard in
  stdin: {}
processors:
  - noop: {}
  - label: Split lines
    lines: {}
output:
  label: standard out
  std: out
```

### Fields
#### `label`
Descriptive label for the component.  (Pipeline, Input, Processor, Output)    
Type: `string`  
Required: `false`  

#### `num_threads`
Number of processor and output threads to spawn in the pipeline  
Type: `int`  
Required: `false` [Default: number of CPUs]  

#### `input`
Valid input configuration.  See [Inputs](./inputs/About.md)  
Type: `object`  
Required: `true`  

#### `processors`
An Array of processors to perform manipulation of messages.  See [Processors](./processors/About.md)  
Type: `array`  
Required: `true`  

#### `output`
Valid output configuration.  See [Outputs](./outputs/About.md)  
Type: `string`  
Required: `true`  

## Environmental Variables
Fiddler supports handlebars style templating  and will replace values of configuration files with available environmental varialbes.  This is useful for dynamic or sensitive values; such as URLs and passowrds.  
<br>
For example:  

```
output:
  elasticsearch:
    url: {{ ES_URL }}
    username: elastic
    password: {{ ES_PASS }}
    index: flow
    cert_validation: None
    batching_policy:
      size: 100
      duration: 10s
```