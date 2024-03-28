use futures::stream::FuturesOrdered;
use futures::stream::StreamExt;
use serde::Deserialize;
use serde::Serialize;
use serde_yaml::Value;
use inline_colorization::{color_red, color_green, color_reset};

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use fiddler::Error;
use fiddler::Environment;
use fiddler::config::Config;

mod input;
mod output;

#[derive(Deserialize)]
struct Test {
    name: String,
    inputs: Vec<String>,
    expected_outputs: Vec<String>
}

#[derive(Serialize)]
struct Input {
    input: Vec<String>
}

#[derive(Serialize)]
struct Output {
    expected: Vec<String>
}

pub async fn handle_tests(configs: Vec<String>) -> Result<(), Error> {
    input::register_mock_input()?;
    output::register_assert()?;
    
    let mut proc_maps: HashMap<usize, String> = HashMap::new();
    
    let mut environments = Vec::new();
    for c in configs {
        let conf = fs::read_to_string(&c).map_err(|e| Error::ConfigurationItemNotFound(format!("cannot read {}: {}", c, e)))?;
        let config: Config = serde_yaml::from_str(&conf)?;
        let _ = Environment::from_config(&conf)?;
        let _ = config.validate()?;
        
        let path = PathBuf::from(&c);
        let new_filename = path.clone().with_extension("");
        let new_filename_str = new_filename.as_os_str().to_str().ok_or(Error::ConfigurationItemNotFound(format!("cannot read {}", c)))?;
        let test_file = fs::read_to_string(format!("{}_test.yaml", new_filename_str)).map_err(|e| Error::ConfigurationItemNotFound(format!("cannot read test file.  expected as {}_test.yaml: {}", new_filename_str, e)))?;
        
        let tests: Vec<Test> = serde_yaml::from_str(&test_file)?;
        for test in tests {
            let mut env = Environment::from_config(&conf)?;

            let i = Input{
                input: test.inputs.clone()
            };

            let input_value: Value = serde_yaml::to_value(i)?;
            let mut new_input_conf: HashMap<String, Value> = HashMap::new();
            let _ = new_input_conf.insert("mock".into(), input_value);
            env.set_input(&new_input_conf)?;

            let o = Output{
                expected: test.expected_outputs.clone()
            };

            let output_value: Value = serde_yaml::to_value(o)?;
            let mut new_output_conf: HashMap<String, Value> = HashMap::new();
            let _ = new_output_conf.insert("assert".into(), output_value);
            env.set_output(&new_output_conf)?;

            let label = format!("{}: {}", &c, test.name);
            let _ = proc_maps.insert(environments.len(), label.clone());

            env.set_label(Some(label))?;
            environments.push(env);            
        }

    };


    let new_futures = FuturesOrdered::from_iter(environments.iter().map(|e| e.run())).fuse();
    let future_to_await = new_futures.collect::<Vec<Result<(), Error>>>();
    futures::pin_mut!(future_to_await);
    let results = future_to_await.await;
    for (i, r) in results.iter().enumerate() {
        let label = proc_maps.get(&i).expect("Unable to track test");
        match r {
            Ok(_) => {
                println!("{color_green}{label} passed{color_reset}");
            },
            Err(e) => {
                println!("{color_red}{label} failed{color_reset}\n  {e}");
            },
        }
    };
    
    Ok(())
}